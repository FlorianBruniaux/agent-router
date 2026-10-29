// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use super::EntryResult;
#[cfg(target_os = "linux")]
use std::{
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
pub(super) fn run(mut command: Command, until: Instant) -> EntryResult<()> {
    if Instant::now() >= until {
        return Err("qualification deadline exhausted".into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = command.spawn()?;
    let pid = rustix::process::Pid::from_child(&child);
    let status = loop {
        if Instant::now() >= until {
            break None;
        }
        use rustix::process::{WaitId, WaitIdOptions as W, waitid};
        match waitid(WaitId::Pid(pid), W::EXITED | W::NOHANG | W::NOWAIT) {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            _ => break None,
        }
    };
    // Keep the leader waitable until its process group has been stopped.
    let killed = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    let exit = finish(&mut child, killed)?;
    if status.and_then(|s| s.exit_status()) != Some(0) || !exit.success() {
        return Err("qualification child failed or timed out".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn finish(
    child: &mut std::process::Child,
    killed: rustix::io::Result<()>,
) -> EntryResult<std::process::ExitStatus> {
    if killed.is_err_and(|e| e != rustix::io::Errno::SRCH) {
        // No blocking wait after a refused signal. The qualification cell owner
        // must tear down the whole cell after this inconclusive result.
        return Err("qualification child cleanup".into());
    }
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= end {
            return Err("qualification child reap timeout".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn run(
    _command: std::process::Command,
    _deadline: std::time::Instant,
) -> EntryResult<()> {
    Err("fresh qualification requires Linux".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn staging_that_exhausted_the_deadline_cannot_spawn_a_checker() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("marker");
        let mut command = Command::new("/usr/bin/touch");
        command.arg(&marker);
        assert!(run(command, Instant::now() - Duration::from_secs(1)).is_err());
        assert!(!marker.exists());
    }
    #[test]
    fn refused_signal_returns_unknown_without_waiting_for_live_child() {
        let mut child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let begin = Instant::now();
        let result = finish(&mut child, Err(rustix::io::Errno::PERM));
        let elapsed = begin.elapsed();
        let live = child.try_wait().unwrap().is_none();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(result.is_err());
        assert!(live);
        assert!(elapsed < Duration::from_secs(1));
    }
    fn ready_pid(path: &std::path::Path) -> i32 {
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(raw) = std::fs::read_to_string(path) {
                if let Ok(pid) = raw.trim().parse() {
                    return pid;
                }
            }
            assert!(Instant::now() < end, "child did not publish its PID");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn missing_process(error: &std::io::Error) -> bool {
        error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
    }
    #[test]
    fn disappeared_proc_entry_is_absent_but_access_errors_are_not() {
        for code in [libc::ENOENT, libc::ESRCH] {
            assert!(missing_process(&std::io::Error::from_raw_os_error(code)));
        }
        for code in [libc::EPERM, libc::EACCES, libc::EIO] {
            assert!(!missing_process(&std::io::Error::from_raw_os_error(code)));
        }
    }
    fn executable(pid: i32) -> bool {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => {
                let state = stat.rsplit_once(") ").unwrap().1.chars().next().unwrap();
                !matches!(state, 'Z' | 'X')
            }
            Err(error) if missing_process(&error) => false,
            Err(error) => panic!("process witness failed: {error}"),
        }
    }
    #[test]
    fn group_descendants_stop_after_success_failure_and_timeout() {
        for code in [Some(0), Some(9), None] {
            let d = tempfile::tempdir().unwrap();
            let leader_path = d.path().join("leader");
            let descendant_path = d.path().join("descendant");
            let release = d.path().join("release");
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "sleep 30 & echo $! > descendant; echo $$ > leader; while [ ! -f release ]; do sleep 0.01; done; exit \"$1\"", "fixture"])
                .arg(code.unwrap_or(0).to_string()).current_dir(d.path());
            let runner =
                std::thread::spawn(move || run(command, Instant::now() + Duration::from_secs(3)));
            let leader = ready_pid(&leader_path);
            let descendant = ready_pid(&descendant_path);
            // The observer is outside the runner's process group. It releases
            // the leader only after witnessing both real processes alive.
            assert!(executable(leader));
            assert!(executable(descendant));
            if code.is_some() {
                std::fs::write(release, b"release").unwrap();
            }
            let result = runner.join().unwrap();
            assert_eq!(result.is_ok(), code == Some(0));
            let end = Instant::now() + Duration::from_secs(1);
            while executable(descendant) && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(!executable(leader));
            assert!(
                !executable(descendant),
                "same-group descendant still executable"
            );
            // A zombie is stopped, not claimed reaped by this non-parent.
            // The outer qualification cell owns final descendant reaping.
        }
    }
    #[test]
    fn spawn_failure_refuses_and_null_output_does_not_block() {
        assert!(
            run(
                Command::new("/no-such-fresh-checker"),
                Instant::now() + Duration::from_secs(1)
            )
            .is_err()
        );
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "dd if=/dev/zero bs=65536 count=4; dd if=/dev/zero bs=65536 count=4 >&2",
        ]);
        assert!(run(command, Instant::now() + Duration::from_secs(2)).is_ok());
    }
    #[test]
    fn child_success_requires_real_execution_and_failed_exit_refuses() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("marker");
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "printf done > \"$1\"", "fixture"])
            .arg(&marker);
        let result = run(c, Instant::now() + Duration::from_secs(2));
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(std::fs::read(marker).unwrap(), b"done");
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "exit 9"]);
        assert!(run(c, Instant::now() + Duration::from_secs(2)).is_err());
    }
    #[test]
    fn timed_out_child_is_stopped_before_returning_unknown() {
        let d = tempfile::tempdir().unwrap();
        let marker = d.path().join("pid");
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "echo $$ > \"$1\"; sleep 30", "fixture"])
            .arg(&marker);
        assert!(run(c, Instant::now() + Duration::from_millis(100)).is_err());
        assert!(marker.exists(), "real owned child must have started");
        let pid: i32 = std::fs::read_to_string(marker)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(
            rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).unwrap()),
            Err(rustix::io::Errno::SRCH)
        );
    }
}

#[cfg(all(test, not(target_os = "linux")))]
#[test]
fn unsupported_host_never_starts_the_child() {
    let d = tempfile::tempdir().unwrap();
    let marker = d.path().join("marker");
    let mut c = std::process::Command::new("/usr/bin/touch");
    c.arg(&marker);
    assert!(
        run(
            c,
            std::time::Instant::now() + std::time::Duration::from_secs(2)
        )
        .is_err()
    );
    assert!(!marker.exists());
}
