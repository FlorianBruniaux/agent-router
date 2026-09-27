//! Linux mount/PID/network isolation. Missing user namespaces or bubblewrap deny
//! execution; no profile-only result is treated as an enforcement proof.
use crate::{
    Cancellation, Result, RuntimeError, SandboxPolicy, SandboxSupervisor,
    process::capture,
    sandbox::{binary_hash, validate_configuration},
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
        fs::create_dir(&probe)?;
        let allowed = probe.join("allowed");
        fs::create_dir(&allowed)?;
        fs::write(allowed.join("read"), b"read-control")?;
        let secret = private_dir.join("bwrap-read-denied");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&secret)?;
        std::io::Write::write_all(&mut file, b"synthetic-read-canary")?;
        drop(file);
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
        let cleanup = fs::remove_dir_all(&probe);
        let cleanup_file = fs::remove_file(&secret);
        check?;
        cleanup?;
        cleanup_file?;
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
        let writes = if run.sandbox == ExecutionSandbox::WorkspaceWrite {
            run.scope.iter().map(|path| cwd.join(path)).collect()
        } else {
            vec![]
        };
        wrap(
            &self.wrapper,
            program.to_str().ok_or(RuntimeError::Sandbox)?,
            &run.argv,
            cwd,
            &reads,
            &writes,
            policy.network,
            &self.environment,
        )
    }
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
    for root in reads {
        if fs::canonicalize(root)? != *root || root == Path::new("/") {
            return Err(RuntimeError::Sandbox);
        }
        command.arg("--ro-bind").arg(root).arg(root);
    }
    for root in writes {
        if fs::canonicalize(root)? != *root {
            return Err(RuntimeError::Sandbox);
        }
        command.arg("--bind").arg(root).arg(root);
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
