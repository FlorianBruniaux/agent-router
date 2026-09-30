//! Per-attempt host files. Never import personal configuration or OAuth tokens.
use crate::{Result, RuntimeError};
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
}
impl HostState {
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
    pub(crate) fn cleanup(self) -> Result<()> {
        crate::stale::remove_stale(&self.path)
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
