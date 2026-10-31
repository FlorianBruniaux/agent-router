// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::{EntryResult, input::Input};
use router_adapter_api::{content_digest, strict_json_limited};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::Write,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct Writer {
    file: File,
    original_flags: OFlags,
    sequence: u64,
}
impl Drop for Writer {
    fn drop(&mut self) {
        let _ = fcntl_setfl(&self.file, self.original_flags);
    }
}
#[derive(Clone)]
pub(super) struct Output {
    writer: Arc<Mutex<Writer>>,
    pub session_id: String,
    pub submission_sha256: String,
    #[cfg(feature = "qualification-fresh-t02")]
    pub(super) qualification: Option<Value>,
}
impl Output {
    pub(super) fn new(session_id: String, submission_sha256: String) -> EntryResult<Self> {
        let file = File::from(rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 0)?);
        Self::from_file(file, session_id, submission_sha256)
    }
    pub(super) fn from_file(
        file: File,
        session_id: String,
        submission_sha256: String,
    ) -> EntryResult<Self> {
        let flags = fcntl_getfl(&file)?;
        // The duplicate shares fd1's open-file-description. This one-shot command
        // owns all stdout writes until the final clone restores the original nonblocking setting.
        fcntl_setfl(&file, flags | OFlags::NONBLOCK)?;
        Ok(Self {
            writer: Arc::new(Mutex::new(Writer {
                file,
                original_flags: flags,
                sequence: 0,
            })),
            session_id,
            submission_sha256,
            #[cfg(feature = "qualification-fresh-t02")]
            qualification: None,
        })
    }
    pub(super) fn emit(&self, kind: &str, fields: Value) -> EntryResult<()> {
        let mut writer = self.writer.lock().map_err(|_| "output lock")?;
        let mut event = json!({"schema_version":1,"kind":kind,"session_id":self.session_id,"submission_sha256":self.submission_sha256,"sequence":writer.sequence});
        let fields = fields.as_object().ok_or("event fields")?;
        for (k, v) in fields {
            if event.get(k).is_some() {
                return Err("reserved event field".into());
            }
            event[k] = v.clone();
        }
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        if bytes.len() > 1048576 {
            return Err("event limit".into());
        }
        let end = Instant::now() + Duration::from_secs(5);
        let mut sent = 0;
        while sent < bytes.len() {
            let left = end
                .checked_duration_since(Instant::now())
                .ok_or("output timeout")?;
            let timeout = Timespec {
                tv_sec: left.as_secs().try_into()?,
                tv_nsec: left.subsec_nanos().into(),
            };
            let mut fds = [PollFd::new(&writer.file, PollFlags::OUT)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => return Err("output timeout".into()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(e) => return Err(e.into()),
                Ok(_) => {}
            }
            match writer.file.write(&bytes[sent..]) {
                Ok(0) => return Err("output closed".into()),
                Ok(n) => sent += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        writer.sequence += 1;
        Ok(())
    }
    #[cfg(any(test, feature = "qualification-fresh-t02"))]
    pub(super) fn candidate_review(
        &self,
        packet: &router_runtime::FiniteApplicationPresentation,
    ) -> EntryResult<()> {
        self.emit(
            "entry.candidate_review",
            json!({"review":packet.review,"diff":packet.diff,
            "action":"accept_candidate_and_apply_to_private_source",
            "acceptance_provenance":"explicit_local_controller","human_acceptance":null}),
        )
    }
    pub(super) fn resume_review(
        &self,
        packet: &router_runtime::FiniteResumePresentation,
        approve: bool,
    ) -> EntryResult<()> {
        self.emit(
            "resume.review",
            json!({"review":packet.review,"original":packet.original,
            "diff":packet.diff,"interactive_approval":approve}),
        )
    }
    pub(super) fn approval(
        &self,
        input: &mut Input,
        phase: &str,
        binding: &str,
        config: &str,
    ) -> EntryResult<()> {
        let binding = json!({"domain":"finite-entry-approval-v1","phase":phase,"session_id":self.session_id,"submission_sha256":self.submission_sha256,"configuration_sha256":config,"owner_digest":binding});
        #[cfg(feature = "qualification-fresh-t02")]
        let binding = {
            let mut binding = binding;
            if let Some(identity) = &self.qualification {
                binding["qualification"] = identity.clone();
            }
            binding
        };
        let digest = content_digest(&serde_json::to_vec(&binding)?);
        let fields = json!({"phase":phase,"approval_digest":digest});
        #[cfg(feature = "qualification-fresh-t02")]
        let fields = {
            let mut fields = fields;
            if let Some(identity) = &self.qualification {
                fields["qualification"] = identity.clone();
            }
            fields
        };
        self.emit("entry.approval_required", fields)?;
        let raw = input
            .line(16384, Instant::now() + Duration::from_secs(15))
            .map_err(|_| "approval input")?;
        let reply = strict_json_limited(&raw, 16384)?;
        if reply
            != json!({"schema_version":1,"kind":"approve","session_id":self.session_id,"submission_sha256":self.submission_sha256,"phase":phase,"approval_digest":digest})
        {
            return Err("approval mismatch".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        os::{fd::OwnedFd, unix::net::UnixStream},
    };
    fn pair() -> (Output, UnixStream) {
        let (writer, reader) = UnixStream::pair().unwrap();
        (
            Output::from_file(
                File::from(OwnedFd::from(writer)),
                "session".into(),
                "task".into(),
            )
            .unwrap(),
            reader,
        )
    }
    #[test]
    fn inline_review_preserves_maximum_text_and_binding() {
        // Use worst-case JSON escaping at the full permitted patch size.
        let diff = "\x1b".repeat(131072);
        let review = router_runtime::FiniteApplicationReview {
            run_id: "entry".into(),
            source_revision: "1".repeat(40),
            source_tree: "2".repeat(40),
            candidate_tree: "3".repeat(40),
            artifact_sha256: content_digest(diff.as_bytes()),
            technical_evidence_sha256: "4".repeat(64),
            technical_record_sha256: "5".repeat(64),
            output: "lib.rs".into(),
            preimage_sha256: "6".repeat(64),
            candidate_sha256: "7".repeat(64),
            approval_digest: "8".repeat(64),
        };
        let (output, reader) = pair();
        let expected = review.clone();
        let producer = std::thread::spawn(move || {
            output
                .candidate_review(&router_runtime::FiniteApplicationPresentation {
                    review: review.clone(),
                    diff: diff.clone(),
                })
                .unwrap();
            output
                .resume_review(
                    &router_runtime::FiniteResumePresentation {
                        review: router_runtime::FiniteResumeReview {
                            schema_version: 1,
                            run_id: "entry".into(),
                            state: router_runtime::FiniteResumeState::PreimageIntact,
                            original_approval: review.approval_digest.clone(),
                            history_sha256: "9".repeat(64),
                            source_snapshot: "a".repeat(64),
                            observed_sha256: review.preimage_sha256.clone(),
                            candidate_sha256: review.candidate_sha256.clone(),
                            approval_digest: Some("b".repeat(64)),
                        },
                        original: review,
                        diff,
                    },
                    true,
                )
                .unwrap();
        });
        let rows: Vec<String> = BufReader::new(reader).lines().map(Result::unwrap).collect();
        producer.join().unwrap();
        assert_eq!(rows.len(), 2);
        for (i, raw) in rows.iter().enumerate() {
            assert!(raw.len() < 1048576);
            assert!(!raw.contains('\x1b'));
            let row: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(row["sequence"], i);
            assert_eq!(row["diff"].as_str().unwrap().len(), 131072);
            assert_eq!(
                content_digest(row["diff"].as_str().unwrap().as_bytes()),
                expected.artifact_sha256
            );
            assert_eq!(
                row[if i == 0 { "review" } else { "original" }],
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(
                row["kind"],
                if i == 0 {
                    "entry.candidate_review"
                } else {
                    "resume.review"
                }
            );
        }
    }
    #[test]
    fn output_sequence_is_shared_and_original_flags_are_restored() {
        let (output, parent) = pair();
        let file = output.writer.lock().unwrap().file.try_clone().unwrap();
        let flags = output.writer.lock().unwrap().original_flags;
        assert!(
            rustix::io::fcntl_getfd(&file)
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        assert!(fcntl_getfl(&file).unwrap().contains(OFlags::NONBLOCK));
        output.emit("first", json!({})).unwrap();
        let clone = output.clone();
        drop(output);
        clone.emit("second", json!({})).unwrap();
        drop(clone);
        // File::write may add macOS O_NOSIGPIPE; it is independent of F_SETFL.
        assert_eq!(
            fcntl_getfl(&file).unwrap().contains(OFlags::NONBLOCK),
            flags.contains(OFlags::NONBLOCK)
        );
        drop(file);
        let rows: Vec<Value> = BufReader::new(parent)
            .lines()
            .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["sequence"], 0);
        assert_eq!(rows[1]["sequence"], 1);
    }
    #[test]
    fn blocked_parent_output_is_bounded() {
        let (output, _parent) = pair();
        let begin = Instant::now();
        let error = output
            .emit("large", json!({"data":"x".repeat(900000)}))
            .unwrap_err();
        assert_eq!(error.to_string(), "output timeout");
        assert!(begin.elapsed() < Duration::from_secs(8));
    }
    #[test]
    fn approval_requires_the_exact_fresh_challenge() {
        for phase in ["runtime", "accept_apply"] {
            for change in 0..7 {
                let (output, parent_out) = pair();
                let (child, parent_in) = UnixStream::pair().unwrap();
                let mut input = Input::from_file(File::from(OwnedFd::from(child)));
                let parent = std::thread::spawn(move || {
                    let mut line = String::new();
                    BufReader::new(parent_out).read_line(&mut line).unwrap();
                    let frame: Value = serde_json::from_str(&line).unwrap();
                    let mut response = json!({"schema_version":1,"kind":"approve","session_id":"session","submission_sha256":"task","phase":phase,"approval_digest":frame["approval_digest"]});
                    match change {
                        1 => response["session_id"] = json!("old"),
                        2 => response["submission_sha256"] = json!("other"),
                        3 => response["phase"] = json!("diagnostic"),
                        4 => response["approval_digest"] = json!("wrong"),
                        5 => response["permission"] = json!("allow"),
                        6 => return,
                        _ => {}
                    }
                    writeln!(&parent_in, "{response}").unwrap();
                });
                let result = output.approval(&mut input, phase, "owner", "config");
                assert_eq!(result.is_ok(), change == 0, "case {change}");
                parent.join().unwrap();
            }
        }
    }
    #[cfg(feature = "qualification-fresh-t02")]
    #[test]
    fn qualification_identity_is_visible_and_bound_to_both_challenges() {
        let mut previous = None;
        for phase in ["diagnostic", "runtime"] {
            for identity in [
                json!({"adapter_sha256":"first"}),
                json!({"adapter_sha256":"second"}),
            ] {
                let (mut output, parent_out) = pair();
                output.qualification = Some(identity.clone());
                let (child, parent_in) = UnixStream::pair().unwrap();
                let mut input = Input::from_file(File::from(OwnedFd::from(child)));
                let parent = std::thread::spawn(move || {
                    let mut line = String::new();
                    BufReader::new(parent_out).read_line(&mut line).unwrap();
                    let frame: Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(frame["qualification"], identity);
                    let response = json!({"schema_version":1,"kind":"approve","session_id":"session","submission_sha256":"task","phase":phase,"approval_digest":frame["approval_digest"]});
                    writeln!(&parent_in, "{response}").unwrap();
                    frame["approval_digest"].clone()
                });
                output
                    .approval(&mut input, phase, "owner", "config")
                    .unwrap();
                let digest = parent.join().unwrap();
                assert_ne!(previous, Some(digest.clone()));
                previous = Some(digest);
            }
        }
    }
}
