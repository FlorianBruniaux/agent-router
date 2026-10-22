use crate::CliError;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use std::{collections::VecDeque, fs::File, io::Read, time::Instant};

pub(super) struct Input {
    file: File,
    buffered: VecDeque<u8>,
}
impl Input {
    pub(super) fn stdin() -> Result<Self, CliError> {
        let fd = rustix::io::fcntl_dupfd_cloexec(std::io::stdin(), 0)
            .map_err(|_| CliError::Product("finite_entry.input"))?;
        Ok(Self {
            file: File::from(fd),
            buffered: VecDeque::new(),
        })
    }
    #[cfg(test)]
    pub(super) fn from_file(file: File) -> Self {
        Self {
            file,
            buffered: VecDeque::new(),
        }
    }
    pub(super) fn line(&mut self, max: usize, deadline: Instant) -> Result<Vec<u8>, CliError> {
        let fail = || CliError::Product("finite_entry.input");
        let mut out = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return Err(fail());
            }
            while let Some(b) = self.buffered.pop_front() {
                if b == b'\n' {
                    return if out.is_empty() { Err(fail()) } else { Ok(out) };
                }
                if out.len() == max {
                    return Err(fail());
                }
                out.push(b);
            }
            let left = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(fail)?;
            let timeout = Timespec {
                tv_sec: left.as_secs().try_into().map_err(|_| fail())?,
                tv_nsec: left.subsec_nanos().into(),
            };
            let mut fds = [PollFd::new(&self.file, PollFlags::IN)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => return Err(fail()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => return Err(fail()),
                Ok(_) => {}
            }
            let mut bytes = [0; 4096];
            let n = self.file.read(&mut bytes).map_err(|_| fail())?;
            if n == 0 {
                return Err(fail());
            }
            self.buffered.extend(&bytes[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn control_input_cannot_survive_an_exec() {
        let input = Input::stdin().unwrap_or_else(|_| panic!("input"));
        assert!(
            rustix::io::fcntl_getfd(&input.file)
                .unwrap()
                .contains(rustix::io::FdFlags::CLOEXEC)
        );
        let status = std::process::Command::new("/bin/sh")
            .args(["-c", "test ! -e /dev/fd/$ENTRY_FD"])
            .env_clear()
            .env("ENTRY_FD", input.file.as_raw_fd().to_string())
            .status()
            .unwrap();
        assert!(status.success());
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use std::{
        io::Write,
        os::{fd::OwnedFd, unix::net::UnixStream},
        time::Duration,
    };
    fn pair() -> (Input, UnixStream) {
        let (reader, writer) = UnixStream::pair().unwrap();
        (Input::from_file(File::from(OwnedFd::from(reader))), writer)
    }
    #[test]
    fn frames_preserve_boundaries_and_reject_partial_eof_or_oversize() {
        let (mut input, mut parent) = pair();
        parent.write_all(b"one\ntwo\ntail").unwrap();
        drop(parent);
        let end = Instant::now() + Duration::from_secs(1);
        assert_eq!(input.line(3, end).ok(), Some(b"one".to_vec()));
        assert_eq!(input.line(3, end).ok(), Some(b"two".to_vec()));
        assert!(input.line(4, end).is_err());
        let (mut input, mut parent) = pair();
        parent.write_all(b"four\n").unwrap();
        assert!(input.line(3, end).is_err());
    }
    #[test]
    fn buffered_reply_cannot_bypass_an_expired_deadline() {
        let (mut input, mut parent) = pair();
        parent.write_all(b"one\ntwo\n").unwrap();
        assert!(
            input
                .line(3, Instant::now() + Duration::from_secs(1))
                .is_ok()
        );
        assert!(
            input
                .line(3, Instant::now() - Duration::from_millis(1))
                .is_err()
        );
    }
    #[test]
    fn held_open_silent_parent_has_an_actual_deadline() {
        let (mut input, _parent) = pair();
        let begin = Instant::now();
        assert!(input.line(3, begin + Duration::from_millis(30)).is_err());
        assert!(begin.elapsed() < Duration::from_secs(2));
    }
}
