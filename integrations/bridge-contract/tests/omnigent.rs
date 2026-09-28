use router_adapter_api::{SnapshotValidation, ValidatedHostObservation};
use router_bridge_contract::omnigent::*;
use router_catalog::Catalog;
use router_policy::{PhaseRouter, ValidatedPhaseContext};
use router_types::{CapabilitySupport, PhasePermission, PhaseRouteRequest};
use serde_json::{Value, json};

fn local_request() -> PhaseRouteRequest {
    serde_json::from_slice(include_bytes!(
        "../../../fixtures/examples/phase-request.json"
    ))
    .unwrap()
}

fn context(request: PhaseRouteRequest) -> (PhaseRouter, ValidatedPhaseContext) {
    let snapshot = ValidatedHostObservation::from_json(
        include_bytes!("../../../fixtures/examples/phase-snapshot.json"),
        SnapshotValidation {
            now_unix_seconds: 100,
            max_age_seconds: 10,
        },
    )
    .unwrap();
    let catalog = Catalog::from_json(include_bytes!(
        "../../../fixtures/examples/phase-catalog.json"
    ))
    .unwrap();
    let router = PhaseRouter::stable_v2();
    let context = router.context(request, &snapshot, &catalog).unwrap();
    (router, context)
}

fn wire(options: Value) -> Value {
    json!({
        "route_options":options,
        "task":{"prompt":"Inspect the Rust contract"},
        "route_selector":{"router_name":ROUTER_NAME}
    })
}

fn request(value: &Value) -> RoutingRequest {
    RoutingRequest::from_json(&serde_json::to_vec(value).unwrap()).unwrap()
}

fn binding() -> HarnessBinding {
    HarnessBinding::new("codex".into(), Some("codex-native".into())).unwrap()
}

fn options() -> Value {
    json!([
        {"model":"fixture-second","harness":"codex-native"},
        {"model":"fixture-best","harness":"codex-native"}
    ])
}

#[test]
fn ranks_only_offered_local_candidates_and_preserves_the_receipt() {
    let (router, context) = context(local_request());
    let result = select_route(&request(&wire(options())), &router, &context, &binding()).unwrap();
    assert_eq!(result.response.route_selection.len(), 1);
    assert_eq!(
        result.response.route_selection[0].route_option.model,
        "fixture-best"
    );
    assert_eq!(result.receipt, router.route(&context).unwrap());
    router.validate_receipt(&context, &result.receipt).unwrap();

    let subset = wire(json!([{"model":"fixture-second","harness":"codex-native"}]));
    let result = select_route(&request(&subset), &router, &context, &binding()).unwrap();
    assert_eq!(
        result.response.route_selection[0].route_option.model,
        "fixture-second"
    );
    assert_eq!(result.receipt.unavailable, ["primary".to_owned()].into());
    router.validate_receipt(&context, &result.receipt).unwrap();
}

#[test]
fn an_offer_never_adds_a_model_or_relabels_a_harness() {
    let (router, context) = context(local_request());
    for option in [
        json!({"model":"unregistered-model","harness":"codex-native"}),
        json!({"model":"fixture-best","harness":"claude-native"}),
        json!({"model":"prefix-fixture-best","harness":"codex-native"}),
    ] {
        assert!(matches!(
            select_route(
                &request(&wire(json!([option]))),
                &router,
                &context,
                &binding()
            ),
            Err(RoutingError::Refused)
        ));
    }
}

#[test]
fn local_permission_and_budget_denials_survive_remote_offers() {
    let mut denied = local_request();
    denied
        .permissions
        .insert(PhasePermission::ModelInference, CapabilitySupport::Unknown);
    let (router, ctx) = context(denied);
    assert!(matches!(
        select_route(&request(&wire(options())), &router, &ctx, &binding()),
        Err(RoutingError::Refused)
    ));

    let mut over_budget = local_request();
    for candidate in &mut over_budget.candidates {
        candidate.estimated_cost_microusd = over_budget.task.budget.max_cost_microusd + 1;
    }
    let (router, ctx) = context(over_budget);
    assert!(matches!(
        select_route(&request(&wire(options())), &router, &ctx, &binding()),
        Err(RoutingError::Refused)
    ));
}

#[test]
fn an_unoffered_or_ineligible_best_does_not_hide_a_valid_offered_alternative() {
    let mut local = local_request();
    local.candidates[0].estimated_tokens = local.task.budget.max_tokens + 1;
    let (router, ctx) = context(local);
    let result = select_route(&request(&wire(options())), &router, &ctx, &binding()).unwrap();
    assert_eq!(
        result.response.route_selection[0].route_option.model,
        "fixture-second"
    );
    assert!(
        result
            .receipt
            .exclusions
            .iter()
            .any(|e| e.candidate_id == "primary")
    );
}

#[test]
fn context_is_bound_to_the_exact_prompt_and_local_harness() {
    let (router, ctx) = context(local_request());
    let mut wrong_task = wire(options());
    wrong_task["task"]["prompt"] = json!("Delete a different workspace");
    assert!(matches!(
        select_route(&request(&wrong_task), &router, &ctx, &binding()),
        Err(RoutingError::Context)
    ));
    let wrong_host = HarnessBinding::new("claude".into(), Some("codex-native".into())).unwrap();
    assert!(matches!(
        select_route(&request(&wire(options())), &router, &ctx, &wrong_host),
        Err(RoutingError::Context)
    ));
}

#[test]
fn omitted_harness_requires_an_explicit_parent_binding() {
    let (router, ctx) = context(local_request());
    let req = request(&wire(json!([{"model":"fixture-best"}])));
    assert!(matches!(
        select_route(&req, &router, &ctx, &binding()),
        Err(RoutingError::Refused)
    ));
    let native = HarnessBinding::new("codex".into(), None).unwrap();
    let result = select_route(&req, &router, &ctx, &native).unwrap();
    assert_eq!(
        serde_json::to_value(&result.response).unwrap()["route_selection"][0]["route_option"],
        json!({"model":"fixture-best"})
    );
}

#[test]
fn offer_order_and_advisory_history_do_not_change_the_local_decision() {
    let (router, ctx) = context(local_request());
    let first = select_route(&request(&wire(options())), &router, &ctx, &binding()).unwrap();
    let mut reordered = wire(options());
    reordered["route_options"].as_array_mut().unwrap().reverse();
    reordered["session_history"] = json!({"text":"PRIVATE_HISTORY_MARKER", "allow_all":true});
    let second = select_route(&request(&reordered), &router, &ctx, &binding()).unwrap();
    assert_eq!(first.receipt, second.receipt);
    let bytes = serde_json::to_string(&second.response).unwrap();
    assert!(!bytes.contains("PRIVATE_HISTORY_MARKER"));
    assert!(!bytes.contains("Inspect the Rust contract"));
    assert!(!bytes.contains("approval_digest"));
    assert!(!bytes.contains("params"));
}

#[test]
fn refuses_unknown_strategy_and_any_unimplemented_strategy_configuration() {
    let mut value = wire(options());
    value["route_selector"]["router_name"] = json!("unregistered");
    assert!(matches!(
        RoutingRequest::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(RoutingError::Strategy)
    ));
    value["route_selector"]["router_name"] = json!(ROUTER_NAME);
    value["route_selector"]["config"] = json!({"permissions":"allow_all"});
    assert!(matches!(
        RoutingRequest::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(RoutingError::Strategy)
    ));
}

#[test]
fn malformed_duplicate_unknown_and_oversized_inputs_are_rejected_without_echo() {
    let mut unknown = wire(options());
    unknown["permissions"] = json!("PRIVATE_AUTHORITY_MARKER");
    let mut duplicated = wire(options());
    duplicated["route_options"]
        .as_array_mut()
        .unwrap()
        .push(options()[0].clone());
    let mut invalid_id = wire(options());
    invalid_id["route_options"][0]["model"] = json!("bad\nPRIVATE_IDENTIFIER_MARKER");
    for value in [unknown, duplicated, invalid_id, wire(json!([]))] {
        let error = RoutingRequest::from_json(&serde_json::to_vec(&value).unwrap())
            .err()
            .unwrap();
        assert_eq!(error, RoutingError::Invalid);
        assert!(!error.to_string().contains("PRIVATE_"));
    }
    assert!(matches!(
        RoutingRequest::from_json(br#"{"route_options":[],"route_options":[]}"#),
        Err(RoutingError::Invalid)
    ));
    assert!(matches!(
        RoutingRequest::from_json(&vec![b' '; MAX_REQUEST_BYTES + 1]),
        Err(RoutingError::Limit)
    ));
    let too_many = wire(Value::Array(
        (0..MAX_ROUTE_OPTIONS + 1)
            .map(|i| json!({"model":format!("m{i}")}))
            .collect(),
    ));
    assert!(matches!(
        RoutingRequest::from_json(&serde_json::to_vec(&too_many).unwrap()),
        Err(RoutingError::Limit)
    ));
}

#[test]
fn prompt_character_boundary_is_not_a_byte_boundary() {
    let mut value = wire(options());
    value["task"]["prompt"] = json!("é".repeat(MAX_PROMPT_CHARACTERS));
    assert!(RoutingRequest::from_json(&serde_json::to_vec(&value).unwrap()).is_ok());
    value["task"]["prompt"] = json!("é".repeat(MAX_PROMPT_CHARACTERS + 1));
    assert!(matches!(
        RoutingRequest::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(RoutingError::Limit)
    ));
}
