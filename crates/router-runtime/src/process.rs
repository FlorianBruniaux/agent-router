use crate::{Result, RuntimeError};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use router_types::{TransportOutcome, TransportTermination};
use std::{
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Notify,
};

#[derive(Clone, Default)]
pub struct Cancellation {
    inner: Arc<CancelState>,
}
#[derive(Default)]
struct CancelState {
    cancelled: AtomicBool,
    notify: Notify,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }
    pub(crate) async fn cancelled(&self) {
        loop {
            let notified = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}
pub(crate) struct Capture {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub outcome: TransportOutcome,
    pub output_limited: bool,
}
struct GroupGuard(Option<u32>);
impl GroupGuard {
    fn signal(&self, signal: Signal) {
        if let Some(pid) = self.0.and_then(|id| i32::try_from(id).ok()) {
            let _ = killpg(Pid::from_raw(pid), signal);
        }
    }
}
impl Drop for GroupGuard {
    fn drop(&mut self) {
        self.signal(Signal::SIGKILL);
    }
}

// A detached Tokio task would retain the pipe after cancellation of capture.
// Abort and drop it even when the surrounding future is dropped abruptly.
struct InputGuard(tokio::task::JoinHandle<std::io::Result<Option<tokio::process::ChildStdin>>>);
impl Drop for InputGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) async fn capture(
    mut command: Command,
    input: Vec<u8>,
    timeout_ms: u64,
    max_bytes: usize,
    grace_ms: u64,
    cancel: &Cancellation,
) -> Result<Capture> {
    if timeout_ms == 0
        || timeout_ms > 86_400_000
        || max_bytes == 0
        || max_bytes > 16 * 1024 * 1024
        || grace_ms > 5_000
    {
        return Err(RuntimeError::Limit);
    }
    if cancel.is_cancelled() {
        return Err(RuntimeError::Cancelled);
    }
    let supervised = command.as_std().get_envs().any(|(key, value)| {
        key == "ROUTER_RUNTIME_SUPERVISION" && value == Some(std::ffi::OsStr::new("framed-v1"))
    });
    if supervised && input.len() > 16 * 1024 * 1024 {
        return Err(RuntimeError::Limit);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(!supervised);
    command.process_group(0);
    let start = Instant::now();
    let mut child = command.spawn()?;
    let child_id = child.id().ok_or(RuntimeError::Process)?;
    let mut guard = GroupGuard((!supervised).then_some(child_id));
    let mut stdin = child.stdin.take().ok_or(RuntimeError::Process)?;
    let stdout = child.stdout.take().ok_or(RuntimeError::Process)?;
    let stderr = child.stderr.take().ok_or(RuntimeError::Process)?;
    let write = InputGuard(tokio::spawn(async move {
        if supervised {
            stdin.write_all(b"ARLIFE01").await?;
            stdin.write_all(&(input.len() as u64).to_be_bytes()).await?;
            stdin.write_all(&timeout_ms.to_be_bytes()).await?;
            stdin.write_all(&grace_ms.to_be_bytes()).await?;
        }
        stdin.write_all(&input).await?;
        if supervised {
            Ok(Some(stdin))
        } else {
            Ok(None)
        }
    }));
    let count = Arc::new(AtomicUsize::new(0));
    let overflow = Cancellation::default();
    let out_task = tokio::spawn(read_bounded(
        stdout,
        count.clone(),
        max_bytes,
        overflow.clone(),
    ));
    let err_task = tokio::spawn(read_bounded(stderr, count, max_bytes, overflow.clone()));
    let termination = tokio::select! {
        status=wait_without_reaping(child_id)=> {status?; TransportTermination::Exited},
        _=tokio::time::sleep(Duration::from_millis(timeout_ms))=>TransportTermination::TimedOut,
        _=cancel.cancelled()=>TransportTermination::Cancelled,
        _=overflow.cancelled()=>TransportTermination::Killed,
    };
    // Closing this pipe asks the surviving watchdog to clean its worker group.
    // Never signal the watchdog first: SIGKILL would bypass its own cleanup.
    drop(write);
    // Raw probes retain group cleanup. The leader is still unreaped, so the
    // identifier cannot refer to an unrelated newly allocated process group.
    guard.signal(Signal::SIGTERM);
    if !supervised && grace_ms > 0 {
        tokio::time::sleep(Duration::from_millis(grace_ms)).await;
    }
    guard.signal(Signal::SIGKILL);
    guard.0 = None; // No signal, including Drop, is legal after reaping.
    let status = tokio::time::timeout(Duration::from_millis(grace_ms + 3_000), child.wait())
        .await
        .map_err(|_| RuntimeError::Process)??;
    let stdout = tokio::time::timeout(Duration::from_secs(2), out_task)
        .await
        .map_err(|_| RuntimeError::Process)?
        .map_err(|_| RuntimeError::Process)??;
    let stderr = tokio::time::timeout(Duration::from_secs(2), err_task)
        .await
        .map_err(|_| RuntimeError::Process)?
        .map_err(|_| RuntimeError::Process)??;
    Ok(Capture {
        stdout,
        stderr,
        outcome: TransportOutcome {
            termination,
            exit_code: status.code(),
            elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        output_limited: overflow.is_cancelled(),
    })
}
async fn wait_without_reaping(id: u32) -> Result<()> {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let pid = Pid::from_raw(i32::try_from(id).map_err(|_| RuntimeError::Process)?)
        .ok_or(RuntimeError::Process)?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        ) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) | Err(rustix::io::Errno::INTR) => {}
            Err(_) => return Err(RuntimeError::Process),
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn read_bounded(
    mut read: impl AsyncRead + Unpin,
    count: Arc<AtomicUsize>,
    limit: usize,
    overflow: Cancellation,
) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = read.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let before = count.fetch_add(n, Ordering::SeqCst);
        let keep = limit.saturating_sub(before).min(n);
        result.extend_from_slice(&buffer[..keep]);
        if keep < n {
            overflow.cancel();
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn term_resistant_descendants_are_killed_as_a_group() {
        let temp = tempfile::tempdir().unwrap();
        let heartbeat = temp.path().join("heartbeat");
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("trap '' TERM; /bin/sh -c 'trap \"\" TERM; while :; do printf x >> heartbeat; sleep 0.01; done' & wait").current_dir(temp.path());
        let out = capture(command, vec![], 150, 1024, 30, &Cancellation::default())
            .await
            .unwrap();
        assert_eq!(out.outcome.termination, TransportTermination::TimedOut);
        assert_stopped(&heartbeat).await;
    }
    #[tokio::test]
    async fn dropping_supervision_future_kills_group() {
        let temp = tempfile::tempdir().unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("while :; do printf x >> heartbeat; sleep 0.01; done")
            .current_dir(temp.path());
        let task = tokio::spawn(async move {
            capture(command, vec![], 30_000, 1024, 10, &Cancellation::default()).await
        });
        for _ in 0..100 {
            if std::fs::metadata(temp.path().join("heartbeat")).is_ok_and(|m| m.len() > 0) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        task.abort();
        let _ = task.await;
        assert_stopped(&temp.path().join("heartbeat")).await;
    }
    async fn assert_stopped(path: &std::path::Path) {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let before = std::fs::metadata(path).unwrap().len();
        assert!(before > 0, "the descendant must actually have started");
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(
            std::fs::metadata(path).unwrap().len(),
            before,
            "descendant continued executing after group termination"
        );
    }
}
