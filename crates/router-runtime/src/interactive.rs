//! Provider-free interactive transport seam. A ping is data, not tool authority.
use crate::{
    ApprovedRun, Cancellation, PendingRun, Result, Runtime, RuntimeError, SandboxPolicy,
    SandboxSupervisor,
    process::{Capture, StreamReader, wait_without_reaping},
};
use router_adapter_api::{
    AdapterError, HarnessAdapter, InteractiveAdapter, ValidatedExecutionRequest,
    validate_interactive_protocol,
};
use router_execution_decision::ExecutionDecision;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_types::*;
use std::{
    io::{Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{io::AsyncWriteExt, process::Command};

#[derive(Clone)]
pub struct PendingInteractiveRun(PendingRun);
pub struct ApprovedInteractiveRun(ApprovedRun);
impl PendingInteractiveRun {
    pub fn approval_digest(&self) -> &str {
        self.0.approval_digest()
    }
    pub fn summary(&self) -> &RunSummary {
        self.0.summary()
    }
}
impl ApprovedInteractiveRun {
    pub fn summary(&self) -> &RunSummary {
        self.0.summary()
    }
}
impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub async fn review_interactive(
        &self,
        run_id: &str,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        request: InteractiveExecutionRequest,
        plan: WorkflowPlan,
        node_id: &str,
    ) -> Result<PendingInteractiveRun> {
        validate_interactive_protocol(&request.protocol)?;
        // M1 has no tool execution or provider ingress. Widening this requires a
        // separate reviewed worker/broker contract, not another callback label.
        if request.execution.sandbox != ExecutionSandbox::ReadOnly
            || request.execution.tools != ToolSelection::Disabled
            || request
                .execution
                .required_capabilities
                .contains(&Capability::Network)
        {
            return Err(RuntimeError::Invalid(
                "interactive v1 is provider-free and read-only",
            ));
        }
        let receipt = router.route(context).map_err(|_| RuntimeError::Route)?;
        let selection = router
            .validate_receipt(context, &receipt)
            .map_err(|_| RuntimeError::Route)?;
        let decision = ExecutionDecision::deterministic(router, context, &selection)
            .map_err(|_| RuntimeError::Route)?;
        self.review_internal(
            run_id,
            router,
            context,
            &decision,
            request.execution,
            plan,
            node_id,
            None,
            None,
            Some(request.protocol),
        )
        .await
        .map(PendingInteractiveRun)
    }
    pub fn approve_interactive(
        &self,
        pending: PendingInteractiveRun,
        digest: &str,
    ) -> Result<ApprovedInteractiveRun> {
        self.approve(pending.0, digest).map(ApprovedInteractiveRun)
    }
    pub async fn execute_interactive(
        &self,
        approved: ApprovedInteractiveRun,
        adapter: &dyn InteractiveAdapter,
        supervisor: &dyn SandboxSupervisor,
        cancel: Cancellation,
    ) -> Result<RunCompletion> {
        let protocol = approved
            .0
            .pending
            .interactive
            .clone()
            .ok_or(RuntimeError::Approval)?;
        let adapter = AdapterBridge { adapter, protocol };
        let supervisor = ProviderFreeSupervisor { inner: supervisor };
        Box::pin(self.execute_mode(approved.0, &adapter, &supervisor, cancel, None)).await
    }
}

/// Interactive v1 has no credential broker. Refuse canonical provider hosts
/// and executable IDs, never materialize host state, and remove sensitive env
/// values from every command, including native-version probes. A custom host
/// ID and trusted supervisor mapping are not executable-identity evidence.
struct ProviderFreeSupervisor<'a> {
    inner: &'a dyn SandboxSupervisor,
}
impl ProviderFreeSupervisor<'_> {
    fn validate_program(run: &PreparedRun) -> Result<()> {
        if matches!(run.host.as_str(), "codex" | "claude" | "claude-code")
            || matches!(run.program.as_str(), "codex" | "claude")
        {
            return Err(RuntimeError::Invalid(
                "provider hosts require the brokered interactive path",
            ));
        }
        Ok(())
    }
    fn strip_sensitive_environment(command: &mut Command) {
        for name in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "HOME",
        ] {
            command.env_remove(name);
        }
    }
    fn command_without_credentials(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy<'_>,
        interactive: bool,
    ) -> Result<Command> {
        Self::validate_program(run)?;
        let mut command = if interactive {
            self.inner.interactive_command(run, policy)?
        } else {
            self.inner.command(run, policy)?
        };
        Self::strip_sensitive_environment(&mut command);
        Ok(command)
    }
}
impl SandboxSupervisor for ProviderFreeSupervisor<'_> {
    fn create_host_state(
        &self,
        _store: &std::path::Path,
        run: &PreparedRun,
    ) -> Result<Option<crate::HostState>> {
        Self::validate_program(run)?;
        Ok(None)
    }
    fn command(&self, run: &PreparedRun, policy: &SandboxPolicy<'_>) -> Result<Command> {
        self.command_without_credentials(run, policy, false)
    }
    fn interactive_command(
        &self,
        run: &PreparedRun,
        policy: &SandboxPolicy<'_>,
    ) -> Result<Command> {
        self.command_without_credentials(run, policy, true)
    }
    fn pid_namespace_isolated(&self) -> bool {
        self.inner.pid_namespace_isolated()
    }
    fn parent_lifetime_enforced(&self) -> bool {
        self.inner.parent_lifetime_enforced()
    }
    fn resource_limits_enforced(&self) -> bool {
        self.inner.resource_limits_enforced()
    }
    fn memory_enforcement(&self) -> MemoryEnforcement {
        self.inner.memory_enforcement()
    }
}

// Internal reuse of the batch authorization gates, never a public conversion
// from a batch approval to an interactive run. The protocol is digest-bound.
struct AdapterBridge<'a> {
    adapter: &'a dyn InteractiveAdapter,
    protocol: InteractiveProtocol,
}
impl HarnessAdapter for AdapterBridge<'_> {
    fn host(&self) -> &'static str {
        self.adapter.host()
    }
    fn requires_native_version(&self) -> bool {
        self.adapter.requires_native_version()
    }
    fn validate_native_version(&self, stdout: &[u8]) -> std::result::Result<(), AdapterError> {
        self.adapter.validate_native_version(stdout)
    }
    fn supports_native_version_pin(&self) -> bool {
        self.adapter.supports_native_version_pin()
    }
    fn validate_native_version_pin(
        &self,
        stdout: &[u8],
        expected: &str,
    ) -> std::result::Result<(), AdapterError> {
        self.adapter.validate_native_version_pin(stdout, expected)
    }
    fn prepare(
        &self,
        request: &ValidatedExecutionRequest,
    ) -> std::result::Result<PreparedRun, AdapterError> {
        let prepared = self.adapter.prepare_interactive(request, &self.protocol)?;
        if prepared.protocol != self.protocol {
            return Err(AdapterError::Invalid("interactive prepared binding"));
        }
        Ok(prepared.launch)
    }
    fn decode_stream(
        &self,
        _: &[u8],
        _: TransportOutcome,
        _: usize,
    ) -> std::result::Result<RuntimeEvidence, AdapterError> {
        // A synthetic ping cannot certify a native model or tool execution.
        Err(AdapterError::Unsupported(
            "interactive synthetic transport only",
        ))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TurnState {
    Active,
    ReplyPending,
    Replied,
    Complete,
}
struct Session<'a> {
    protocol: &'a InteractiveProtocol,
    state: TurnState,
    messages: usize,
}
impl<'a> Session<'a> {
    fn new(protocol: &'a InteractiveProtocol) -> Self {
        Self {
            protocol,
            state: TurnState::Active,
            messages: 0,
        }
    }
    fn receive(&mut self, bytes: &[u8]) -> Result<Option<Vec<u8>>> {
        self.messages += 1;
        if self.messages > self.protocol.max_messages {
            return Err(AdapterError::OutputLimit.into());
        }
        let value = router_adapter_api::strict_json(bytes).map_err(|_| AdapterError::Protocol)?;
        let message: InteractiveControllerMessage =
            serde_json::from_value(value).map_err(|_| AdapterError::Protocol)?;
        match message {
            InteractiveControllerMessage::Ping {
                turn_id,
                call_id,
                payload,
            } => {
                if self.state != TurnState::Active
                    || turn_id != self.protocol.turn_id
                    || call_id.is_empty()
                    || call_id.len() > 64
                    || !call_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                {
                    return Err(AdapterError::Protocol.into());
                }
                self.state = TurnState::ReplyPending;
                encode(
                    &InteractiveRuntimeMessage::Pong {
                        turn_id,
                        call_id,
                        payload,
                    },
                    self.protocol,
                )
                .map(Some)
            }
            InteractiveControllerMessage::Completed { turn_id } => {
                if !matches!(self.state, TurnState::Active | TurnState::Replied)
                    || turn_id != self.protocol.turn_id
                {
                    return Err(AdapterError::Protocol.into());
                }
                self.state = TurnState::Complete;
                Ok(None)
            }
        }
    }
    fn replied(&mut self) -> Result<()> {
        if self.state != TurnState::ReplyPending {
            return Err(AdapterError::Protocol.into());
        }
        self.state = TurnState::Replied;
        Ok(())
    }
}
fn encode(message: &InteractiveRuntimeMessage, protocol: &InteractiveProtocol) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(message)?;
    if bytes.len() > protocol.max_frame_bytes {
        return Err(AdapterError::OutputLimit.into());
    }
    bytes.push(b'\n');
    Ok(bytes)
}
async fn write_application(socket: &mut UnixStream, mut bytes: &[u8]) -> Result<()> {
    while !bytes.is_empty() {
        match socket.write(bytes) {
            Ok(0) => return Err(RuntimeError::Process),
            Ok(n) => bytes = &bytes[n..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(2)).await
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn application(
    socket: &mut UnixStream,
    initial: &[u8],
    protocol: &InteractiveProtocol,
    stdout: &mut Vec<u8>,
    stdout_eof: &mut bool,
    count: &AtomicUsize,
    max_bytes: usize,
    overflow: &Cancellation,
) -> Result<()> {
    write_application(socket, initial).await?;
    let mut session = Session::new(protocol);
    let mut frame = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let n = match socket.read(&mut buffer) {
            Ok(0) => {
                *stdout_eof = true;
                return if frame.is_empty() && session.state == TurnState::Complete {
                    Ok(())
                } else {
                    Err(AdapterError::Protocol.into())
                };
            }
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(Duration::from_millis(2)).await;
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        let before = count.fetch_add(n, Ordering::SeqCst);
        let keep = max_bytes.saturating_sub(before).min(n);
        stdout.extend_from_slice(&buffer[..keep]);
        if keep < n {
            overflow.cancel();
            return Err(AdapterError::OutputLimit.into());
        }
        for byte in &buffer[..n] {
            if *byte == b'\n' {
                if let Some(reply) = session.receive(&frame)? {
                    write_application(socket, &reply).await?;
                    session.replied()?;
                }
                frame.clear();
            } else {
                if frame.len() >= protocol.max_frame_bytes {
                    return Err(AdapterError::OutputLimit.into());
                }
                frame.push(*byte);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn capture_interactive(
    mut command: Command,
    run: &PreparedRun,
    protocol: &InteractiveProtocol,
    grace_ms: u64,
    cancel: &Cancellation,
    dispatch: Option<router_execution_decision::DispatchGuard<'_>>,
    budget_dispatch: Option<crate::admission::BudgetDispatchGuard>,
    validate_capabilities: &(dyn Fn() -> Result<()> + Sync),
) -> Result<Capture> {
    validate_interactive_protocol(protocol)?;
    if !command.as_std().get_envs().any(|(key, value)| {
        key == "ROUTER_RUNTIME_SUPERVISION" && value == Some(std::ffi::OsStr::new("framed-v2"))
    }) {
        return Err(RuntimeError::Sandbox);
    }
    if cancel.is_cancelled() {
        return Err(RuntimeError::Cancelled);
    }
    let input =
        String::from_utf8(run.stdin.as_bytes().to_vec()).map_err(|_| AdapterError::Protocol)?;
    let initial = encode(
        &InteractiveRuntimeMessage::Start {
            turn_id: protocol.turn_id.clone(),
            input,
        },
        protocol,
    )?;
    // CLOEXEC socketpair ends are constructed before sandbox entry. Only the
    // child endpoint goes to stdout; stdin remains the dedicated lifetime pipe.
    let (mut application_socket, child_socket) = UnixStream::pair()?;
    application_socket.set_nonblocking(true)?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::from(OwnedFd::from(child_socket)))
        .stderr(Stdio::piped())
        .kill_on_drop(false)
        .process_group(0);
    if let Some(guard) = &dispatch {
        guard.validate_at(crate::now()?)?;
    }
    validate_capabilities()?;
    let start = Instant::now();
    let spawned = command.spawn();
    drop(budget_dispatch);
    drop(dispatch);
    drop(command); // Do not retain the controller's duplex descriptor.
    let mut child = spawned?;
    let child_id = child.id().ok_or(RuntimeError::Process)?;
    let mut life = child.stdin.take().ok_or(RuntimeError::Process)?;
    let stderr = child.stderr.take().ok_or(RuntimeError::Process)?;
    let count = Arc::new(AtomicUsize::new(0));
    let overflow = Cancellation::default();
    let mut err_reader = StreamReader::spawn(
        stderr,
        count.clone(),
        run.max_output_bytes,
        overflow.clone(),
    );
    let mut stdout = Vec::new();
    let mut stdout_eof = false;
    let exchange = async {
        life.write_all(b"ARLIFE02").await?;
        for value in [0_u64, run.timeout_ms, grace_ms] {
            life.write_all(&value.to_be_bytes()).await?;
        }
        application(
            &mut application_socket,
            &initial,
            protocol,
            &mut stdout,
            &mut stdout_eof,
            &count,
            run.max_output_bytes,
            &overflow,
        )
        .await?;
        wait_without_reaping(child_id).await
    };
    let mut protocol_error = None;
    let termination = tokio::select! {
        result = exchange => match result {
            Ok(()) => TransportTermination::Exited,
            Err(error) => { protocol_error = Some(error); TransportTermination::Killed }
        },
        _ = tokio::time::sleep(Duration::from_millis(run.timeout_ms)) => TransportTermination::TimedOut,
        _ = cancel.cancelled() => TransportTermination::Cancelled,
        _ = overflow.cancelled() => TransportTermination::Killed,
    };
    // Dropping this future also drops life. Never kill the surviving watchdog:
    // it must signal and reap its controller before releasing the durable lease.
    drop(life);
    drop(application_socket);
    let status = tokio::time::timeout(Duration::from_millis(grace_ms + 3_000), child.wait())
        .await
        .map_err(|_| RuntimeError::Process)??;
    let (stderr, complete) = err_reader.finish(Duration::from_secs(2)).await;
    if let Some(error) = protocol_error {
        return Err(error);
    }
    Ok(Capture {
        stdout,
        stderr,
        outcome: TransportOutcome {
            termination,
            exit_code: status.code(),
            elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        output_limited: overflow.is_cancelled(),
        output_incomplete: !stdout_eof || !complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct PinnedAdapter;
    impl InteractiveAdapter for PinnedAdapter {
        fn host(&self) -> &'static str {
            "synthetic"
        }
        fn requires_native_version(&self) -> bool {
            true
        }
        fn supports_native_version_pin(&self) -> bool {
            true
        }
        fn validate_native_version(&self, stdout: &[u8]) -> std::result::Result<(), AdapterError> {
            if stdout == b"synthetic 1.0" {
                Ok(())
            } else {
                Err(AdapterError::Protocol)
            }
        }
        fn validate_native_version_pin(
            &self,
            stdout: &[u8],
            expected: &str,
        ) -> std::result::Result<(), AdapterError> {
            self.validate_native_version(stdout)?;
            if expected == "1.0" {
                Ok(())
            } else {
                Err(AdapterError::Protocol)
            }
        }
        fn prepare_interactive(
            &self,
            _: &ValidatedExecutionRequest,
            _: &InteractiveProtocol,
        ) -> std::result::Result<PreparedInteractiveRun, AdapterError> {
            Err(AdapterError::Unsupported("test has no process"))
        }
    }
    #[test]
    fn interactive_bridge_preserves_native_version_and_pin_hooks() {
        let bridge = AdapterBridge {
            adapter: &PinnedAdapter,
            protocol: InteractiveProtocol::default(),
        };
        assert!(bridge.requires_native_version());
        assert!(bridge.supports_native_version_pin());
        assert!(bridge.validate_native_version(b"synthetic 1.0").is_ok());
        assert!(bridge.validate_native_version(b"synthetic 2.0").is_err());
        assert!(
            bridge
                .validate_native_version_pin(b"synthetic 1.0", "1.0")
                .is_ok()
        );
        assert!(
            bridge
                .validate_native_version_pin(b"synthetic 1.0", "2.0")
                .is_err()
        );
    }
    const PING: &[u8] =
        br#"{"type":"ping","turn_id":"turn-1","call_id":"call-1","payload":"hello"}"#;
    #[test]
    fn one_outstanding_callback_and_one_use_reply() {
        let protocol = InteractiveProtocol::default();
        let mut session = Session::new(&protocol);
        assert!(session.receive(PING).unwrap().is_some());
        assert!(
            session.receive(PING).is_err(),
            "a pending callback cannot be replayed"
        );
        session.replied().unwrap();
        assert!(
            session.replied().is_err(),
            "a reply cannot be consumed twice"
        );
        assert!(
            session.receive(PING).is_err(),
            "a completed callback cannot be replayed"
        );
    }
    #[test]
    fn completing_a_pending_callback_or_starting_another_turn_is_refused() {
        let protocol = InteractiveProtocol::default();
        let mut session = Session::new(&protocol);
        session.receive(PING).unwrap();
        assert!(
            session
                .receive(br#"{"type":"completed","turn_id":"turn-1"}"#)
                .is_err()
        );
        assert!(
            session
                .receive(br#"{"type":"start","turn_id":"turn-2"}"#)
                .is_err()
        );
    }
    #[test]
    fn repeated_fields_and_trailing_messages_are_rejected() {
        let protocol = InteractiveProtocol::default();
        let mut session = Session::new(&protocol);
        assert!(
            session
                .receive(br#"{"type":"completed","type":"completed","turn_id":"turn-1"}"#)
                .is_err()
        );
        session
            .receive(br#"{"type":"completed","turn_id":"turn-1"}"#)
            .unwrap();
        assert!(
            session
                .receive(br#"{"type":"completed","turn_id":"turn-1"}"#)
                .is_err()
        );
    }
}
