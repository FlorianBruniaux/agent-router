use super::{
    Entry, EntryResult, config_bytes,
    routing::{self, HOST, RUN, TOOLS},
    transport::Broker,
};
use crate::Outcome;
use router_adapter_api::{
    authorize_execution_with_capability_acceptance, content_digest,
    execution_authorization_digest_with_capability_acceptance,
};
use router_runtime::*;
use router_types::*;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    sync::Arc,
    time::Duration,
};

pub(super) fn mkdir(path: &Path) -> EntryResult<()> {
    fs::DirBuilder::new().mode(0o700).create(path)?;
    Ok(())
}
pub(super) fn save(path: &Path, bytes: &[u8]) -> EntryResult<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
struct FreshRoot {
    path: PathBuf,
    identity: (u64, u64),
}
impl FreshRoot {
    fn create(path: &Path) -> EntryResult<Self> {
        let parent = path.parent().ok_or("root parent")?;
        if fs::canonicalize(parent)? != parent {
            return Err("canonical root parent required".into());
        }
        let p = fs::symlink_metadata(parent)?;
        if !p.is_dir() || p.mode() & 0o022 != 0 || p.uid() != rustix::process::getuid().as_raw() {
            return Err("private root parent required".into());
        }
        mkdir(path)?;
        let m = fs::symlink_metadata(path)?;
        Ok(Self {
            path: path.into(),
            identity: (m.dev(), m.ino()),
        })
    }
    fn check(&self) -> EntryResult<()> {
        let m = fs::symlink_metadata(&self.path)?;
        if !m.is_dir()
            || (m.dev(), m.ino()) != self.identity
            || m.mode() & 0o077 != 0
            || fs::canonicalize(&self.path)? != self.path
        {
            return Err("fresh root changed".into());
        }
        Ok(())
    }
}
pub(super) async fn git(root: &Path, args: &[&str]) -> EntryResult<()> {
    let mut cmd = tokio::process::Command::new("/usr/bin/git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "gc.auto=0",
        "-c",
        "maintenance.auto=false",
        "-C",
    ])
    .arg(root)
    .args(args)
    .env_clear()
    .env("PATH", "/usr/bin:/bin")
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env("GIT_NO_LAZY_FETCH", "1")
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(5), cmd.status()).await??;
    if !status.success() {
        return Err("public source preparation".into());
    }
    Ok(())
}
fn recheck(path: &Path, expected: &[u8], root: &FreshRoot) -> EntryResult<()> {
    root.check()?;
    if config_bytes(path).map_err(|_| "configuration changed")? != expected {
        return Err("configuration changed".into());
    }
    Ok(())
}
pub(super) async fn run(entry: Entry) -> EntryResult<Outcome> {
    let Entry {
        config: c,
        submission: s,
        task,
        raw,
        config_path,
        config_raw,
        mut input,
        output,
        dispatched,
    } = entry;
    if std::env::consts::OS != "linux"
        || std::env::consts::ARCH != "aarch64"
        || fs::read_to_string("/sys/fs/cgroup/memory.max")?.trim() != "8589934592"
        || fs::read_to_string("/sys/fs/cgroup/memory.swap.max")?.trim() != "0"
    {
        return Err("qualified Linux cell required".into());
    }
    let root = FreshRoot::create(&c.root)?;
    save(&root.path.join("submission.json"), &raw)?;
    let config_hash = content_digest(&config_raw);
    save(
        &root.path.join("entry-context.json"),
        &serde_json::to_vec(
            &json!({"schema_version":1,"session_id":output.session_id,"submission_sha256":output.submission_sha256,"configuration_sha256":config_hash,"transport":"local_scripted","model":s.model,"effort":s.effort,"task":task}),
        )?,
    )?;
    let source = root.path.join("candidate");
    mkdir(&source)?;
    for (name, file) in &s.public_files {
        save(&source.join(name), file.content.as_bytes())?;
    }
    git(&source, &["init", "-q"]).await?;
    git(&source, &["add", "--all"]).await?;
    git(
        &source,
        &[
            "-c",
            "user.name=Finite entry",
            "-c",
            "user.email=local@example.invalid",
            "commit",
            "-q",
            "--no-gpg-sign",
            "-m",
            "Explicit public task",
        ],
    )
    .await?;
    let source_hash = source_state(&source).await?;
    let plan = router_workflow::build_plan(
        &task,
        WorkflowPreset::DirectSafe,
        router_workflow::PlanLimits::default(),
    )?;
    let execute = plan
        .nodes
        .iter()
        .find(|n| n.id == "execute")
        .ok_or("execute node")?;
    let owner = execute.owner.clone();
    let execution_budget = execute.budget.clone();
    #[cfg(feature = "qualification-fresh-t02")]
    let verification_timeout = super::qualification::timeout(&plan)?;
    let store = root.path.join("runtime");
    mkdir(&store)?;
    let policy = AdmissionPolicy {
        schema_version: 1,
        global_limit: 1,
        runtimes: BTreeMap::from([(
            "entry".into(),
            AdmissionRuntimePolicy {
                root_sha256: admission_runtime_root_hash(&store)?,
                limit: 1,
            },
        )]),
        providers: BTreeMap::from([("local_fixture".into(), 1)]),
        agents: BTreeMap::from([("worker".into(), 1)]),
        agent_mappings: vec![AdmissionAgentMapping {
            runtime_id: "entry".into(),
            plan_owner: owner,
            candidate_agent: None,
            quota_id: "worker".into(),
        }],
    };
    let registry = root.path.join("admission");
    let admission = AdmissionStore::create(&registry, &policy)?;
    let admission_config = AdmissionConfig {
        schema_version: 1,
        registry,
        expected_policy_sha256: admission.policy_hash().into(),
        runtime_id: "entry".into(),
        session_budget: None,
    };
    let runtime = Runtime::open_with_admission(
        &store,
        RuntimeLimits {
            output_retention: OutputRetentionPolicy::PrivateArtifacts {
                retention_seconds: 3600,
            },
            ..Default::default()
        },
        &admission_config,
    )?;
    let listener = TcpListener::bind(("0.0.0.0", 0))?;
    rustix::net::sockopt::set_socket_reuseaddr(&listener, false)?;
    rustix::net::sockopt::set_socket_reuseport(&listener, false)?;
    let diagnostic = runtime
        .prepare_finite_output_workspace(
            &source,
            "discovery",
            &s.output,
            FiniteOutputMode::ReplaceExisting,
        )
        .await?;
    let seed = routing::seed(&s, &task, diagnostic.path(), &source_hash)?;
    let review = routing::review(
        &c,
        &s,
        &seed,
        &store,
        listener.try_clone()?,
        diagnostic,
        "discovery",
    )?;
    let digest = review.approval_digest().to_owned();
    recheck(&config_path, &config_raw, &root)?;
    output.approval(&mut input, "diagnostic", &digest, &config_hash)?;
    recheck(&config_path, &config_raw, &root)?;
    let discovery = root.path.join("discovery");
    mkdir(&discovery)?;
    let (snapshot, proofs) = review
        .approve(&digest)?
        .discover_linux_tools(&discovery)
        .await?;
    output.emit(
        "entry.diagnostic_complete",
        json!({"worker_attempts":0,"snapshot_hash":snapshot.hash(),"host":HOST}),
    )?;
    let runtime = runtime.with_capability_proof_verifier(Arc::new(proofs));
    let (phase, catalog, router) = routing::phase(&s, task, &config_hash, &execution_budget)?;
    let context = router.context(phase, &snapshot, &catalog)?;
    let receipt = router.route(&context)?;
    output.emit(
        "entry.route",
        json!({"receipt":receipt,"worker_attempts":0}),
    )?;
    let selected = receipt.selected.as_ref().ok_or("no eligible route")?;
    if selected.candidate.model != s.model || selected.candidate.effort != s.effort {
        return Err("route contradicts pinned selection".into());
    }
    let view = runtime
        .prepare_finite_output_workspace(&source, RUN, &s.output, FiniteOutputMode::ReplaceExisting)
        .await?;
    let view_path = view.path().to_owned();
    let request = ExecutionRequest {
        schema_version: 1,
        receipt_id: receipt.receipt_id.clone(),
        candidate_id: selected.candidate.id.clone(),
        phase: receipt.phase,
        host: snapshot.host().harness.clone(),
        model: selected.candidate.model.clone(),
        effort: selected.candidate.effort,
        workspace: view.path().to_str().ok_or("workspace encoding")?.into(),
        source_state: source_hash.clone(),
        scope: vec![s.output.clone()],
        sandbox: ExecutionSandbox::WorkspaceWrite,
        required_capabilities: router.execution_capabilities(&context, selected)?,
        tools: ToolSelection::AllowList(TOOLS.into_iter().map(str::to_owned).collect()),
        agent: None,
        skills: vec![],
        prompt: format!(
            "{}\nRead AGENTS.md and statement.md. Change only the approved output {}.",
            s.task.objective, s.output
        ),
        mode: ExecutionMode::Execute,
        target: ExecutionTarget::ExternalWorker,
        allow_degradation: false,
        timeout_ms: execution_budget.max_elapsed_ms,
        max_output_bytes: 65536,
    };
    let auth = execution_authorization_digest_with_capability_acceptance(
        &request,
        &snapshot,
        &context.request().restrictions,
        context.capability_acceptance(),
    )?;
    let authorized = authorize_execution_with_capability_acceptance(
        request.clone(),
        &snapshot,
        &context.request().restrictions,
        context.capability_acceptance(),
        Some(&auth),
    )?;
    let mut prepared = router_adapter_api::prepared_run(
        &authorized,
        "finite-client",
        vec!["--worker".into()],
        vec![],
    )?;
    prepared.missing_runtime_evidence.clear();
    let client = routing::review(&c, &s, &prepared, &store, listener.try_clone()?, view, RUN)
        .map_err(|_| super::EntryStage("finite_entry.preparation"))?;
    let pending = runtime
        .review_finite_client(RUN, &router, &context, request, plan, "execute", client)
        .await
        .map_err(|_| super::EntryStage("finite_entry.review"))?;
    let digest = pending.approval_digest().to_owned();
    recheck(&config_path, &config_raw, &root)?;
    check_source(&source, &source_hash, &s).await?;
    output.approval(&mut input, "runtime", &digest, &config_hash)?;
    recheck(&config_path, &config_raw, &root)?;
    check_source(&source, &source_hash, &s).await?;
    let approved = runtime.approve_finite_client(pending, &digest)?;
    let verifier = runtime.finite_client_verifier(&approved)?;
    output.emit(
        "entry.dispatch",
        json!({"run_id":RUN,"model":s.model,"effort":s.effort,"receipt_id":receipt.receipt_id}),
    )?;
    let broker = Broker::start(
        listener,
        input,
        output.clone(),
        s.model.to_string(),
        serde_json::to_value(s.effort)?
            .as_str()
            .ok_or("effort encoding")?
            .into(),
    )?;
    dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
    let result = runtime
        .execute_finite_client(
            approved,
            &adapter_codex::CodexAdapter,
            Cancellation::default(),
        )
        .await;
    let broker_result = broker.finish();
    let unchanged = source_state(&source).await? == source_hash
        && s.public_files.iter().all(|(name, f)| {
            read_output(&source.join(name)).is_ok_and(|b| content_digest(&b) == f.sha256)
        });
    root.check()?;
    runtime.verify_integrity()?;
    let artifact = match &result {
        Ok(completion)
            if completion.run.state == RunState::Verifying
                && unchanged
                && broker_result.is_ok() =>
        {
            let patch = verifier.artifact()?;
            if patch.len() > 131072 {
                return Err("captured patch limit".into());
            }
            Some(json!({"sha256":content_digest(&patch),"patch":String::from_utf8(patch)?}))
        }
        _ => None,
    };
    let result_output = if artifact.is_some() {
        let bytes = read_output(&view_path.join(&s.output))?;
        if bytes.len() > 32768 {
            return Err("output limit".into());
        }
        Some(json!({"name":s.output,"content":String::from_utf8(bytes)?}))
    } else {
        None
    };
    let report = json!({"schema_version":1,"source_revision":source_hash,"session_id":output.session_id,"submission_sha256":output.submission_sha256,"run_id":RUN,"result":result.as_ref().ok(),"runtime_error":result.is_err(),"broker_error":broker_result.is_err(),"local_requests":broker_result.as_ref().ok(),"summary":runtime.summary(RUN).ok(),"events":runtime.events().ok(),"attempts":runtime.attempt_evidence(RUN).ok(),"cleanup":runtime.cleanup_evidence(RUN).ok(),"admission":admission.status().ok(),"source_unchanged":unchanged,"artifact":artifact,"output":result_output,"human_acceptance":null,"independent_verification_performed":false,"transport":"local_scripted","provider_http_calls_by_entry":0,"parent_model_activity":"unknown"});
    #[cfg(feature = "qualification-fresh-t02")]
    let report = if c.qualification.is_some() {
        recheck(&config_path, &config_raw, &root)?;
        check_source(&source, &source_hash, &s).await?;
        let qualification = super::qualification::evaluate(
            &root.path,
            &raw,
            &report,
            &verifier,
            &runtime,
            verification_timeout,
        )
        .await?;
        let mut report = report;
        report["qualification"] = qualification;
        report["independent_verification_performed"] = json!(true);
        report["summary"] = serde_json::to_value(runtime.summary(RUN)?)?;
        report["events"] = serde_json::to_value(runtime.events()?)?;
        report
    } else {
        report
    };
    save(
        &root.path.join("entry-result.json"),
        &serde_json::to_vec(&report)?,
    )?;
    output.emit("entry.result", json!({"report":report}))?;
    let completion = result?;
    broker_result?;
    if completion.run.state != RunState::Verifying {
        return Err("worker did not complete transport".into());
    }
    if !unchanged {
        return Err("source changed".into());
    }
    Ok(Outcome {
        output: String::new(),
        exit_code: ExitCode::from(3),
    })
}

pub(super) fn read_output(path: &Path) -> EntryResult<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 || meta.uid() != rustix::process::getuid().as_raw() {
        return Err("output file identity".into());
    }
    let mut bytes = Vec::new();
    file.take(32769).read_to_end(&mut bytes)?;
    if bytes.len() > 32768 {
        return Err("output limit".into());
    }
    Ok(bytes)
}
async fn check_source(
    source: &Path,
    expected: &str,
    submission: &super::Submission,
) -> EntryResult<()> {
    if source_state(source).await? != expected {
        return Err("source changed before dispatch".into());
    }
    for (name, file) in &submission.public_files {
        if content_digest(&read_output(&source.join(name))?) != file.sha256 {
            return Err("public source bytes changed before dispatch".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn result_reader_refuses_links_and_oversized_files() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("output");
        let link = d.path().join("link");
        fs::write(&path, b"public result").unwrap();
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_output(&link).is_err());
        fs::write(&path, vec![b'x'; 32769]).unwrap();
        assert!(read_output(&path).is_err());
        fs::write(&path, b"public result").unwrap();
        assert_eq!(read_output(&path).unwrap(), b"public result");
    }
    #[test]
    fn fresh_root_and_configuration_are_rechecked_without_reopening() {
        let d = tempfile::tempdir().unwrap();
        let parent = fs::canonicalize(d.path()).unwrap();
        let config = parent.join("config");
        fs::write(&config, b"enabled").unwrap();
        let path = parent.join("root");
        let root = FreshRoot::create(&path).unwrap();
        assert!(FreshRoot::create(&path).is_err());
        assert!(recheck(&config, b"enabled", &root).is_ok());
        fs::write(&config, b"disabled").unwrap();
        assert!(recheck(&config, b"enabled", &root).is_err());
        fs::write(&config, b"enabled").unwrap();
        fs::rename(&path, parent.join("moved")).unwrap();
        mkdir(&path).unwrap();
        assert!(recheck(&config, b"enabled", &root).is_err());
    }
}
