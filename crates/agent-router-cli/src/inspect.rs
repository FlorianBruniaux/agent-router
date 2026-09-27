use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "codex")]
use adapter_codex::{CodexProbe, CodexProbeConfig};
#[cfg(any(feature = "codex", feature = "claude"))]
use router_adapter_api::HostProbe;
use router_adapter_api::{MAX_SNAPSHOT_BYTES, SnapshotValidation, ValidatedHostObservation};

use crate::{CliError, Outcome, command::HostChoice, read_bounded};

pub(crate) fn current_time(override_time: Option<u64>) -> Result<u64, CliError> {
    override_time.map_or_else(
        || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|time| time.as_secs())
                .map_err(|_| CliError::Internal)
        },
        Ok,
    )
}

pub(crate) struct InspectArgs {
    pub host: HostChoice,
    pub model_cache: Option<PathBuf>,
    pub native_stream: Option<PathBuf>,
    pub observed_at: Option<u64>,
    pub observed_version: Option<String>,
    pub now: Option<u64>,
}
pub(crate) fn inspect(args: InspectArgs, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    // An installed executable or a cache alone is not evidence of an active host.
    if matches!(args.host, HostChoice::Auto) && std::env::var_os("CODEX_THREAD_ID").is_none() {
        return Err(CliError::HostUnknown);
    }
    let now = current_time(args.now)?;
    let observation = if matches!(args.host, HostChoice::Claude) {
        if args.model_cache.is_some() || args.observed_version.is_some() {
            return Err(CliError::InvalidCommand);
        }
        let stream = args
            .native_stream
            .ok_or(CliError::Product("probe.native_stream.required"))?;
        let observed_at = args
            .observed_at
            .ok_or(CliError::Product("probe.observed_at.required"))?;
        probe_claude(&stream, observed_at)?
    } else {
        if args.native_stream.is_some() || args.observed_at.is_some() {
            return Err(CliError::InvalidCommand);
        }
        let model_cache_path = args
            .model_cache
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".codex/models_cache.json"))
            })
            .ok_or(CliError::HostUnknown)?;
        probe(model_cache_path, args.observed_version, now)?
    };
    let validated = ValidatedHostObservation::from_observation(
        observation,
        SnapshotValidation {
            now_unix_seconds: now,
            max_age_seconds: router_adapter_api::DEFAULT_MAX_AGE_SECONDS,
        },
    )
    .map_err(|error| CliError::Snapshot(error.code()))?;
    let output =
        serde_json::to_string_pretty(validated.observation()).map_err(|_| CliError::Internal)?;
    Ok(Outcome {
        output: format!("{output}\n"),
        exit_code: ExitCode::SUCCESS,
    })
}

#[cfg(feature = "claude")]
fn probe_claude(
    path: &Path,
    observed_at: u64,
) -> Result<router_adapter_api::HostObservation, CliError> {
    let bytes = read_bounded(
        path,
        router_adapter_api::MAX_TRANSPORT_BYTES,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )?;
    adapter_claude::ClaudeStreamProbe::from_native_stream(&bytes)
        .probe(observed_at)
        .map_err(|error| CliError::Probe(error.code()))
}
#[cfg(not(feature = "claude"))]
fn probe_claude(_: &Path, _: u64) -> Result<router_adapter_api::HostObservation, CliError> {
    Err(CliError::Probe("probe.feature.disabled"))
}

pub(crate) fn read_snapshot(
    path: &Path,
    now: Option<u64>,
    max_age_seconds: u64,
) -> Result<ValidatedHostObservation, CliError> {
    let bytes = read_bounded(
        path,
        MAX_SNAPSHOT_BYTES,
        CliError::SnapshotRead,
        CliError::SnapshotTooLarge,
    )?;
    ValidatedHostObservation::from_json(
        &bytes,
        SnapshotValidation {
            now_unix_seconds: current_time(now)?,
            max_age_seconds,
        },
    )
    .map_err(|error| CliError::Snapshot(error.code()))
}

#[cfg(feature = "codex")]
fn probe(
    model_cache_path: PathBuf,
    observed_version: Option<String>,
    now: u64,
) -> Result<router_adapter_api::HostObservation, CliError> {
    CodexProbe::new(CodexProbeConfig {
        model_cache_path,
        observed_version,
    })
    .probe(now)
    .map_err(|error| CliError::Probe(error.code()))
}

#[cfg(not(feature = "codex"))]
fn probe(
    _: PathBuf,
    _: Option<String>,
    _: u64,
) -> Result<router_adapter_api::HostObservation, CliError> {
    Err(CliError::Probe("probe.feature.disabled"))
}
