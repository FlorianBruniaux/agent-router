// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Offline translation of Omnigent's routing API, not execution authority.
//!
//! The parent must construct a locally authorized phase context for this exact
//! task. The remote request can only narrow its candidate pool. It cannot set
//! permissions, budgets, model prefixes, native capabilities, or host mappings.
use std::collections::BTreeSet;

use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_types::PhaseReceipt;
use serde::{Deserialize, Serialize};

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const MAX_ROUTE_OPTIONS: usize = 64;
pub const MAX_PROMPT_CHARACTERS: usize = 4_000;
pub const ROUTER_NAME: &str = "agent-router-phase-v1";

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum RoutingError {
    #[error("omnigent.input.invalid")]
    Invalid,
    #[error("omnigent.input.limit")]
    Limit,
    #[error("omnigent.strategy.unsupported")]
    Strategy,
    #[error("omnigent.context.mismatch")]
    Context,
    #[error("omnigent.routing.refused")]
    Refused,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
pub struct RouteOption {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    prompt: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selector {
    router_name: String,
    #[serde(default)]
    config: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    route_options: Vec<RouteOption>,
    task: Task,
    route_selector: Selector,
    // The public reference does not specify the SessionHistory fields. Accept
    // a bounded object as advisory data, but never reinterpret it as authority.
    #[serde(default)]
    session_history: serde_json::Map<String, serde_json::Value>,
}

/// Validated, bounded request. Construction always passes the byte parser.
pub struct RoutingRequest {
    options: Vec<RouteOption>,
    prompt: String,
}

impl RoutingRequest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, RoutingError> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(RoutingError::Limit);
        }
        let wire: WireRequest = serde_json::from_slice(bytes).map_err(|_| RoutingError::Invalid)?;
        if wire.route_selector.router_name != ROUTER_NAME || !wire.route_selector.config.is_empty()
        {
            return Err(RoutingError::Strategy);
        }
        if wire.route_options.len() > MAX_ROUTE_OPTIONS
            || wire.task.prompt.chars().count() > MAX_PROMPT_CHARACTERS
        {
            return Err(RoutingError::Limit);
        }
        if wire.route_options.is_empty()
            || wire.task.prompt.trim().is_empty()
            || wire.route_options.iter().any(|option| {
                !identifier(&option.model)
                    || option.harness.as_deref().is_some_and(|s| !identifier(s))
            })
            || wire.route_options.iter().collect::<BTreeSet<_>>().len() != wire.route_options.len()
        {
            return Err(RoutingError::Invalid);
        }
        // Discard history after bounded parsing: never retain or echo it in a
        // receipt, error, explanation, or request-debug representation.
        drop(wire.session_history);
        Ok(Self {
            options: wire.route_options,
            prompt: wire.task.prompt,
        })
    }
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-/:".contains(&byte))
}

/// An explicit parent mapping for one locally observed harness. Missing remote
/// harness is a separate mapping, never an automatic native-harness inference.
pub struct HarnessBinding {
    local: String,
    remote: Option<String>,
}

impl HarnessBinding {
    pub fn new(local: String, remote: Option<String>) -> Result<Self, RoutingError> {
        if !identifier(&local) || remote.as_deref().is_some_and(|s| !identifier(s)) {
            return Err(RoutingError::Invalid);
        }
        Ok(Self { local, remote })
    }
}

#[derive(Debug, Serialize)]
pub struct RouteSelection {
    pub route_option: RouteOption,
}

#[derive(Debug, Serialize)]
pub struct RoutingResponse {
    pub route_selection: Vec<RouteSelection>,
    pub rationale: &'static str,
}

/// Keep the full receipt locally. The wire response is advisory and does not
/// carry an execution approval or guarantee that Omnigent honors effort/assets.
pub struct RoutingOutcome {
    pub response: RoutingResponse,
    pub receipt: PhaseReceipt,
}

pub fn select_route(
    request: &RoutingRequest,
    router: &PhaseRouter,
    context: &ValidatedPhaseContext,
    binding: &HarnessBinding,
) -> Result<RoutingOutcome, RoutingError> {
    if context.snapshot().host.harness.as_str() != binding.local
        || context.request().task.objective != request.prompt
    {
        return Err(RoutingError::Context);
    }
    let offered: BTreeSet<_> = request
        .options
        .iter()
        .filter(|option| option.harness == binding.remote)
        .map(|option| option.model.as_str())
        .collect();
    // Explicit offer filtering uses the existing immutable fallback contract.
    // An unavailable ID here denotes absence from this offer, not a measured
    // provider outage. No new candidate or evidence is synthesized.
    let unavailable = context
        .request()
        .candidates
        .iter()
        .filter(|candidate| !offered.contains(candidate.model.as_str()))
        .map(|candidate| candidate.id.clone())
        .collect();
    let receipt = router
        .fallback(context, &unavailable)
        .map_err(|_| RoutingError::Refused)?;
    let proof = router
        .validate_receipt(context, &receipt)
        .map_err(|_| RoutingError::Refused)?;
    let selected = request
        .options
        .iter()
        .find(|option| {
            option.harness == binding.remote
                && option.model == proof.selection().candidate.model.as_str()
        })
        .ok_or(RoutingError::Refused)?;
    Ok(RoutingOutcome {
        response: RoutingResponse {
            route_selection: vec![RouteSelection {
                route_option: selected.clone(),
            }],
            rationale: "Selected from offered routes under the local phase policy. This is a routing recommendation, not execution approval.",
        },
        receipt,
    })
}
