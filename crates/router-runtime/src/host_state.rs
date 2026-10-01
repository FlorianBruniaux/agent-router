//! Per-attempt host files. Never import personal configuration or OAuth tokens.
use crate::{Result, RuntimeError, private_program::PrivateProgram};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Runtime-created state, held until the supervised attempt ends. No public
/// constructor: callers cannot turn an arbitrary path into a writable grant.
pub struct HostState {
    path: PathBuf,
    store: PathBuf,
    program: String,
    private_program: Option<PrivateProgram>,
}
impl HostState {
    pub(crate) fn create_with_program(
        store: &Path,
        program: &str,
        environment: &BTreeMap<String, String>,
        source: &Path,
        expected_hash: &str,
    ) -> Result<Option<Self>> {
        let Some(mut state) = Self::create(store, program, environment)? else {
            return Ok(None);
        };
        state.private_program = Some(PrivateProgram::copy(source, expected_hash, store)?);
        Ok(Some(state))
    }

    pub(crate) fn create(
        store: &Path,
        program: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<Option<Self>> {
        if !matches!(program, "codex" | "claude") {
            return Ok(None);
        }
        private_directory(store)?;
        // These built-in read roots are granted even without extra read_roots.
        let builtins: &[&str] = if cfg!(target_os = "macos") {
            &["/bin", "/sbin", "/usr", "/System", "/Library"]
        } else {
            &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/proc", "/dev"]
        };
        if builtins.iter().any(|root| store.starts_with(root)) {
            return Err(RuntimeError::Sandbox);
        }
        if environment.contains_key("CODEX_HOME") || environment.contains_key("CLAUDE_CONFIG_DIR") {
            return Err(RuntimeError::Sandbox);
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RuntimeError::Invalid("clock"))?
            .as_nanos();
        let path = store.join(format!(
            "host-state-{}-{}-{}",
            std::process::id(),
            stamp,
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        let state = Self {
            path,
            store: store.into(),
            program: program.into(),
            private_program: None,
        };
        if program == "codex" {
            if let Some(key) = environment.get("OPENAI_API_KEY") {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(nix::libc::O_NOFOLLOW)
                    .open(state.path.join("auth.json"))?;
                let bytes = serde_json::to_vec(&serde_json::json!({"OPENAI_API_KEY":key}))?;
                file.write_all(&bytes)?;
            }
        }
        Ok(Some(state))
    }
    pub(crate) fn private_program_path(&self, program: &str, expected_hash: &str) -> Result<&Path> {
        if program != self.program {
            return Err(RuntimeError::Sandbox);
        }
        self.private_program
            .as_ref()
            .ok_or(RuntimeError::Sandbox)?
            .verify(expected_hash)
    }
    pub(crate) fn extend(
        &self,
        program: &str,
        cwd: &Path,
        reads: &mut Vec<PathBuf>,
        writes: &mut Vec<PathBuf>,
        environment: &mut BTreeMap<String, String>,
    ) -> Result<()> {
        private_directory(&self.path)?;
        if program != self.program {
            return Err(RuntimeError::Sandbox);
        }
        for root in reads.iter().chain(writes.iter()) {
            // Runtime worktrees and version-probe cwd are intentional views.
            // No other store child, store root or ancestor may become readable.
            let runtime_cwd = cwd.starts_with(self.store.join("worktrees"))
                || (cwd.parent() == Some(self.store.as_path())
                    && cwd
                        .file_name()
                        .is_some_and(|n| n.as_encoded_bytes().starts_with(b"version-")));
            if self.store.starts_with(root)
                || (root.starts_with(&self.store) && !(runtime_cwd && root.starts_with(cwd)))
            {
                return Err(RuntimeError::Sandbox);
            }
        }
        if let Some(copy) = &self.private_program {
            if writes.iter().any(|root| {
                copy.directory().starts_with(root) || root.starts_with(copy.directory())
            }) {
                return Err(RuntimeError::Sandbox);
            }
            reads.push(copy.directory().into());
        }
        reads.push(self.path.clone());
        writes.push(self.path.clone());
        let variable = if program == "codex" {
            "CODEX_HOME"
        } else {
            "CLAUDE_CONFIG_DIR"
        };
        environment.insert(
            variable.into(),
            self.path
                .to_str()
                .ok_or(RuntimeError::Invalid("utf8 host state"))?
                .into(),
        );
        Ok(())
    }
    pub(crate) fn cleanup(mut self) -> Result<()> {
        let state_cleanup = crate::stale::remove_stale(&self.path);
        let program_cleanup = self
            .private_program
            .take()
            .map_or(Ok(()), PrivateProgram::cleanup);
        state_cleanup?;
        program_cleanup
    }
}
impl Drop for HostState {
    fn drop(&mut self) {
        let _ = crate::stale::remove_stale(&self.path);
    }
}
fn private_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir()
        || meta.permissions().mode() & 0o077 != 0
        || meta.uid() != rustix::process::geteuid().as_raw()
        || fs::canonicalize(path)? != path
    {
        return Err(RuntimeError::Sandbox);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SandboxPolicy, SandboxSupervisor};
    use router_types::*;

    fn native_run(cwd: &Path) -> PreparedRun {
        PreparedRun {
            schema_version: 1,
            receipt_id: "a".repeat(64),
            source_state: "b".repeat(40),
            scope: vec![],
            authorization_digest: "c".repeat(64),
            snapshot_hash: "d".repeat(64),
            host: HarnessId::parse("codex").unwrap(),
            requested_model: ModelId::parse("fixture-model").unwrap(),
            requested_effort: EffortLevel::Medium,
            program: "codex".into(),
            argv: vec![
                "-c".into(),
                r#"set -eu
if (printf denied > "$1") 2>/dev/null; then exit 61; fi
if (printf denied > "$2/denied") 2>/dev/null; then exit 62; fi
printf approved
"#
                .into(),
            ],
            cwd: cwd.to_str().unwrap().into(),
            stdin: PreparedInput::new(vec![]),
            mode: ExecutionMode::Execute,
            target: ExecutionTarget::ExternalWorker,
            sandbox: ExecutionSandbox::ReadOnly,
            timeout_ms: 3000,
            max_output_bytes: 4096,
            degradations: vec![],
            missing_runtime_evidence: vec![],
            parent_model_changed: false,
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn compile_shell_shim(path: &Path) {
        let source = path.with_extension("c");
        fs::write(
            &source,
            b"#include <unistd.h>\nint main(int argc, char **argv) { (void)argc; argv[0] = \"/bin/sh\"; execv(\"/bin/sh\", argv); return 127; }\n",
        )
        .unwrap();
        #[cfg(target_os = "macos")]
        let compiler = "/usr/bin/clang";
        #[cfg(target_os = "linux")]
        let compiler = "/usr/bin/cc";
        let output = std::process::Command::new(compiler)
            .arg(&source)
            .arg("-o")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "local fixture compile: {output:?}");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    #[ignore = "requires native OS sandbox; runs only copied local programs"]
    async fn approved_program_copy_survives_source_replacement_and_is_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let store = root.join("store");
        let cwd = root.join("repo");
        fs::DirBuilder::new().mode(0o700).create(&store).unwrap();
        fs::create_dir(&cwd).unwrap();
        let source = root.join("fixture-program");
        compile_shell_shim(&source);
        let programs = BTreeMap::from([("codex".into(), source.clone())]);
        #[cfg(target_os = "macos")]
        let sandbox = crate::SeatbeltSandbox::probe(&store, programs, vec![], BTreeMap::new())
            .await
            .unwrap();
        #[cfg(target_os = "linux")]
        let sandbox = crate::BubblewrapSandbox::probe(
            &store,
            &fs::canonicalize("/usr/bin/bwrap").unwrap(),
            programs,
            vec![],
            BTreeMap::new(),
        )
        .await
        .unwrap();
        let run = native_run(&cwd);
        let state = sandbox.create_host_state(&store, &run).unwrap().unwrap();
        let state_path = state.path.clone();
        let mut command = sandbox
            .command(
                &run,
                &SandboxPolicy {
                    network: false,
                    host_state: Some(&state),
                },
            )
            .unwrap();
        let args: Vec<_> = command.as_std().get_args().collect();
        #[cfg(target_os = "macos")]
        let executable = PathBuf::from(args[2]);
        #[cfg(target_os = "linux")]
        let executable = PathBuf::from(args[args.iter().position(|arg| *arg == "--").unwrap() + 1]);
        let private_dir = executable.parent().unwrap().to_path_buf();
        command.arg("--").arg(&executable).arg(&private_dir);

        // Only this temporary source changes; the system binaries stay intact.
        let replacement = root.join("replacement-program");
        fs::copy("/usr/bin/false", &replacement).unwrap();
        fs::rename(replacement, &source).unwrap();
        let output = command.output().await.unwrap();
        assert!(
            output.status.success(),
            "copied shell shim must run: {output:?}"
        );
        assert_eq!(output.stdout, b"approved");
        assert_ne!(executable, source);
        assert!(!private_dir.starts_with(&store));
        assert_eq!(
            fs::metadata(&private_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
            0o500
        );
        assert!(!private_dir.join("denied").exists());
        state.cleanup().unwrap();
        assert!(!state_path.exists());
        assert!(!private_dir.exists());

        // The changed source must be rejected before a second copy is created.
        assert!(matches!(
            sandbox.create_host_state(&store, &run),
            Err(RuntimeError::Integrity(_))
        ));
    }
    #[test]
    fn private_program_refuses_missing_copy_other_host_tampering_and_write_grants() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let store = root.join("store");
        fs::DirBuilder::new().mode(0o700).create(&store).unwrap();
        let source = root.join("fixture-program");
        fs::copy("/bin/sh", &source).unwrap();
        let hash = crate::sandbox::binary_hash(&source).unwrap();
        let empty = HostState::create(&store, "codex", &BTreeMap::new())
            .unwrap()
            .unwrap();
        assert!(matches!(
            empty.private_program_path("codex", &hash),
            Err(RuntimeError::Sandbox)
        ));
        drop(empty);

        let state =
            HostState::create_with_program(&store, "codex", &BTreeMap::new(), &source, &hash)
                .unwrap()
                .unwrap();
        let executable = state
            .private_program_path("codex", &hash)
            .unwrap()
            .to_path_buf();
        let directory = executable.parent().unwrap().to_path_buf();
        assert!(matches!(
            state.private_program_path("claude", &hash),
            Err(RuntimeError::Sandbox)
        ));
        assert!(matches!(
            state.extend(
                "codex",
                &root.join("repo"),
                &mut vec![],
                &mut vec![directory.clone()],
                &mut BTreeMap::new(),
            ),
            Err(RuntimeError::Sandbox)
        ));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(matches!(
            state.private_program_path("codex", &hash),
            Err(RuntimeError::Integrity(_))
        ));
        state.cleanup().unwrap();
        assert!(!directory.exists());
    }
    #[test]
    fn host_state_is_unique_private_and_api_key_only_then_removed() {
        let temp = tempfile::tempdir().unwrap();
        let store = fs::canonicalize(temp.path()).unwrap();
        fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
        let env = [("OPENAI_API_KEY".into(), "synthetic-key".into())].into();
        let state = HostState::create(&store, "codex", &env).unwrap().unwrap();
        let second = HostState::create(&store, "codex", &BTreeMap::new())
            .unwrap()
            .unwrap();
        assert_ne!(state.path, second.path);
        assert_eq!(
            fs::metadata(&state.path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let auth = state.path.join("auth.json");
        assert_eq!(
            fs::metadata(&auth).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let value: serde_json::Value = serde_json::from_slice(&fs::read(auth).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!({"OPENAI_API_KEY":"synthetic-key"}));
        assert_eq!(fs::read_dir(&second.path).unwrap().count(), 0);
        let path = state.path.clone();
        state.cleanup().unwrap();
        assert!(!path.exists());
        let path = second.path.clone();
        drop(second);
        assert!(!path.exists());
    }
    #[test]
    fn host_state_refuses_aliases_and_personal_directories() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let store = root.join("store");
        fs::DirBuilder::new().mode(0o700).create(&store).unwrap();
        std::os::unix::fs::symlink(&store, root.join("alias")).unwrap();
        assert!(HostState::create(&root.join("alias"), "codex", &BTreeMap::new()).is_err());
        for name in ["CODEX_HOME", "CLAUDE_CONFIG_DIR"] {
            let env = [(name.into(), "/personal".into())].into();
            assert!(HostState::create(&store, "codex", &env).is_err());
        }
        fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(HostState::create(&store, "claude", &BTreeMap::new()).is_err());
    }
    #[test]
    fn host_state_cannot_expose_store_and_adds_only_one_write_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        let store = root.join("store");
        fs::DirBuilder::new().mode(0o700).create(&store).unwrap();
        let state = HostState::create(&store, "claude", &BTreeMap::new())
            .unwrap()
            .unwrap();
        let cwd = root.join("repo");
        for forbidden in [
            &root,
            &store,
            &store.join("approval.key"),
            &store.join("runtime.sqlite3"),
        ] {
            assert!(
                state
                    .extend(
                        "claude",
                        &cwd,
                        &mut vec![forbidden.clone()],
                        &mut vec![],
                        &mut BTreeMap::new()
                    )
                    .is_err()
            );
            assert!(
                state
                    .extend(
                        "claude",
                        &cwd,
                        &mut vec![],
                        &mut vec![forbidden.clone()],
                        &mut BTreeMap::new()
                    )
                    .is_err()
            );
        }
        let mut reads = vec![cwd.clone()];
        let mut writes = vec![];
        let mut env = BTreeMap::new();
        state
            .extend("claude", &cwd, &mut reads, &mut writes, &mut env)
            .unwrap();
        assert_eq!(writes, [state.path.clone()]);
        assert_eq!(reads, [cwd, state.path.clone()]);
        assert_eq!(env["CLAUDE_CONFIG_DIR"], state.path.to_str().unwrap());
        assert!(!env.contains_key("CODEX_HOME"));
    }
}
