// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use crate::{CliError, Outcome, execution, human, product::json_output};
use router_runtime::RunObservation;
use serde::Serialize;
use std::{path::Path, process::ExitCode};

const HUMAN_EVENT_LIMIT: usize = 20;

pub(crate) fn run(
    store: &Path,
    id: &str,
    schema_version: u32,
    json: bool,
) -> Result<Outcome, CliError> {
    if !(1..=5).contains(&schema_version) {
        return Err(CliError::InvalidCommand);
    }
    if schema_version == 5 {
        let report = router_runtime::observe_run_v5(store, id).map_err(execution::error)?;
        if json {
            return json_output(&report, ExitCode::SUCCESS);
        }
        let mut output = render_v4(&report.observation, "5")?;
        output.push_str(&render_findings(&report.findings)?);
        return Ok(Outcome {
            output,
            exit_code: ExitCode::SUCCESS,
        });
    }
    if schema_version == 4 {
        let report = router_runtime::observe_run_v4(store, id).map_err(execution::error)?;
        if json {
            return json_output(&report, ExitCode::SUCCESS);
        }
        return Ok(Outcome {
            output: render_v4(&report, "4")?,
            exit_code: ExitCode::SUCCESS,
        });
    }
    if schema_version == 3 {
        let report = router_runtime::observe_run_v3(store, id).map_err(execution::error)?;
        if json {
            return json_output(&report, ExitCode::SUCCESS);
        }
        let mut output = render_v2(&report.observation)?;
        for timing in &report.timings {
            let value = match timing.measurement {
                router_runtime::CaptureTiming::Known { duration_ms } => format!("{duration_ms} ms"),
                router_runtime::CaptureTiming::Unknown { reason } => {
                    format!("unknown ({})", label(&reason)?)
                }
            };
            output.push_str(&format!("Attempt {} local supervisor capture: {} (monotonic; includes dispatch checks, reap and both output drains)\n", timing.ordinal, value));
        }
        return Ok(Outcome {
            output,
            exit_code: ExitCode::SUCCESS,
        });
    }
    if schema_version == 2 {
        let report = router_runtime::observe_run_v2(store, id).map_err(execution::error)?;
        if json {
            return json_output(&report, ExitCode::SUCCESS);
        }
        return Ok(Outcome {
            output: render_v2(&report)?,
            exit_code: ExitCode::SUCCESS,
        });
    }
    let report = router_runtime::observe_run(store, id).map_err(execution::error)?;
    if json {
        return json_output(&report, ExitCode::SUCCESS);
    }
    Ok(Outcome {
        output: render(&report)?,
        exit_code: ExitCode::SUCCESS,
    })
}

fn label(value: &impl Serialize) -> Result<String, CliError> {
    let value = serde_json::to_value(value).map_err(|_| CliError::Internal)?;
    Ok(match value {
        serde_json::Value::String(text) => human::escaped(&text),
        other => other.to_string(),
    })
}

fn render(report: &RunObservation) -> Result<String, CliError> {
    let mut output = format!(
        "Run observation\nSnapshot: {}\nRun key: {}\nState: {} | Attempts: {}\nJournal: {} through global sequence {} | Details: {}\nExternal observations: {}\nModel requested/observed: {}/{} | Effort requested/observed: {}/{}\nTransport: {} | Validation: {} | Objective: {}\nDuration: {}\nProcess liveness: {} | Journal authenticity: {}\nRecorded cleanup: {}\n",
        report.snapshot_sha256,
        report.identity.run_key_sha256,
        label(&report.known.state)?,
        report.known.attempts,
        label(&report.completeness.journal)?,
        report.horizon.sequence,
        label(&report.completeness.detail)?,
        label(&report.completeness.external_observations)?,
        label(&report.unknown.requested_model)?,
        label(&report.unknown.observed_model)?,
        label(&report.unknown.requested_effort)?,
        label(&report.unknown.observed_effort)?,
        label(&report.unknown.transport)?,
        label(&report.unknown.validation)?,
        label(&report.unknown.objective)?,
        label(&report.unknown.duration)?,
        label(&report.unknown.process_liveness)?,
        label(&report.unknown.journal_authenticity)?,
        label(&report.known.cleanup)?,
    );
    if let Some(failure) = &report.known.failure {
        output.push_str(&format!("Recorded failure: {}\n", label(failure)?));
    }
    let omitted = report.known.events.len().saturating_sub(HUMAN_EVENT_LIMIT);
    output.push_str(&format!(
        "Run events: showing {} of {} (sequence, recorded state)\n",
        report.known.events.len() - omitted,
        report.known.events.len(),
    ));
    for event in &report.known.events[omitted..] {
        output.push_str(&format!("  {}: {}\n", event.sequence, label(&event.state)?));
    }
    if omitted > 0 {
        output.push_str(&format!(
            "{} earlier event references omitted from this view; use --json for the full bounded snapshot.\n",
            omitted,
        ));
    }
    Ok(output)
}

fn render_v2(report: &router_runtime::RunObservationV2) -> Result<String, CliError> {
    let mut output = render(&report.summary)?;
    output.push_str("Aggregate fields remain unknown; per-attempt durable evidence follows.\n");
    for attempt in &report.attempts {
        let (model, effort) = match &attempt.requested {
            Some(requested) => (label(&requested.model)?, label(&requested.effort)?),
            None => ("unknown".into(), "unknown".into()),
        };
        let (termination, exit_code) = match &attempt.transport {
            Some(transport) => (
                label(&transport.termination)?,
                transport
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".into()),
            ),
            None => ("unknown".into(), "unknown".into()),
        };
        output.push_str(&format!("Attempt {}\nRequested model: {} | Requested effort: {}\nSupervisor termination: {} | Exit code: {}\nEvidence: {} | Capture: {} | Protocol: {}\n", attempt.ordinal, model, effort, termination, exit_code, label(&attempt.evidence)?, label(&attempt.capture)?, label(&attempt.protocol)?));
    }
    Ok(output)
}

fn sequence(value: Option<u64>) -> String {
    value.map_or_else(|| "none".into(), |value| value.to_string())
}

fn microusd(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".into(), |value| format!("{value} microusd"))
}

fn render_v4(report: &router_runtime::RunObservationV4, schema: &str) -> Result<String, CliError> {
    let v3 = &report.observation;
    let summary = &v3.observation.summary;
    let t = &report.timeline;
    let mut output = format!(
        "Run timeline (schema {})\nSnapshot: {}\nRun key: {}\nState: {} | Attempts: {}\n",
        schema,
        summary.snapshot_sha256,
        summary.identity.run_key_sha256,
        label(&summary.known.state)?,
        summary.known.attempts,
    );
    if let Some(failure) = &summary.known.failure {
        output.push_str(&format!("Recorded failure: {}\n", label(failure)?));
    }
    output.push_str(&format!(
        "Journal: {} through global sequence {} | Later events from other runs: {} | Details: {}\nOrdering: {} (no timestamps)\nShowing {} of {} events (limit {})\n",
        label(&summary.completeness.journal)?,
        summary.horizon.sequence,
        t.later_global_events,
        label(&summary.completeness.detail)?,
        label(&t.ordering)?,
        t.shown,
        t.run_events,
        t.entry_limit,
    ));
    if t.omitted_earlier > 0 {
        output.push_str(&format!(
            "{} earlier events omitted from the list below; the phase summary still counts them. Use --json for references.\n",
            t.omitted_earlier,
        ));
    }
    output.push_str("Events (sequence: phase kind [recorded state]):\n");
    for entry in &t.entries {
        output.push_str(&format!(
            "  {}: {} {} [{}]",
            entry.sequence,
            label(&entry.phase)?,
            entry.kind,
            label(&entry.state)?,
        ));
        if let Some(attempt) = entry.attempt {
            output.push_str(&format!(" attempt {attempt}"));
        }
        if entry.after_cancel_request {
            output.push_str(" after cancel request");
        }
        output.push('\n');
    }
    output.push_str("Phases:\n");
    for phase in &t.phases {
        output.push_str(&format!(
            "  {} {} (first {}, last {})\n",
            label(&phase.phase)?,
            phase.events,
            sequence(phase.first_sequence),
            sequence(phase.last_sequence),
        ));
    }
    output.push_str(&format!(
        "Approval: {} (awaiting {}, granted {}; state transitions only)\n",
        label(&t.approval.outcome)?,
        sequence(t.approval.awaiting_sequence),
        sequence(t.approval.granted_sequence),
    ));
    output.push_str(&format!(
        "Cancellation: requested {} | before start {} | terminal cancelled: {} | events after request: {} (logical only)\n",
        sequence(t.cancellation.requested_sequence),
        sequence(t.cancellation.before_start_sequence),
        if t.cancellation.terminal_cancelled { "yes" } else { "no" },
        t.cancellation.events_after_request,
    ));
    output.push_str(&format!(
        "Cleanup: recorded {} | confirmed marker {} | unknown marker {} | physical shutdown unknown\n",
        label(&t.cleanup.recorded)?,
        sequence(t.cleanup.confirmed_sequence),
        sequence(t.cleanup.unknown_sequence),
    ));
    output.push_str(&format!(
        "Verification: {} (record {}) | Objective: {}\n",
        label(&t.verification.state)?,
        sequence(t.verification.record_sequence),
        label(&t.unknown.objective)?,
    ));
    for attempt in &t.attempts {
        let requested = match &attempt.requested {
            Some(requested) => format!(
                "{} effort {}",
                label(&requested.model)?,
                label(&requested.effort)?
            ),
            None => "unknown".into(),
        };
        let cleanup = match &attempt.recorded_cleanup {
            Some(cleanup) => label(cleanup)?,
            None => "unknown".into(),
        };
        let after_cancel = match attempt.result_after_cancel_request {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        let after_cancel = match attempt.cancel_request_sequence {
            Some(request) if attempt.result_after_cancel_request == Some(true) => {
                format!("{after_cancel} (request {request})")
            }
            _ => after_cancel.to_owned(),
        };
        let capture = v3
            .timings
            .iter()
            .find(|timing| timing.ordinal == attempt.ordinal)
            .map(|timing| match timing.measurement {
                router_runtime::CaptureTiming::Known { duration_ms } => {
                    Ok(format!("{duration_ms} ms"))
                }
                router_runtime::CaptureTiming::Unknown { reason } => {
                    label(&reason).map(|reason| format!("unknown ({reason})"))
                }
            })
            .transpose()?
            .unwrap_or_else(|| "unknown".into());
        output.push_str(&format!(
            "Attempt {}: requested {} | applied unknown/unknown | usage {} (tokens {}, cost {}, unattested) | recorded cleanup {} | result after cancel request {} | local supervisor capture {}\n",
            attempt.ordinal,
            requested,
            label(&attempt.usage.status)?,
            attempt
                .usage
                .tokens
                .map_or_else(|| "unknown".into(), |tokens| tokens.to_string()),
            microusd(attempt.usage.cost_microusd),
            cleanup,
            after_cancel,
            capture,
        ));
    }
    output.push_str(
        "Unknown: exact revision acceptance, notification delivery, lost notifications, event timestamps, physical cleanup, objective, applied model and effort\n",
    );
    Ok(output)
}

fn render_findings(findings: &router_runtime::RunFindings) -> Result<String, CliError> {
    let mut output = format!(
        "Findings (rules v{}, advisory only, no automatic action; source snapshot {}):\n",
        findings.rules_version, findings.source_snapshot_sha256,
    );
    if findings.findings.is_empty() {
        output.push_str("Findings: none\n");
    }
    for finding in &findings.findings {
        let attempts = finding
            .affected_attempts
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let evidence = finding
            .evidence
            .iter()
            .map(|e| format!("event {}", e.sequence))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "[{}] {} id {}\n  Attempts {} | Basis: {} | Evidence: {}\n  Fact: {}\n  Hypothesis: {}\n  Preserve: {}\n  Check: {}\n",
            label(&finding.severity)?,
            label(&finding.rule)?,
            finding.id,
            attempts,
            label(&finding.basis)?,
            evidence,
            finding.fact,
            finding.hypothesis,
            finding.preserve.join("; "),
            finding.check,
        ));
    }
    output.push_str("Suppressed rules (rule: reason):\n");
    for suppressed in &findings.suppressed {
        output.push_str(&format!(
            "  {}: {}\n",
            label(&suppressed.rule)?,
            label(&suppressed.reason)?,
        ));
    }
    Ok(output)
}
