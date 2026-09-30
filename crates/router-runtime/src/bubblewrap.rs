//! Linux mount/PID/network isolation. Missing user namespaces or bubblewrap deny
//! execution; no profile-only result is treated as an enforcement proof.
use crate::{
    Cancellation, Result, RuntimeError, SandboxPolicy, SandboxSupervisor,
    process::capture,
    sandbox::{binary_hash, require_canonical_paths, validate_configuration},
    workspace::safe_scope,
};
use router_types::{ExecutionSandbox, PreparedRun};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub struct BubblewrapSandbox {
    wrapper: PathBuf,
    wrapper_hash: String,
    programs: BTreeMap<String, PathBuf>,
    program_hashes: BTreeMap<String, String>,
    read_roots: Vec<PathBuf>,
    environment: BTreeMap<String, String>,
}
impl BubblewrapSandbox {
    pub async fn probe(
        private_dir: &Path,
        wrapper: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
    ) -> Result<Self> {
        Self::probe_inner(
            private_dir,
            wrapper,
            programs,
            read_roots,
            environment,
            None,
        )
        .await
    }
    pub async fn probe_expected(
        private_dir: &Path,
        wrapper: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
        expected: &BTreeMap<PathBuf, String>,
    ) -> Result<Self> {
        Self::probe_inner(
            private_dir,
            wrapper,
            programs,
            read_roots,
            environment,
            Some(expected),
        )
        .await
    }
    async fn probe_inner(
        private_dir: &Path,
        wrapper: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
        expected: Option<&BTreeMap<PathBuf, String>>,
    ) -> Result<Self> {
        if !cfg!(target_os = "linux")
            || fs::canonicalize(private_dir)? != private_dir
            || fs::canonicalize(wrapper)? != wrapper
        {
            return Err(RuntimeError::Sandbox);
        }
        validate_configuration(&programs, &read_roots, &environment)?;
        let wrapper_hash = binary_hash(wrapper)?;
        crate::sandbox::check_expected(wrapper, &wrapper_hash, expected)?;
        let program_hashes = programs
            .iter()
            .map(|(name, path)| {
                let hash = binary_hash(path)?;
                crate::sandbox::check_expected(path, &hash, expected)?;
                Ok((name.clone(), hash))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let probe = private_dir.join("bwrap-probe");
        let secret = private_dir.join("bwrap-read-denied");
        // Tracked before creation: every early return below still cleans up.
        let mut scratch = crate::stale::Scratch::default();
        scratch.claim(&probe)?;
        scratch.claim(&secret)?;
        fs::create_dir(&probe)?;
        let allowed = probe.join("allowed");
        fs::create_dir(&allowed)?;
        fs::write(allowed.join("read"), b"read-control")?;
        crate::stale::create_canary(&secret, b"synthetic-read-canary")?;
        let check = async {
            for (program, arg, wanted) in [
                ("/usr/bin/touch", allowed.join("yes"), true),
                ("/usr/bin/touch", probe.join("denied"), false),
                ("/bin/cat", allowed.join("read"), true),
                ("/bin/cat", secret.clone(), false),
            ] {
                if binary_hash(wrapper)? != wrapper_hash {
                    return Err(RuntimeError::Integrity("bubblewrap changed"));
                }
                let command = wrap(
                    wrapper,
                    program,
                    &[arg.to_string_lossy().into_owned()],
                    &probe,
                    std::slice::from_ref(&probe),
                    std::slice::from_ref(&allowed),
                    false,
                    &BTreeMap::new(),
                )?;
                let output =
                    capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default()).await?;
                if (output.outcome.exit_code == Some(0)) != wanted {
                    return Err(RuntimeError::Sandbox);
                }
            }
            if !allowed.join("yes").exists() || probe.join("denied").exists() {
                return Err(RuntimeError::Sandbox);
            }
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port().to_string();
            let args = vec![
                "-z".into(),
                "-w".into(),
                "1".into(),
                "127.0.0.1".into(),
                port,
            ];
            let mut control = Command::new("/usr/bin/nc");
            control.args(&args).env_clear();
            if capture(control, vec![], 2_000, 16_384, 0, &Cancellation::default())
                .await?
                .outcome
                .exit_code
                != Some(0)
            {
                return Err(RuntimeError::Sandbox);
            }
            if binary_hash(wrapper)? != wrapper_hash {
                return Err(RuntimeError::Integrity("bubblewrap changed"));
            }
            let command = wrap(
                wrapper,
                "/usr/bin/nc",
                &args,
                &probe,
                std::slice::from_ref(&probe),
                &[],
                false,
                &BTreeMap::new(),
            )?;
            if capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default())
                .await?
                .outcome
                .exit_code
                == Some(0)
            {
                return Err(RuntimeError::Sandbox);
            }
            Ok(())
        }
        .await;
        let cleanup = scratch.cleanup();
        check?;
        cleanup?;
        Ok(Self {
            wrapper: wrapper.into(),
            wrapper_hash,
            programs,
            program_hashes,
            read_roots,
            environment,
        })
    }
}
impl SandboxSupervisor for BubblewrapSandbox {
    fn create_host_state(
        &self,
        store: &Path,
        run: &PreparedRun,
    ) -> Result<Option<crate::HostState>> {
        crate::HostState::create(store, &run.program, &self.environment)
    }
    fn pid_namespace_isolated(&self) -> bool {
        cfg!(target_os = "linux")
    }
    fn command(&self, run: &PreparedRun, policy: &SandboxPolicy) -> Result<Command> {
        if binary_hash(&self.wrapper)? != self.wrapper_hash {
            return Err(RuntimeError::Integrity("bubblewrap changed"));
        }
        let program = self
            .programs
            .get(&run.program)
            .ok_or(RuntimeError::Sandbox)?;
        if self.program_hashes.get(&run.program) != Some(&binary_hash(program)?) {
            return Err(RuntimeError::Integrity("native binary changed"));
        }
        let cwd = Path::new(&run.cwd);
        safe_scope(cwd, &run.scope)?;
        let mut reads = self.read_roots.clone();
        reads.push(cwd.into());
        reads.push(program.clone());
        let mut writes = if run.sandbox == ExecutionSandbox::WorkspaceWrite {
            run.scope.iter().map(|path| cwd.join(path)).collect()
        } else {
            vec![]
        };
        let mut environment = self.environment.clone();
        if let Some(state) = policy.host_state {
            state.extend(&run.program, cwd, &mut reads, &mut writes, &mut environment)?;
        } else if matches!(run.program.as_str(), "codex" | "claude") {
            return Err(RuntimeError::Sandbox);
        }
        wrap(
            &self.wrapper,
            program.to_str().ok_or(RuntimeError::Sandbox)?,
            &run.argv,
            cwd,
            &reads,
            &writes,
            policy.network,
            &environment,
        )
    }
}
/// `.git` entries directly inside a writable scope root, compared ASCII
/// case-insensitively, without following symlinks. Only the scope root is
/// inspected: a recursive walk fails (or costs seconds) on scopes containing
/// `node_modules` or `target`. A `.git` symlink is refused. Nested `.git`
/// entries, and any created during the run, are not remounted; the host refuses
/// any nested `.git` in the worktree before its first git command
/// (`workspace::refuse_nested_git`).
fn git_entries(root: &Path) -> Result<Vec<PathBuf>> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Ok(vec![]);
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .eq_ignore_ascii_case(b".git")
        {
            if entry.file_type()?.is_symlink() {
                return Err(RuntimeError::Sandbox);
            }
            found.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}
#[allow(clippy::too_many_arguments)]
fn wrap(
    wrapper: &Path,
    program: &str,
    args: &[String],
    cwd: &Path,
    reads: &[PathBuf],
    writes: &[PathBuf],
    network: bool,
    environment: &BTreeMap<String, String>,
) -> Result<Command> {
    let mut command = Command::new(wrapper);
    command.args([
        "--die-with-parent",
        "--new-session",
        "--unshare-all",
        "--cap-drop",
        "ALL",
    ]);
    if network {
        command.arg("--share-net");
    }
    for root in [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc/ld.so.cache",
        "/etc/resolv.conf",
        "/etc/hosts",
    ] {
        if Path::new(root).exists() {
            command.arg("--ro-bind").arg(root).arg(root);
        }
    }
    command.args(["--proc", "/proc", "--dev", "/dev"]);
    require_canonical_paths(reads, writes, true)?;
    for root in reads {
        command.arg("--ro-bind").arg(root).arg(root);
    }
    for root in writes {
        command.arg("--bind").arg(root).arg(root);
        // Seatbelt denies writes to every `.git` path. Bubblewrap cannot deny
        // by name, so remount a `.git` at the scope root read-only over the bind.
        for git in git_entries(root)? {
            command.arg("--ro-bind").arg(&git).arg(&git);
        }
    }
    command
        .arg("--chdir")
        .arg(cwd)
        .arg("--")
        .arg(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    for (key, value) in environment {
        command.env(key, value);
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(command: &Command) -> Vec<String> {
        command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temp.path()).unwrap();
        fs::create_dir_all(root.join("src/nested/.GIT")).unwrap();
        fs::create_dir_all(root.join("src/.git")).unwrap();
        fs::create_dir_all(root.join("src2/.GIT")).unwrap();
        fs::create_dir_all(root.join("src/plain")).unwrap();
        fs::create_dir_all(root.join("src/pkg")).unwrap();
        fs::write(root.join("src/pkg/.Git"), b"gitdir: elsewhere").unwrap();
        (temp, root)
    }
    #[test]
    fn writable_binds_remount_a_scope_root_git_read_only_after_the_bind() {
        let (_temp, root) = tree();
        let (src, src2) = (root.join("src"), root.join("src2"));
        let command = wrap(
            Path::new("/usr/bin/bwrap"),
            "/bin/true",
            &[],
            &root,
            std::slice::from_ref(&root),
            &[src.clone(), src2.clone()],
            false,
            &BTreeMap::new(),
        )
        .unwrap();
        let args = argv(&command);
        let position = |flag: &str, path: &Path| {
            let path = path.to_str().unwrap();
            args.windows(3)
                .position(|w| w[0] == flag && w[1] == path && w[2] == path)
        };
        for (write, git) in [(&src, src.join(".git")), (&src2, src2.join(".GIT"))] {
            let bind = position("--bind", write).expect("writable bind");
            let ro = position("--ro-bind", &git)
                .unwrap_or_else(|| panic!("missing read-only remount of {}", git.display()));
            assert!(ro > bind, "remount must follow the writable bind");
        }
        // Documented limit: nested entries are not walked or remounted.
        assert!(position("--ro-bind", &src.join("nested/.GIT")).is_none());
        assert!(position("--ro-bind", &src.join("pkg/.Git")).is_none());
    }
    #[test]
    fn git_symlink_at_a_scope_root_is_refused_but_nested_ones_are_not_walked() {
        let (_temp, root) = tree();
        let write = root.join("src/plain");
        std::os::unix::fs::symlink(root.join("src"), write.join(".git")).unwrap();
        assert!(matches!(git_entries(&write), Err(RuntimeError::Sandbox)));
        fs::create_dir(root.join("src/deep")).unwrap();
        std::os::unix::fs::symlink(root.join("src"), root.join("src/deep/.git")).unwrap();
        assert_eq!(
            git_entries(&root.join("src")).unwrap(),
            [root.join("src/.git")]
        );
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn live_bubblewrap_denies_writes_below_git() {
        let bwrap = Path::new("/usr/bin/bwrap");
        if !bwrap.is_file() {
            return;
        }
        let (_temp, root) = tree();
        let write = root.join("src");
        let run = |script: &str| {
            wrap(
                bwrap,
                "/bin/sh",
                &["-c".into(), script.into()],
                &root,
                std::slice::from_ref(&root),
                std::slice::from_ref(&write),
                false,
                &BTreeMap::new(),
            )
        };
        let control = crate::process::capture(
            run("printf x > src/plain/ok").unwrap(),
            vec![],
            5_000,
            4096,
            0,
            &Cancellation::default(),
        )
        .await
        .unwrap();
        if control.outcome.exit_code != Some(0) {
            return; // user namespaces unavailable here; the argv test still holds
        }
        let denied = crate::process::capture(
            run("printf x > src/.git/probe").unwrap(),
            vec![],
            5_000,
            4096,
            0,
            &Cancellation::default(),
        )
        .await
        .unwrap();
        assert_ne!(denied.outcome.exit_code, Some(0));
        assert!(!write.join(".git/probe").exists());
    }
}
