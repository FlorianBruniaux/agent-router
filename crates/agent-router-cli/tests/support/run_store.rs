// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Synthetic runtime stores shared by the observation CLI tests. No provider.
#![allow(dead_code)]
use assert_cmd::Command;
use router_runtime::{Runtime, RuntimeLimits};
use router_types::{OutputRetentionPolicy, RunState, RunSummary};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

pub const PRIVATE: &str = "TIMELINE_PRIVATE_PROMPT_PATH_PROVIDER_CANARY";
pub const MODEL: &str = "vendor/model-v4:2026_10.28";

pub struct Store {
    _temp: tempfile::TempDir,
    pub root: PathBuf,
}
impl Store {
    pub fn new(state: RunState, attempts: u32) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("runtime");
        drop(Runtime::open(&root, RuntimeLimits::default()).unwrap());
        let db = Connection::open(root.join("runtime.sqlite3")).unwrap();
        let run = RunSummary {
            run_id: PRIVATE.into(),
            definition_hash: "a".repeat(64),
            approval_digest: "b".repeat(64),
            receipt_id: PRIVATE.into(),
            plan_id: PRIVATE.into(),
            node_id: PRIVATE.into(),
            state,
            attempts,
            failure: None,
            worktree: Some(PRIVATE.into()),
            artifact_sha256: None,
            output_retention: OutputRetentionPolicy::Disabled,
            workflow: None,
            artifact_contract_sha256: None,
        };
        db.execute(
            "INSERT INTO runs VALUES(?1,?1,?1,?2)",
            params![PRIVATE, serde_json::to_string(&run).unwrap()],
        )
        .unwrap();
        for ordinal in 1..=attempts {
            db.execute(
                "INSERT INTO run_attempts VALUES(?1,?2,'observed')",
                params![PRIVATE, ordinal],
            )
            .unwrap();
        }
        Self { _temp: temp, root }
    }
    pub fn append(&self, state: RunState, kind: &str, detail: &str) -> String {
        let db = Connection::open(self.root.join("runtime.sqlite3")).unwrap();
        let (sequence, previous): (u64, String) = db
            .query_row(
                "SELECT sequence,event_hash FROM events ORDER BY sequence DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, String::new()));
        let mut hasher = Sha256::new();
        hasher.update(b"runtime-event-v1\0");
        hasher.update(
            serde_json::to_vec(&(sequence + 1, PRIVATE, kind, state, detail, &previous)).unwrap(),
        );
        let hash = hex::encode(hasher.finalize());
        db.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                sequence + 1,
                PRIVATE,
                kind,
                serde_json::to_string(&state).unwrap(),
                detail,
                previous,
                hash
            ],
        )
        .unwrap();
        hash
    }
    pub fn approve(&self) {
        self.append(RunState::Planned, "planned", "");
        self.append(RunState::AwaitingApproval, "transition", "reviewed");
        self.append(RunState::Queued, "transition", "approved");
        self.append(RunState::Preparing, "transition", "preparing");
    }
    pub fn attempt(
        &self,
        end: RunState,
        cancel_between: bool,
        tokens: Option<u64>,
        cost: Option<u64>,
    ) {
        self.attempt_transport(end, cancel_between, tokens, cost, ("exited", Some(0)));
    }
    pub fn attempt_transport(
        &self,
        end: RunState,
        cancel_between: bool,
        tokens: Option<u64>,
        cost: Option<u64>,
        transport: (&str, Option<i32>),
    ) {
        let start = self.append(RunState::Preparing, "attempt.started", "");
        let ordinal: u32 = Connection::open(self.root.join("runtime.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='attempt.started'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        self.append(RunState::Running, "transition", "prepared");
        if cancel_between {
            self.append(RunState::Running, "control.cancel_requested", "");
        }
        let finish = self.append(RunState::Running, "attempt.finished", "observed");
        let transition = self.append(end, "transition", "transport_observed");
        let evidence = json!({
            "schema_version":1, "identity":{"run_id":PRIVATE,"attempt":ordinal},
            "definition_hash":"a".repeat(64), "authorization_id":PRIVATE,
            "decision_origin":"deterministic", "sandbox":"read_only",
            "context_hash":"c".repeat(64), "pool_hash":"d".repeat(64),
            "source_state":PRIVATE, "task_execution_sha256":"e".repeat(64),
            "candidate_id":PRIVATE, "model":MODEL, "effort":"high", "workflow":null,
            "state":end, "failure":null,
            "transport":{"termination":transport.0,"exit_code":transport.1,"elapsed_ms":987654321},
            "cleanup":"unknown", "observed_cost_microusd":cost,
            "observed_tokens":tokens, "started_event_hash":start,
            "finished_event_hash":finish, "transition_event_hash":transition
        });
        self.append(end, "attempt.evidence.v1", &evidence.to_string());
    }
    pub fn observe(&self, json: bool) -> Command {
        self.observe_version(json, "4")
    }
    pub fn observe_version(&self, json: bool, version: &str) -> Command {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("agent-router"));
        cmd.args(["observe", "--store"]).arg(&self.root).args([
            "--run-id",
            PRIVATE,
            "--schema-version",
            version,
        ]);
        if json {
            cmd.arg("--json");
        }
        cmd
    }
    /// Returns (json, human) after checking redaction of both outputs.
    pub fn both(&self) -> (Value, String) {
        self.both_version("4")
    }
    pub fn both_version(&self, version: &str) -> (Value, String) {
        let json = self
            .observe_version(true, version)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let human = self
            .observe_version(false, version)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let (json_text, human) = (
            String::from_utf8(json).unwrap(),
            String::from_utf8(human).unwrap(),
        );
        for text in [&json_text, &human] {
            for needle in [
                PRIVATE,
                self.root.to_str().unwrap(),
                "987654321",
                "reviewed",
                "transport_observed",
            ] {
                assert!(!text.contains(needle), "leaked {needle}:\n{text}");
            }
        }
        (serde_json::from_str(&json_text).unwrap(), human)
    }
}
