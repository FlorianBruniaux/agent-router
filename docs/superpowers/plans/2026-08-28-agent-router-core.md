# Agent Router Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a deterministic Rust CLI that selects and explains a compatible `plan-execute-verify` route from a normalized task, host snapshot, catalog, and skill candidates.

**Architecture:** A four-crate Cargo workspace keeps serialized contracts independent from catalog validation, policy logic, and CLI presentation. Hard constraints remove incompatible stage-model pairs before deterministic scoring. The initial CLI is read-only and consumes JSON files; existing BM25 output enters through a versioned skill-candidate contract rather than being reimplemented.

**Tech Stack:** Rust 2024 edition, MSRV 1.85, Cargo workspace, Serde, serde_json, thiserror, Clap, SHA-256, assert_cmd, predicates, tempfile.

**Spec:** `docs/superpowers/specs/2026-08-28-agent-router-core-design.md`

## Global Constraints

- The first slice is deterministic, read-only, local-first, and performs no network calls.
- Do not modify `~/.config/ai-agents`, `~/.codex`, `~/.claude`, global hooks, plugins, or skills.
- Exact model identifiers are stored in catalogs. Bare aliases such as `sol`, `terra`, and `luna` are invalid catalog IDs.
- Hard constraint exclusions cannot be overridden by ranking scores.
- Unknown capability excludes high-risk work and is visible in the decision trace.
- Empty eligible candidate sets produce a typed `no_route` decision, never first-entry fallback.
- Repeated routing of identical JSON inputs and catalog bytes must produce byte-identical JSON.
- Library crates use typed errors. `anyhow` is not used in public library APIs.
- Production behavior follows red-green-refactor. Every new behavior must first be observed failing for the expected reason.
- Workspace CI rejects formatting drift and Clippy warnings.

---

## File map

```text
Cargo.toml                                      workspace metadata and shared dependencies
rust-toolchain.toml                             MSRV toolchain declaration
crates/router-types/Cargo.toml                  serialization-only crate manifest
crates/router-types/src/lib.rs                  module exports
crates/router-types/src/asset.rs                typed IDs, model and skill metadata
crates/router-types/src/task.rs                 task, risk, capabilities and host snapshot
crates/router-types/src/decision.rs             plans, stages, exclusions and output envelope
crates/router-types/tests/contracts.rs           serialization and ordering contracts
crates/router-catalog/Cargo.toml                catalog crate manifest
crates/router-catalog/src/lib.rs                validated catalog API
crates/router-catalog/src/error.rs              catalog validation errors
crates/router-catalog/src/validate.rs           exact ID and semantic validation
crates/router-catalog/tests/catalog_validation.rs catalog red-green tests
assets/catalogs/codex-core-v1.json               minimal exact-ID Codex catalog
crates/router-policy/Cargo.toml                 policy crate manifest
crates/router-policy/src/lib.rs                 public routing API
crates/router-policy/src/constraint.rs          hard constraint evaluators
crates/router-policy/src/preset.rs              plan-execute-verify candidate builder
crates/router-policy/src/score.rs               deterministic score and tie-breaker
crates/router-policy/src/route.rs               orchestration and decision trace
crates/router-policy/src/retrieval.rs           SkillRetriever port and file DTO
crates/router-policy/tests/constraints.rs        exclusion invariants
crates/router-policy/tests/routing.rs            selected plan and determinism tests
crates/router-policy/tests/retrieval.rs          BM25 boundary contract tests
crates/agent-router-cli/Cargo.toml               binary manifest
crates/agent-router-cli/src/main.rs              process entry point
crates/agent-router-cli/src/cli.rs               Clap command model
crates/agent-router-cli/src/command.rs           route and explain handlers
crates/agent-router-cli/src/render.rs            JSON and human output
crates/agent-router-cli/tests/cli.rs              end-to-end CLI tests
tests/fixtures/requests/*.json                   route request fixtures
tests/fixtures/catalogs/*.json                   valid and invalid catalog fixtures
tests/fixtures/decisions/*.json                  stable golden outputs
docs/reference/json-contracts.md                 public input and output examples
```

## Task 1: Workspace and stable domain contracts

**Files:**

- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Create: `crates/router-types/Cargo.toml`
- Create: `crates/router-types/src/lib.rs`
- Create: `crates/router-types/src/asset.rs`
- Create: `crates/router-types/src/task.rs`
- Create: `crates/router-types/src/decision.rs`
- Create: `crates/router-types/tests/contracts.rs`

**Interfaces:**

- Consumes: no internal crate.
- Produces: `ModelId`, `SkillId`, `HarnessId`, `EffortLevel`, `StageKind`, `Capability`, `RiskLevel`, `TaskContext`, `HostSnapshot`, `ModelSpec`, `SkillCandidate`, `RouteRequest`, `PlanStage`, `ExecutionPlan`, `Exclusion`, `RouteDecision`, and `DecisionEnvelope`.

- [ ] **Step 1: Create the workspace manifests only**

Create a virtual workspace with members `crates/router-types`, `crates/router-catalog`, `crates/router-policy`, and `crates/agent-router-cli`. Set `resolver = "2"`, edition `2024`, `rust-version = "1.85"`, dual licence `MIT OR Apache-2.0`, and shared dependency versions.

The root shared dependency block is:

```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }
sha2 = "0.10"
hex = "0.4"
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

Create only the `router-types` member directory at this step. Temporarily comment the three not-yet-created workspace members so Cargo can run. They are enabled in their own tasks.

- [ ] **Step 2: Write the failing serialization contract tests**

In `crates/router-types/tests/contracts.rs`, create tests with this public API:

```rust
use std::collections::{BTreeMap, BTreeSet};

use router_types::{
    Capability, EffortLevel, HarnessId, HostSnapshot, ModelId, RiskLevel,
    SkillCandidate, SkillId, TaskContext, TaskKind,
};

#[test]
fn model_id_rejects_bare_tier_aliases() {
    let error = ModelId::parse("sol").expect_err("bare aliases must be rejected");
    assert_eq!(error.to_string(), "model id 'sol' is not canonical");
}

#[test]
fn host_snapshot_round_trips_with_stable_set_order() {
    let snapshot = HostSnapshot {
        id: "fixture-codex".into(),
        harness: HarnessId::parse("codex").unwrap(),
        supported_models: BTreeSet::from([
            ModelId::parse("gpt-5.6-terra").unwrap(),
            ModelId::parse("gpt-5.6-sol").unwrap(),
        ]),
        supported_efforts: BTreeMap::from([(
            ModelId::parse("gpt-5.6-sol").unwrap(),
            BTreeSet::from([EffortLevel::High, EffortLevel::Xhigh]),
        )]),
        capabilities: BTreeSet::from([Capability::Tools, Capability::Subagents]),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let decoded: HostSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(json.find("gpt-5.6-sol").unwrap() < json.find("gpt-5.6-terra").unwrap());
}

#[test]
fn task_context_requires_explicit_risk_and_acceptance() {
    let task = TaskContext {
        objective: "Implement a bounded parser change".into(),
        kind: TaskKind::Implement,
        risk: RiskLevel::Medium,
        required_capabilities: BTreeSet::from([Capability::Tools]),
        acceptance: vec!["workspace tests pass".into()],
    };
    assert_eq!(task.risk, RiskLevel::Medium);
    assert_eq!(task.acceptance.len(), 1);
}

#[test]
fn skill_candidate_preserves_retrieval_source_and_score() {
    let candidate = SkillCandidate {
        skill: SkillId::parse("tdd").unwrap(),
        source: "bm25-hook-v1".into(),
        score_micros: 812_500,
    };
    assert_eq!(candidate.score_micros, 812_500);
}
```

- [ ] **Step 3: Run the contract tests and verify RED**

Run:

```bash
cargo test -p router-types --test contracts
```

Expected: compilation fails because the `router_types` public types and modules do not exist. This is the correct RED reason.

- [ ] **Step 4: Implement the minimal domain types**

Implement validated string newtypes with private fields and `parse`, `as_str`, `Display`, `Serialize`, `Deserialize`, `Ord`, and `Hash`. `ModelId::parse` rejects empty values and the exact pseudo IDs `sol`, `terra`, `luna`, `mini`, `spark`, and `deep`. Other IDs remain opaque because provider naming differs.

Use integer micro-scores instead of floats in serialized decisions. Define enums with lower snake-case Serde names:

```rust
pub enum EffortLevel { None, Minimal, Low, Medium, High, Xhigh, Max, Ultra }
pub enum StageKind { Plan, Execute, Verify }
pub enum Capability { Tools, Subagents, Vision, Network, WorkspaceWrite }
pub enum RiskLevel { Low, Medium, High, Critical }
pub enum TaskKind { Explore, Implement, Review, Architecture, Security, Migration }
```

Use `BTreeSet` and `BTreeMap` for all serialized collections whose order affects deterministic JSON.

- [ ] **Step 5: Add decision contracts before implementing their producers**

Define:

```rust
pub struct PlanStage {
    pub kind: StageKind,
    pub model: ModelId,
    pub effort: EffortLevel,
    pub skills: Vec<SkillId>,
}

pub struct ExecutionPlan {
    pub preset: String,
    pub stages: Vec<PlanStage>,
    pub utility_micros: i64,
}

pub struct Exclusion {
    pub stage: StageKind,
    pub model: ModelId,
    pub code: String,
    pub evidence: String,
}

pub enum RouteDecision {
    Selected {
        plan: ExecutionPlan,
        alternatives: Vec<ExecutionPlan>,
        exclusions: Vec<Exclusion>,
        missing_evidence: Vec<String>,
    },
    NoRoute {
        exclusions: Vec<Exclusion>,
        missing_evidence: Vec<String>,
    },
}

pub struct DecisionEnvelope {
    pub schema_version: String,
    pub decision_id: String,
    pub catalog_version: String,
    pub policy_version: String,
    pub decision: RouteDecision,
}
```

- [ ] **Step 6: Run router-types tests and verify GREEN**

Run:

```bash
cargo test -p router-types
cargo fmt --check
```

Expected: all router-types tests pass and formatting is clean.

- [ ] **Step 7: Commit the domain contracts**

```bash
git add Cargo.toml rust-toolchain.toml crates/router-types
git commit -m "feat(types): define deterministic routing contracts"
```

## Task 2: Validated model and skill catalog

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/router-catalog/Cargo.toml`
- Create: `crates/router-catalog/src/lib.rs`
- Create: `crates/router-catalog/src/error.rs`
- Create: `crates/router-catalog/src/validate.rs`
- Create: `crates/router-catalog/tests/catalog_validation.rs`
- Create: `assets/catalogs/codex-core-v1.json`
- Create: `tests/fixtures/catalogs/invalid-alias.json`
- Create: `tests/fixtures/catalogs/duplicate-model.json`

**Interfaces:**

- Consumes: `ModelId`, `SkillId`, `ModelSpec`, `SkillSpec`, `StageKind`, `EffortLevel`, and `Capability` from `router-types`.
- Produces: `Catalog::from_json(bytes) -> Result<Catalog, CatalogError>`, `Catalog::model(&ModelId)`, `Catalog::skill(&SkillId)`, and `Catalog::version()`.

- [ ] **Step 1: Enable the catalog workspace member and write failing tests**

Tests must cover:

```rust
#[test]
fn rejects_bare_model_alias_in_catalog() {
    let bytes = include_bytes!("../../../tests/fixtures/catalogs/invalid-alias.json");
    let error = Catalog::from_json(bytes).unwrap_err();
    assert_eq!(error.code(), "catalog.model.non_canonical_id");
}

#[test]
fn rejects_duplicate_model_ids() {
    let bytes = include_bytes!("../../../tests/fixtures/catalogs/duplicate-model.json");
    let error = Catalog::from_json(bytes).unwrap_err();
    assert_eq!(error.code(), "catalog.model.duplicate_id");
}

#[test]
fn loads_exact_codex_models() {
    let bytes = include_bytes!("../../../assets/catalogs/codex-core-v1.json");
    let catalog = Catalog::from_json(bytes).unwrap();
    assert!(catalog.model(&ModelId::parse("gpt-5.6-sol").unwrap()).is_some());
    assert!(catalog.model(&ModelId::parse("gpt-5.6-terra").unwrap()).is_some());
    assert!(catalog.model(&ModelId::parse("gpt-5.6-luna").unwrap()).is_some());
}
```

- [ ] **Step 2: Run catalog tests and verify RED**

Run:

```bash
cargo test -p router-catalog --test catalog_validation
```

Expected: compilation fails because `Catalog` and `CatalogError` are absent.

- [ ] **Step 3: Implement parsing with raw rows followed by semantic validation**

Deserialize into `RawCatalog` and `RawModelSpec` first so duplicate IDs are detectable before conversion into maps. Convert only after checking:

- schema version is exactly `1`;
- catalog version is non-empty;
- IDs parse and are unique;
- every model has at least one supported stage and effort;
- relative quality is in `0..=1_000_000`;
- relative cost and latency are non-negative integers;
- every skill ID is unique;
- skill trust is one of `builtin`, `trusted_local`, `verified_third_party`, `untrusted`, `blocked`.

Return `CatalogError { code, path, message }`. Do not collapse semantic errors into a generic parse error.

- [ ] **Step 4: Create the minimal Codex catalog**

The built-in catalog declares:

```text
gpt-5.6-sol: plan, execute, verify; high, xhigh, max; quality 950000; cost 900000
gpt-5.6-terra: execute, verify; medium, high, xhigh; quality 820000; cost 550000
gpt-5.6-luna: execute; low, medium, high; quality 650000; cost 200000
```

All three require `tools`. Sol and Terra declare `subagents`; Luna does not. These numbers are policy priors, not claims about benchmark percentages, and the catalog metadata must label them `source: "builtin-policy-prior"`.

Add built-in skill metadata for `tdd`, `critique-plan`, and `security-guardian`. Store descriptions and requirements only, not skill bodies.

- [ ] **Step 5: Run catalog tests and verify GREEN**

Run:

```bash
cargo test -p router-catalog
```

Expected: valid catalog loads and invalid fixtures return their exact codes.

- [ ] **Step 6: Commit the catalog**

```bash
git add Cargo.toml crates/router-catalog assets/catalogs tests/fixtures/catalogs
git commit -m "feat(catalog): validate exact model and skill metadata"
```

## Task 3: Hard constraints and fail-closed eligibility

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/router-policy/Cargo.toml`
- Create: `crates/router-policy/src/lib.rs`
- Create: `crates/router-policy/src/constraint.rs`
- Create: `crates/router-policy/tests/constraints.rs`

**Interfaces:**

- Consumes: `TaskContext`, `HostSnapshot`, `ModelSpec`, `StageKind`, and `EffortLevel`.
- Produces: `ConstraintEngine::evaluate(&EligibilityContext) -> Vec<ConstraintVerdict>` and `is_eligible(&[ConstraintVerdict]) -> bool`.

- [ ] **Step 1: Write a failing test proving score cannot rescue incompatibility**

```rust
#[test]
fn unavailable_high_quality_model_is_excluded() {
    let context = fixture_context(
        "gpt-5.6-sol",
        StageKind::Plan,
        EffortLevel::High,
        RiskLevel::Medium,
    );
    let verdicts = ConstraintEngine::default().evaluate(&context);
    assert!(!is_eligible(&verdicts));
    assert!(verdicts.iter().any(|v| v.code() == "host.model.unavailable"));
}
```

The host fixture deliberately supports Terra and Luna but not Sol, while the model prior gives Sol maximum quality.

- [ ] **Step 2: Write failing tests for effort, capability, stage, and unknown high-risk evidence**

Required tests and codes:

```text
unsupported effort              -> host.effort.unsupported
missing required capability     -> host.capability.missing
model cannot serve stage        -> model.stage.unsupported
unknown high-risk capability    -> host.capability.unknown_high_risk
```

Represent host capability knowledge separately from the supported set so absent and unknown are not conflated. Use:

```rust
pub enum Support { Supported, Unsupported, Unknown }
```

- [ ] **Step 3: Run constraints tests and verify RED**

Run:

```bash
cargo test -p router-policy --test constraints
```

Expected: compilation fails because the constraint engine is absent.

- [ ] **Step 4: Implement composable constraints**

Define:

```rust
pub enum ConstraintVerdict {
    Eligible { code: &'static str },
    Excluded { code: &'static str, evidence: String },
    Unknown { code: &'static str, evidence: String },
}

pub trait Constraint: Send + Sync {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict;
}
```

Create `ModelAvailable`, `EffortSupported`, `StageSupported`, and `CapabilitiesSatisfied`. `ConstraintEngine::default` evaluates them in this fixed order. `is_eligible` returns false for `Excluded`, and also for `Unknown` when task risk is high or critical. Preserve every verdict for explanation.

- [ ] **Step 5: Run constraints tests and verify GREEN**

Run:

```bash
cargo test -p router-policy --test constraints
```

Expected: every incompatible fixture is excluded with the exact stable code.

- [ ] **Step 6: Add an invariant test over all fixture models**

Iterate through the catalog with a host snapshot that supports no models. Assert zero eligible stage-model pairs. This test would fail if any future constraint path adds a first-entry fallback.

- [ ] **Step 7: Commit the constraint engine**

```bash
git add Cargo.toml crates/router-policy
git commit -m "feat(policy): enforce hard routing constraints"
```

## Task 4: Skill retrieval boundary for the existing BM25 system

**Files:**

- Create: `crates/router-policy/src/retrieval.rs`
- Modify: `crates/router-policy/src/lib.rs`
- Create: `crates/router-policy/tests/retrieval.rs`
- Create: `tests/fixtures/requests/bm25-candidates.json`

**Interfaces:**

- Consumes: versioned JSON candidate output with canonical skill ID, source, integer score, and optional path.
- Produces: `SkillRetriever` trait, `StaticSkillRetriever`, and `parse_candidate_document(bytes) -> Result<Vec<SkillCandidate>, RetrievalError>`.

- [ ] **Step 1: Write failing boundary tests**

```rust
#[test]
fn parses_versioned_bm25_candidates_without_loading_skill_bodies() {
    let bytes = include_bytes!("../../../tests/fixtures/requests/bm25-candidates.json");
    let candidates = parse_candidate_document(bytes).unwrap();
    assert_eq!(candidates[0].skill.as_str(), "tdd");
    assert_eq!(candidates[0].source, "ai-agents-bm25-v1");
    assert_eq!(candidates[0].score_micros, 910_000);
}

#[test]
fn rejects_unknown_candidate_schema() {
    let bytes = br#"{"schema_version":"2","candidates":[]}"#;
    let error = parse_candidate_document(bytes).unwrap_err();
    assert_eq!(error.code(), "retrieval.schema.unsupported");
}
```

- [ ] **Step 2: Run retrieval tests and verify RED**

Run:

```bash
cargo test -p router-policy --test retrieval
```

Expected: missing retrieval module or functions.

- [ ] **Step 3: Implement the retrieval port and JSON adapter**

```rust
pub trait SkillRetriever {
    fn retrieve(&self, task: &TaskContext) -> Result<Vec<SkillCandidate>, RetrievalError>;
}

pub struct StaticSkillRetriever {
    candidates: Vec<SkillCandidate>,
}
```

The JSON schema is:

```json
{
  "schema_version": "1",
  "source": "ai-agents-bm25-v1",
  "candidates": [
    { "skill": "tdd", "score_micros": 910000 }
  ]
}
```

Reject duplicate skill IDs, negative scores, scores above 1,000,000, unknown schema versions, and empty source values. Do not read the candidate path or `SKILL.md` body in this crate.

- [ ] **Step 4: Run retrieval tests and verify GREEN**

```bash
cargo test -p router-policy --test retrieval
```

- [ ] **Step 5: Commit the BM25 boundary**

```bash
git add crates/router-policy tests/fixtures/requests/bm25-candidates.json
git commit -m "feat(policy): define versioned BM25 candidate boundary"
```

## Task 5: Deterministic plan-execute-verify routing

**Files:**

- Create: `crates/router-policy/src/preset.rs`
- Create: `crates/router-policy/src/score.rs`
- Create: `crates/router-policy/src/route.rs`
- Modify: `crates/router-policy/src/lib.rs`
- Create: `crates/router-policy/tests/routing.rs`
- Create: `tests/fixtures/requests/codex-implementation.json`
- Create: `tests/fixtures/requests/codex-security.json`
- Create: `tests/fixtures/requests/no-compatible-plan.json`

**Interfaces:**

- Consumes: `RouteRequest`, validated `Catalog`, `PolicyConfig`, and retrieved skill candidates.
- Produces: `Router::route(&RouteRequest, &Catalog) -> Result<DecisionEnvelope, RouteError>`.

- [ ] **Step 1: Write the failing happy-path route test**

```rust
#[test]
fn codex_implementation_uses_sol_plan_terra_execute_and_terra_verify() {
    let catalog = load_builtin_catalog();
    let request = load_request("codex-implementation.json");
    let decision = Router::stable_v1().route(&request, &catalog).unwrap();
    let plan = decision.selected_plan().unwrap();

    assert_eq!(plan.stages[0].kind, StageKind::Plan);
    assert_eq!(plan.stages[0].model.as_str(), "gpt-5.6-sol");
    assert_eq!(plan.stages[0].effort, EffortLevel::High);
    assert_eq!(plan.stages[1].kind, StageKind::Execute);
    assert_eq!(plan.stages[1].model.as_str(), "gpt-5.6-terra");
    assert_eq!(plan.stages[1].effort, EffortLevel::Medium);
    assert_eq!(plan.stages[2].kind, StageKind::Verify);
    assert_eq!(plan.stages[2].model.as_str(), "gpt-5.6-terra");
    assert_eq!(plan.stages[2].effort, EffortLevel::High);
}
```

- [ ] **Step 2: Write failing tests for security and no-route behavior**

The security fixture requires Sol for planning and verification, rejects Luna because it lacks the `verify` stage and subagent capability, and attaches `security-guardian` when retrieved.

The no-compatible fixture asserts:

```rust
assert!(matches!(decision.decision, RouteDecision::NoRoute { .. }));
assert!(!decision.exclusions().is_empty());
```

- [ ] **Step 3: Write the failing determinism test**

Route the same request twice. Serialize both `DecisionEnvelope` values with `serde_json::to_vec`. Assert exact byte equality.

The decision ID is:

```text
sha256(schema_version + canonical_request_json + catalog_version + policy_version)
```

Use a delimiter or length-prefix encoding so concatenation cannot collide ambiguously.

- [ ] **Step 4: Run routing tests and verify RED**

Run:

```bash
cargo test -p router-policy --test routing
```

Expected: missing `Router`, preset, scoring, and route implementation.

- [ ] **Step 5: Implement candidate construction**

For each of the three stages, enumerate catalog models declaring that stage. Use preset defaults as preferred efforts:

```text
Plan: High
Execute: Medium
Verify: High
```

If a preferred effort is unavailable, try the nearest effort in this fixed order:

```text
preferred, one level higher, one level lower, then remaining levels from low to ultra
```

Every pair passes through `ConstraintEngine`. Excluded pairs remain in the trace.

- [ ] **Step 6: Implement integer scoring and tie-breaking**

Use:

```text
utility_micros =
  quality_micros
  - cost_micros / 4
  - latency_micros / 10
  + retrieved_skill_score_micros / 20
```

Plan quality is the minimum quality across its stages, which prevents one strong stage from hiding a weak bottleneck. Plan cost and latency are sums. Skill bonus applies only when the catalog skill supports the task kind and its requirements are satisfied.

Sort by descending utility, then ascending total cost, then lexicographic concatenation of stage model IDs. This makes ties stable.

- [ ] **Step 7: Implement risk-specific verification**

For high or critical risk, the verify stage must use a model whose quality prior is at least that of the execute model and that declares `verify`. It may use the same model ID only as an independent logical instance. Record `verification_independence: "fresh_instance"` in `PlanStage` for the verify stage.

- [ ] **Step 8: Implement selected and no-route envelopes**

Return the best plan and at most two alternatives. Sort exclusions by stage, model ID, and code. `missing_evidence` is sorted and deduplicated. If any stage has no eligible pair, return `NoRoute` without building a partial plan.

- [ ] **Step 9: Run routing tests and verify GREEN**

```bash
cargo test -p router-policy --test routing
cargo test -p router-policy
```

- [ ] **Step 10: Commit deterministic routing**

```bash
git add crates/router-policy tests/fixtures/requests
git commit -m "feat(policy): route deterministic plan execute verify workflows"
```

## Task 6: Read-only route and explain CLI

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/agent-router-cli/Cargo.toml`
- Create: `crates/agent-router-cli/src/main.rs`
- Create: `crates/agent-router-cli/src/cli.rs`
- Create: `crates/agent-router-cli/src/command.rs`
- Create: `crates/agent-router-cli/src/render.rs`
- Create: `crates/agent-router-cli/tests/cli.rs`
- Create: `tests/fixtures/decisions/codex-implementation.json`

**Interfaces:**

- Consumes: request JSON path, optional catalog path, decision JSON path.
- Produces: executable `agent-router`; commands `route --request <path> [--catalog <path>] --json` and `explain --decision <path>`.

- [ ] **Step 1: Enable the CLI workspace member and write failing end-to-end tests**

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn route_emits_versioned_json_without_mutating_home() {
    let temp_home = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("agent-router").unwrap();
    cmd.env("HOME", temp_home.path())
        .args([
            "route",
            "--request",
            "../../tests/fixtures/requests/codex-implementation.json",
            "--catalog",
            "../../assets/catalogs/codex-core-v1.json",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": \"1\""));

    assert_eq!(std::fs::read_dir(temp_home.path()).unwrap().count(), 0);
}

#[test]
fn no_route_has_a_distinct_exit_code() {
    let mut cmd = Command::cargo_bin("agent-router").unwrap();
    cmd.args([
        "route",
        "--request",
        "../../tests/fixtures/requests/no-compatible-plan.json",
        "--catalog",
        "../../assets/catalogs/codex-core-v1.json",
        "--json",
    ])
    .assert()
    .code(3)
    .stdout(predicate::str::contains("\"no_route\""));
}
```

- [ ] **Step 2: Run CLI tests and verify RED**

```bash
cargo test -p agent-router-cli --test cli
```

Expected: the binary target and commands do not exist.

- [ ] **Step 3: Implement Clap command parsing**

Use:

```rust
#[derive(clap::Parser)]
#[command(name = "agent-router", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand)]
pub enum Commands {
    Route {
        #[arg(long)] request: PathBuf,
        #[arg(long)] catalog: PathBuf,
        #[arg(long)] json: bool,
    },
    Explain {
        #[arg(long)] decision: PathBuf,
    },
}
```

The initial CLI requires an explicit catalog path. Embedded catalog defaults are deferred so tests and users always know which bytes produced a decision.

- [ ] **Step 4: Implement route error mapping**

Exit codes:

```text
0 selected route
2 malformed input or invalid catalog
3 valid request with no compatible route
4 internal invariant violation
```

Write errors to stderr as one concise line. JSON decisions, including `no_route`, go to stdout.

- [ ] **Step 5: Implement explain rendering**

For selected decisions, print:

```text
Decision <id>
Preset: plan-execute-verify
Plan: gpt-5.6-sol/high -> gpt-5.6-terra/medium -> gpt-5.6-terra/high
Excluded candidates: <count>
Missing evidence: <count>
```

For no-route decisions, print the stage, model, code, and evidence for each exclusion. Do not print raw request bodies or skill file contents.

- [ ] **Step 6: Generate and pin the first golden decision**

Run the CLI for `codex-implementation.json`, inspect the complete JSON, then copy it to `tests/fixtures/decisions/codex-implementation.json`. Add a test that compares CLI stdout bytes to the golden file.

Before accepting the golden, verify its plan and exclusions line by line against the spec. A snapshot is not automatically correct because it is stable.

- [ ] **Step 7: Run CLI tests and verify GREEN**

```bash
cargo test -p agent-router-cli
cargo run -p agent-router-cli -- route --request tests/fixtures/requests/codex-implementation.json --catalog assets/catalogs/codex-core-v1.json --json
```

Expected: tests pass and manual output contains the three expected stages.

- [ ] **Step 8: Commit the CLI**

```bash
git add Cargo.toml crates/agent-router-cli tests/fixtures/decisions
git commit -m "feat(cli): expose deterministic route and explain commands"
```

## Task 7: Fixture matrix, documentation, and quality gate

**Files:**

- Create: `tests/fixtures/requests/codex-exploration.json`
- Create: `tests/fixtures/requests/codex-architecture.json`
- Create: `tests/fixtures/requests/codex-migration.json`
- Create: `tests/fixtures/requests/claude-unknown-host.json`
- Create: `tests/fixtures/requests/missing-tools.json`
- Create: `tests/fixtures/requests/unsupported-effort.json`
- Create: `tests/fixtures/requests/high-risk-unknown-capability.json`
- Create: `tests/fixtures/requests/explicit-skill.json`
- Create: `tests/fixtures/requests/empty-skill-candidates.json`
- Create: `docs/reference/json-contracts.md`
- Modify: `README.md`

**Interfaces:**

- Consumes: completed core and CLI.
- Produces: at least twelve auditable route fixtures, documented contracts, and one full workspace verification command.

- [ ] **Step 1: Add a table-driven test that initially fails on missing fixtures**

Create a test matrix with expected outcome, stage models, and exclusion codes. Include the three existing requests and the nine files listed above. The test loads every path and fails with its filename if parsing or assertions fail.

- [ ] **Step 2: Run the matrix test and verify RED**

Run:

```bash
cargo test -p router-policy fixture_matrix -- --nocapture
```

Expected: failure because the nine additional fixture files do not exist.

- [ ] **Step 3: Add the nine fixtures one at a time**

After each fixture, rerun the table test and confirm the remaining failure count decreases. Required behaviors:

```text
codex-exploration                   Luna may execute; Sol plans only if preset demands plan
codex-architecture                  Sol plans and verifies
codex-migration                     high-risk unknown excludes candidate
claude-unknown-host                 no route with host evidence unknown
missing-tools                       exclusion host.capability.missing
unsupported-effort                  exclusion host.effort.unsupported
high-risk-unknown-capability        exclusion host.capability.unknown_high_risk
explicit-skill                      candidate appears once, no duplicate skill binding
empty-skill-candidates              valid route with zero skill bonus
```

- [ ] **Step 4: Document exact JSON contracts**

`docs/reference/json-contracts.md` must contain complete valid examples for catalog, route request, BM25 candidate document, selected decision, and no-route decision. Document exit codes and deterministic ordering. State that configuration proves intent, not actual model execution.

- [ ] **Step 5: Update README quickstart**

Add build, test, route, and explain commands. Keep the status explicit: dry-run routing only, no host execution and no global installation.

- [ ] **Step 6: Run the full quality gate**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
git diff --check
```

Expected: all commands exit 0, all tests pass, no warnings, and docs build.

- [ ] **Step 7: Verify the safety boundary**

Record hashes of `~/.config/ai-agents/current`, `~/.codex/AGENTS.md`, and `~/.claude/CLAUDE.md` before and after the CLI fixture suite using read-only commands. The hashes must match. If a target is absent, record `absent` both times rather than treating it as success.

- [ ] **Step 8: Commit the verified first slice**

```bash
git add README.md docs/reference tests/fixtures
git commit -m "test: complete deterministic routing fixture matrix"
```

## Plan self-review

### Spec coverage

- Stable domain contracts: Task 1.
- Validated catalog and exact IDs: Task 2.
- Hard constraints before scoring: Task 3.
- Existing BM25 adapter boundary: Task 4.
- Deterministic preset, scoring, alternatives, exclusions, and no-route: Task 5.
- Versioned route and explain CLI: Task 6.
- Twelve fixtures, docs, quality, and no-global-write proof: Task 7.

Deferred by the approved first-slice scope: live Codex and Claude probes, SQLite, child processes, DAGs, worktrees, hooks, plugins, Tantivy, embeddings, policy sidecars, feedback, bandits, TUI, and distribution installers.

### Type consistency

All later tasks consume the exact newtypes and contracts introduced in Task 1. Catalog validation returns typed `CatalogError`; policy returns `RouteError`; CLI maps both without changing their stable codes. Serialized scores are integers throughout.

### Placeholder scan

The plan contains no deferred implementation markers inside the first-slice scope. Explicitly deferred systems are listed as out of scope and require separate specs and plans.
