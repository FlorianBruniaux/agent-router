//! The supervisor is trusted code, never an authority deserialized from a file.
use crate::{Cancellation, Result, RuntimeError, process::capture, workspace::safe_scope};
use router_types::{ExecutionSandbox, PreparedRun};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use tokio::process::Command;

pub struct SandboxPolicy {
    pub network: bool,
}
/// Implementors must enforce read/write/network restrictions in the child and
/// every descendant. A declared host capability or receipt is not sufficient.
pub trait SandboxSupervisor: Send + Sync {
    fn command(&self, run: &PreparedRun, policy: &SandboxPolicy) -> Result<Command>;
    fn resource_limits_enforced(&self) -> bool {
        false
    }
    fn memory_enforcement(&self) -> router_types::MemoryEnforcement {
        router_types::MemoryEnforcement::Unsupported
    }
}
pub struct DenySandbox;
impl SandboxSupervisor for DenySandbox {
    fn command(&self, _: &PreparedRun, _: &SandboxPolicy) -> Result<Command> {
        Err(RuntimeError::Sandbox)
    }
}

/// Native macOS Seatbelt, available only after live deny/allow/network probes.
/// Environment and extra readable roots are explicit trusted integration input.
pub struct SeatbeltSandbox {
    wrapper_hash: String,
    programs: BTreeMap<String, PathBuf>,
    program_hashes: BTreeMap<String, String>,
    read_roots: Vec<PathBuf>,
    environment: BTreeMap<String, String>,
}
impl SeatbeltSandbox {
    pub async fn probe(
        private_dir: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
    ) -> Result<Self> {
        Self::probe_inner(private_dir, programs, read_roots, environment, None).await
    }
    pub async fn probe_expected(
        private_dir: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
        expected: &BTreeMap<PathBuf, String>,
    ) -> Result<Self> {
        Self::probe_inner(
            private_dir,
            programs,
            read_roots,
            environment,
            Some(expected),
        )
        .await
    }
    async fn probe_inner(
        private_dir: &Path,
        programs: BTreeMap<String, PathBuf>,
        read_roots: Vec<PathBuf>,
        environment: BTreeMap<String, String>,
        expected: Option<&BTreeMap<PathBuf, String>>,
    ) -> Result<Self> {
        if !cfg!(target_os = "macos") || !Path::new("/usr/bin/sandbox-exec").is_file() {
            return Err(RuntimeError::Sandbox);
        }
        if fs::canonicalize(private_dir)? != private_dir {
            return Err(RuntimeError::Invalid("canonical probe directory"));
        }
        validate_configuration(&programs, &read_roots, &environment)?;
        let wrapper_hash = binary_hash(Path::new("/usr/bin/sandbox-exec"))?;
        check_expected(Path::new("/usr/bin/sandbox-exec"), &wrapper_hash, expected)?;
        let program_hashes = programs
            .iter()
            .map(|(name, path)| {
                let hash = binary_hash(path)?;
                check_expected(path, &hash, expected)?;
                Ok((name.clone(), hash))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let probe = private_dir.join("sandbox-probe");
        fs::create_dir(&probe)?;
        let allowed = probe.join("allowed");
        fs::create_dir(&allowed)?;
        let denied = probe.join("denied");
        let denied_read = private_dir.join("sandbox-canary-read");
        let mut denied_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&denied_read)?;
        denied_file.write_all(b"synthetic-read-canary")?;
        drop(denied_file);
        fs::write(allowed.join("read"), b"read-control")?;
        let profile = seatbelt_profile(&[probe.clone()], &[allowed.clone()], false)?;
        let check = async {
            let mut command = checked_seatbelt_command(
                &wrapper_hash,
                &profile,
                "/usr/bin/touch",
                &[denied.to_string_lossy().into_owned()],
            )?;
            command.env_clear();
            let output =
                capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default()).await?;
            if output.outcome.exit_code == Some(0) || denied.exists() {
                return Err(RuntimeError::Invalid("sandbox canary denied write escaped"));
            }
            let mut command = checked_seatbelt_command(
                &wrapper_hash,
                &profile,
                "/usr/bin/touch",
                &[allowed.join("yes").to_string_lossy().into_owned()],
            )?;
            command.env_clear();
            let output =
                capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default()).await?;
            if output.outcome.exit_code != Some(0) || !allowed.join("yes").is_file() {
                return Err(RuntimeError::Invalid("sandbox canary allowed write failed"));
            }
            let mut command = checked_seatbelt_command(
                &wrapper_hash,
                &profile,
                "/bin/cat",
                &[allowed.join("read").to_string_lossy().into_owned()],
            )?;
            command.env_clear();
            let control =
                capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default()).await?;
            if control.outcome.exit_code != Some(0) || control.stdout != b"read-control" {
                return Err(RuntimeError::Invalid("sandbox canary allowed read failed"));
            }
            let mut command = checked_seatbelt_command(
                &wrapper_hash,
                &profile,
                "/bin/cat",
                &[denied_read.to_string_lossy().into_owned()],
            )?;
            command.env_clear();
            let denied_output =
                capture(command, vec![], 2_000, 16_384, 0, &Cancellation::default()).await?;
            if denied_output.outcome.exit_code == Some(0) || !denied_output.stdout.is_empty() {
                return Err(RuntimeError::Invalid("sandbox canary denied read escaped"));
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
            let mut command =
                checked_seatbelt_command(&wrapper_hash, &profile, "/usr/bin/nc", &args)?;
            command.env_clear();
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
        // Only the probe directory created by this call is removed.
        let cleanup = fs::remove_dir_all(&probe);
        let cleanup_file = fs::remove_file(&denied_read);
        check?;
        cleanup?;
        cleanup_file?;
        Ok(Self {
            wrapper_hash,
            programs,
            program_hashes,
            read_roots,
            environment,
        })
    }
}
impl SandboxSupervisor for SeatbeltSandbox {
    fn command(&self, run: &PreparedRun, policy: &SandboxPolicy) -> Result<Command> {
        let program = self
            .programs
            .get(&run.program)
            .ok_or(RuntimeError::Sandbox)?;
        if fs::canonicalize(program)? != *program || !program.is_file() {
            return Err(RuntimeError::Sandbox);
        }
        if self.program_hashes.get(&run.program) != Some(&binary_hash(program)?) {
            return Err(RuntimeError::Integrity("native binary changed"));
        }
        let cwd = Path::new(&run.cwd);
        safe_scope(cwd, &run.scope)?;
        let mut reads = self.read_roots.clone();
        reads.push(cwd.into());
        reads.push(program.clone());
        let writes = if run.sandbox == ExecutionSandbox::WorkspaceWrite {
            run.scope.iter().map(|p| cwd.join(p)).collect()
        } else {
            vec![]
        };
        let profile = seatbelt_profile(&reads, &writes, policy.network)?;
        let mut command = checked_seatbelt_command(
            &self.wrapper_hash,
            &profile,
            program.to_str().ok_or(RuntimeError::Sandbox)?,
            &run.argv,
        )?;
        command
            .current_dir(cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        for (key, value) in &self.environment {
            command.env(key, value);
        }
        Ok(command)
    }
}
fn checked_seatbelt_command(
    wrapper_hash: &str,
    profile: &str,
    program: &str,
    args: &[String],
) -> Result<Command> {
    if binary_hash(Path::new("/usr/bin/sandbox-exec"))? != wrapper_hash {
        return Err(RuntimeError::Integrity("sandbox-exec changed"));
    }
    let mut command = Command::new("/usr/bin/sandbox-exec");
    command.arg("-p").arg(profile).arg(program).args(args);
    Ok(command)
}
fn quote(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or(RuntimeError::Sandbox)?;
    if !path.starts_with('/') || path.contains(['\0', '\n', '\r']) {
        return Err(RuntimeError::Sandbox);
    }
    Ok(format!(
        "\"{}\"",
        path.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
pub(crate) fn binary_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 256 * 1024 * 1024 {
        return Err(RuntimeError::Limit);
    }
    let mut count = 0;
    let mut hash = Sha256::new();
    let mut buffer = [0; 16_384];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n;
        if count > 256 * 1024 * 1024 {
            return Err(RuntimeError::Limit);
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub(crate) fn check_expected(
    path: &Path,
    actual: &str,
    expected: Option<&BTreeMap<PathBuf, String>>,
) -> Result<()> {
    if expected.is_some_and(|values| values.get(path).map(String::as_str) != Some(actual)) {
        return Err(RuntimeError::Integrity(
            "approved binary changed or missing",
        ));
    }
    Ok(())
}
fn seatbelt_profile(reads: &[PathBuf], writes: &[PathBuf], network: bool) -> Result<String> {
    let mut profile = String::from(
        "(version 1)\n(deny default)\n(allow process-exec process-fork sysctl-read mach-lookup)\n(allow signal (target self))\n(allow file-read-metadata)\n(allow file-read* (literal \"/\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/usr\") (subpath \"/System\") (subpath \"/Library\") (literal \"/dev/null\") (literal \"/dev/urandom\") (literal \"/private/etc/resolv.conf\") (literal \"/private/etc/hosts\"))\n(allow file-write* (literal \"/dev/null\"))\n",
    );
    for path in reads {
        profile.push_str(&format!(
            "(allow file-read* (literal {0}) (subpath {0}))\n",
            quote(path)?
        ));
    }
    for path in writes {
        profile.push_str(&format!(
            "(allow file-write* (literal {0}) (subpath {0}))\n",
            quote(path)?
        ));
    }
    // A child may read Git metadata but may never redirect its private .git file.
    profile.push_str("(deny file-write* (regex #\"(^|/)\\.git(/|$)\"))\n");
    if network {
        profile.push_str("(allow network*)\n");
    }
    Ok(profile)
}
pub(crate) fn validate_configuration(
    programs: &BTreeMap<String, PathBuf>,
    roots: &[PathBuf],
    environment: &BTreeMap<String, String>,
) -> Result<()> {
    if programs.is_empty() || programs.len() > 16 || roots.len() > 32 || environment.len() > 16 {
        return Err(RuntimeError::Limit);
    }
    for path in programs.values().chain(roots.iter()) {
        if fs::canonicalize(path)? != *path || path == Path::new("/") {
            return Err(RuntimeError::Sandbox);
        }
    }
    for (key, value) in environment {
        if !matches!(
            key.as_str(),
            "OPENAI_API_KEY"
                | "ANTHROPIC_API_KEY"
                | "CODEX_HOME"
                | "CLAUDE_CONFIG_DIR"
                | "HOME"
                | "SSL_CERT_FILE"
        ) || value.contains('\0')
            || value.len() > 16_384
        {
            return Err(RuntimeError::Sandbox);
        }
    }
    Ok(())
}
