use crate::{
    Result, RuntimeError, digest,
    leases::{LeaseRecord, LeaseStorage},
};
use nix::fcntl::{Flock, FlockArg};
use router_types::{
    ExecutionRequest, ModelId, OutputArtifacts, OutputArtifactsState, OutputRetentionPolicy,
    RunFailure, RunState, RunSummary, RuntimeEvent, WorkflowExecutionContract,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};

mod escalation;
mod preparation;
pub(crate) use escalation::EscalationHistory;
pub(crate) use preparation::Preparation;

pub(crate) struct Store {
    db: Mutex<Connection>,
    pub root: PathBuf,
    leases: LeaseStorage,
    _lock: Flock<File>,
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_absolute() || root.file_name().is_none() {
            return Err(RuntimeError::Invalid("storage path"));
        }
        if !root.exists() {
            fs::DirBuilder::new().mode(0o700).create(root)?;
        }
        let meta = fs::symlink_metadata(root)?;
        if !meta.is_dir() || meta.file_type().is_symlink() || meta.permissions().mode() & 0o077 != 0
        {
            return Err(RuntimeError::Invalid("private storage directory"));
        }
        let root = fs::canonicalize(root)?;
        let lock_file = private_file(&root.join("runtime.lock"))?;
        let lock = Flock::lock(lock_file, FlockArg::LockExclusiveNonblock)
            .map_err(|_| RuntimeError::Locked)?;
        let path = root.join("runtime.sqlite3");
        let file = private_file(&path)?;
        drop(file);
        let mut db = Connection::open(&path)?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 8 {
            return Err(RuntimeError::Integrity("future schema"));
        }
        if version == 0 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE runs(run_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, node_id TEXT NOT NULL, summary TEXT NOT NULL, UNIQUE(plan_id,node_id));
                CREATE TABLE run_attempts(run_id TEXT NOT NULL REFERENCES runs(run_id), attempt INTEGER NOT NULL, status TEXT NOT NULL, PRIMARY KEY(run_id,attempt));
                CREATE TABLE events(sequence INTEGER PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), kind TEXT NOT NULL, state TEXT NOT NULL, detail TEXT NOT NULL, previous_hash TEXT NOT NULL, event_hash TEXT NOT NULL);
                CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'append-only'); END;
                CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'append-only'); END;
                CREATE TABLE writer_locks(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), workspace TEXT NOT NULL, scope TEXT NOT NULL);
                CREATE TABLE artifacts(run_id TEXT NOT NULL REFERENCES runs(run_id), sha256 TEXT NOT NULL, kind TEXT NOT NULL, PRIMARY KEY(run_id,kind));
                CREATE TABLE verifications(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), record TEXT NOT NULL);
                PRAGMA user_version=1;")?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE control_requests(run_id TEXT PRIMARY KEY REFERENCES runs(run_id), cancel_requested INTEGER NOT NULL CHECK(cancel_requested=1)); PRAGMA user_version=2;")?;
            tx.commit()?;
        }
        if version < 3 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE output_artifacts(run_id TEXT NOT NULL REFERENCES runs(run_id), attempt INTEGER NOT NULL, expires_at INTEGER NOT NULL, state TEXT NOT NULL, record TEXT NOT NULL, PRIMARY KEY(run_id,attempt)); CREATE INDEX output_expiration ON output_artifacts(state,expires_at); CREATE TABLE output_files(run_id TEXT NOT NULL, attempt INTEGER NOT NULL, stream TEXT NOT NULL CHECK(stream IN ('stdout','stderr')), identity TEXT NOT NULL, PRIMARY KEY(run_id,attempt,stream), FOREIGN KEY(run_id,attempt) REFERENCES output_artifacts(run_id,attempt)); PRAGMA user_version=3;")?;
            tx.commit()?;
        }
        if version < 4 {
            // SQLite table rebuild removes plan-global uniqueness. Historical
            // runs remain unbound: a shared plan hash is never an instance ID.
            db.pragma_update(None, "foreign_keys", "OFF")?;
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE runs_v4(run_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, node_id TEXT NOT NULL, summary TEXT NOT NULL);
                INSERT INTO runs_v4 SELECT run_id,plan_id,node_id,summary FROM runs;
                DROP TABLE runs;
                ALTER TABLE runs_v4 RENAME TO runs;
                CREATE TABLE workflow_instances(instance_id TEXT PRIMARY KEY, contract_hash TEXT NOT NULL, record TEXT NOT NULL);
                CREATE TABLE workflow_node_runs(instance_id TEXT NOT NULL REFERENCES workflow_instances(instance_id), node_id TEXT NOT NULL, generation INTEGER NOT NULL, run_id TEXT NOT NULL UNIQUE REFERENCES runs(run_id), model TEXT NOT NULL, agent TEXT, PRIMARY KEY(instance_id,node_id,generation));
                PRAGMA user_version=4;")?;
            let violations: bool = tx.prepare("PRAGMA foreign_key_check")?.exists([])?;
            if violations {
                return Err(RuntimeError::Integrity("migration foreign keys"));
            }
            tx.commit()?;
            db.pragma_update(None, "foreign_keys", "ON")?;
        }
        if version < 5 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE declared_artifacts(artifact_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id), attempt INTEGER NOT NULL, label TEXT NOT NULL, record TEXT NOT NULL, identity TEXT, UNIQUE(run_id,attempt,label)); PRAGMA user_version=5;")?;
            tx.commit()?;
        }
        if version < 6 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE attempt_leases(run_id TEXT NOT NULL REFERENCES runs(run_id), attempt INTEGER NOT NULL, record TEXT NOT NULL, resolved INTEGER NOT NULL DEFAULT 0 CHECK(resolved IN (0,1)), evidence TEXT, PRIMARY KEY(run_id,attempt)); CREATE TABLE cleanup_blocks(run_id TEXT PRIMARY KEY REFERENCES runs(run_id)); PRAGMA user_version=6;")?;
            tx.commit()?;
        }
        if version < 7 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE workflow_plan_authority(instance_id TEXT PRIMARY KEY REFERENCES workflow_instances(instance_id), pin TEXT NOT NULL); INSERT INTO workflow_plan_authority SELECT instance_id,'legacy' FROM workflow_instances; PRAGMA user_version=7;")?;
            tx.commit()?;
        }
        if version < 8 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE preparation_cycles(run_id TEXT NOT NULL REFERENCES runs(run_id), sequence INTEGER NOT NULL, record TEXT NOT NULL, PRIMARY KEY(run_id,sequence)); PRAGMA user_version=8;")?;
            tx.commit()?;
        }
        for suffix in ["", "-wal", "-shm"] {
            let path = root.join(format!("runtime.sqlite3{suffix}"));
            if path.exists() {
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
        }
        let store = Self {
            db: Mutex::new(db),
            leases: LeaseStorage::open(&root)?,
            root,
            _lock: lock,
        };
        store.verify_events()?;
        store.recover()?;
        Ok(store)
    }
    pub(crate) fn verify_plan_pin(
        &self,
        workflow: Option<&router_types::WorkflowRunBinding>,
        generated: Option<&router_types::GeneratedDecisionBinding>,
    ) -> Result<()> {
        verify_plan_pin(&*self.lock()?, workflow, generated)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| RuntimeError::Integrity("storage lock poisoned"))
    }
    pub(crate) fn diagnose_snapshot(
        &self,
    ) -> router_doctor::Result<router_doctor::DatabaseSnapshotReport> {
        let connection = self.lock().map_err(|_| router_doctor::DoctorError::Read)?;
        router_doctor::diagnose_database_snapshot(&connection)
    }
    #[cfg(test)]
    pub fn register(&self, summary: &RunSummary) -> Result<RunSummary> {
        self.register_inner(summary, None, None, 5, None)
    }
    pub fn register_workflow(
        &self,
        summary: &RunSummary,
        contract: Option<&WorkflowExecutionContract>,
        request: &ExecutionRequest,
        max_attempts: u32,
        generated: Option<&router_types::GeneratedDecisionBinding>,
    ) -> Result<RunSummary> {
        self.register_inner(summary, contract, Some(request), max_attempts, generated)
    }
    fn register_inner(
        &self,
        summary: &RunSummary,
        contract: Option<&WorkflowExecutionContract>,
        request: Option<&ExecutionRequest>,
        max_attempts: u32,
        generated: Option<&router_types::GeneratedDecisionBinding>,
    ) -> Result<RunSummary> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(existing) = load_opt(&tx, &summary.run_id)? {
            if existing.definition_hash != summary.definition_hash {
                return Err(RuntimeError::Integrity("definition changed"));
            }
            verify_plan_pin(&tx, summary.workflow.as_ref(), generated)?;
            return Ok(existing);
        }
        let mut initial_generated_pin = None;
        if let Some(binding) = &summary.workflow {
            let contract = contract.ok_or(RuntimeError::Integrity("missing workflow contract"))?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT contract_hash FROM workflow_instances WHERE instance_id=?1",
                    [&binding.instance_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|hash| hash != &binding.contract_sha256)
            {
                return Err(RuntimeError::Integrity("execution instance changed"));
            }
            if existing.is_none() {
                tx.execute(
                    "INSERT INTO workflow_instances VALUES(?1,?2,?3)",
                    params![
                        binding.instance_id,
                        binding.contract_sha256,
                        serde_json::to_string(contract)?
                    ],
                )?;
                let pin = plan_pin(generated)?;
                tx.execute(
                    "INSERT INTO workflow_plan_authority VALUES(?1,?2)",
                    params![binding.instance_id, pin],
                )?;
                if generated.is_some() {
                    initial_generated_pin = Some(PlanAuthorityAnchor {
                        schema_version: 1,
                        instance_id: binding.instance_id.clone(),
                        contract_sha256: binding.contract_sha256.clone(),
                        pin,
                    });
                }
            }
            // An existing instance must retain its original pin. Missing data
            // is corruption, never permission to replace the global proposal.
            let previous = workflow_node(&tx, &binding.instance_id, &summary.node_id)?;
            match previous {
                None if binding.generation == 0 => {}
                Some((run, _, _))
                    if run
                        .workflow
                        .as_ref()
                        .is_some_and(|old| old.generation + 1 == binding.generation)
                        && retryable(&run)
                        && workflow_attempts(&tx, &run)? < max_attempts =>
                {
                    // A registered consumer freezes producer generation. A future
                    // coordinator must schedule fallback before reviewing consumers.
                    for (node, execution) in &contract.nodes {
                        if execution.dependencies.contains_key(&summary.node_id)
                            && workflow_node(&tx, &binding.instance_id, node)?.is_some()
                        {
                            return Err(RuntimeError::Invalid(
                                "dependency generation already bound",
                            ));
                        }
                    }
                }
                _ => {
                    return Err(RuntimeError::Invalid(
                        "node generation requires bounded retryable predecessor",
                    ));
                }
            }
        } else if contract.is_some() || generated.is_some() {
            return Err(RuntimeError::Integrity("missing workflow binding"));
        }
        tx.execute(
            "INSERT INTO runs(run_id,plan_id,node_id,summary) VALUES(?1,?2,?3,?4)",
            params![
                summary.run_id,
                summary.plan_id,
                summary.node_id,
                serde_json::to_string(summary)?
            ],
        )?;
        if let Some(binding) = &summary.workflow {
            let request = request.ok_or(RuntimeError::Integrity("missing node identity"))?;
            tx.execute(
                "INSERT INTO workflow_node_runs VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    binding.instance_id,
                    summary.node_id,
                    binding.generation,
                    summary.run_id,
                    serde_json::to_string(&request.model)?,
                    request.agent
                ],
            )?;
        }
        append(&tx, summary, "planned", "")?;
        if let Some(anchor) = initial_generated_pin {
            append(
                &tx,
                summary,
                "workflow.plan.authority.v1",
                &serde_json::to_string(&anchor)?,
            )?;
        }
        verify_plan_pin(&tx, summary.workflow.as_ref(), generated)?;
        tx.commit()?;
        Ok(summary.clone())
    }
    pub fn get(&self, id: &str) -> Result<RunSummary> {
        load_opt(&*self.lock()?, id)?.ok_or(RuntimeError::NotFound)
    }
    pub fn reserve_declared_artifact(
        &self,
        metadata: &router_types::DeclaredArtifact,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, &metadata.run_id)?.ok_or(RuntimeError::NotFound)?;
        if run.state != RunState::SucceededTransport
            || run.workflow.as_ref() != Some(&metadata.workflow)
            || run.node_id != metadata.node_id
            || run.attempts != metadata.attempt
            || run.artifact_contract_sha256.is_none()
            || metadata.state != router_types::DeclaredArtifactState::Pending
        {
            return Err(RuntimeError::Integrity("declared output authority"));
        }
        tx.execute(
            "INSERT INTO declared_artifacts VALUES(?1,?2,?3,?4,?5,NULL)",
            params![
                metadata.artifact_id,
                metadata.run_id,
                metadata.attempt,
                metadata.label,
                serde_json::to_string(metadata)?
            ],
        )?;
        append(&tx, &run, "artifact.declared", &metadata.artifact_id)?;
        tx.commit()?;
        Ok(())
    }
    pub fn declared_artifact_identity(
        &self,
        id: &str,
        identity: &crate::outputs::FileIdentity,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if tx.execute(
            "UPDATE declared_artifacts SET identity=?2 WHERE artifact_id=?1 AND identity IS NULL",
            params![id, serde_json::to_string(identity)?],
        )? != 1
        {
            return Err(RuntimeError::Integrity("result identity registration"));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn declared_artifact(
        &self,
        id: &str,
    ) -> Result<(
        router_types::DeclaredArtifact,
        Option<crate::outputs::FileIdentity>,
    )> {
        let value: Option<(String, Option<String>)> = self
            .lock()?
            .query_row(
                "SELECT record,identity FROM declared_artifacts WHERE artifact_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (record, identity) = value.ok_or(RuntimeError::NotFound)?;
        Ok((
            serde_json::from_str(&record)?,
            identity.map(|v| serde_json::from_str(&v)).transpose()?,
        ))
    }
    pub fn declared_artifacts(&self, run: &str) -> Result<Vec<router_types::DeclaredArtifact>> {
        let db = self.lock()?;
        load_opt(&db, run)?.ok_or(RuntimeError::NotFound)?;
        let mut query = db.prepare(
            "SELECT record FROM declared_artifacts WHERE run_id=?1 ORDER BY attempt,label",
        )?;
        let records = query.query_map([run], |r| r.get::<_, String>(0))?;
        records.map(|v| Ok(serde_json::from_str(&v?)?)).collect()
    }
    pub fn declared_artifact_state(
        &self,
        id: &str,
        next: router_types::DeclaredArtifactState,
    ) -> Result<router_types::DeclaredArtifact> {
        use router_types::DeclaredArtifactState::*;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let record: String = tx.query_row(
            "SELECT record FROM declared_artifacts WHERE artifact_id=?1",
            [id],
            |r| r.get(0),
        )?;
        let mut metadata: router_types::DeclaredArtifact = serde_json::from_str(&record)?;
        if metadata.state != next
            && !matches!(
                (metadata.state, next),
                (Pending, Retained | Purging) | (Retained, Purging) | (Purging, Purged)
            )
        {
            return Err(RuntimeError::Transition);
        }
        metadata.state = next;
        tx.execute(
            "UPDATE declared_artifacts SET record=?2 WHERE artifact_id=?1",
            params![id, serde_json::to_string(&metadata)?],
        )?;
        let run = load_opt(&tx, &metadata.run_id)?.ok_or(RuntimeError::NotFound)?;
        append(
            &tx,
            &run,
            "artifact.state",
            &digest("runtime-declared-artifact-v1", &metadata)?,
        )?;
        tx.commit()?;
        Ok(metadata)
    }
    pub fn reserve_output_artifacts(&self, metadata: &OutputArtifacts) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, &metadata.run_id)?.ok_or(RuntimeError::NotFound)?;
        if metadata.state != OutputArtifactsState::Pending
            || metadata.attempt != run.attempts
            || run.output_retention == OutputRetentionPolicy::Disabled
        {
            return Err(RuntimeError::Integrity("output retention authority"));
        }
        tx.execute(
            "INSERT INTO output_artifacts VALUES(?1,?2,?3,?4,?5)",
            params![
                metadata.run_id,
                metadata.attempt,
                i64::try_from(metadata.expires_at_unix_seconds).map_err(|_| RuntimeError::Limit)?,
                "pending",
                serde_json::to_string(metadata)?
            ],
        )?;
        append(
            &tx,
            &run,
            "outputs.pending",
            &digest("runtime-output-artifacts-v1", metadata)?,
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn output_artifacts(&self, id: &str) -> Result<Vec<OutputArtifacts>> {
        let db = self.lock()?;
        load_opt(&db, id)?.ok_or(RuntimeError::NotFound)?;
        let mut query = db.prepare(
            "SELECT record FROM output_artifacts WHERE run_id=?1 ORDER BY attempt LIMIT 6",
        )?;
        let records = query
            .query_map([id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if records.len() > 5 {
            return Err(RuntimeError::Limit);
        }
        records
            .into_iter()
            .map(|record| serde_json::from_str(&record).map_err(Into::into))
            .collect()
    }
    pub fn register_output_file(
        &self,
        id: &str,
        attempt: u32,
        stream: &str,
        identity: &crate::outputs::FileIdentity,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        tx.execute(
            "INSERT INTO output_files VALUES(?1,?2,?3,?4)",
            params![id, attempt, stream, serde_json::to_string(identity)?],
        )?;
        append(
            &tx,
            &run,
            "outputs.file_owned",
            &digest("runtime-output-file-v1", &(attempt, stream, identity))?,
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn output_file_identities(
        &self,
        id: &str,
        attempt: u32,
    ) -> Result<std::collections::BTreeMap<String, crate::outputs::FileIdentity>> {
        let db = self.lock()?;
        let mut query=db.prepare("SELECT stream,identity FROM output_files WHERE run_id=?1 AND attempt=?2 ORDER BY stream LIMIT 3")?;
        let rows = query
            .query_map(params![id, attempt], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if rows.len() > 2 {
            return Err(RuntimeError::Integrity("output files"));
        }
        rows.into_iter()
            .map(|(stream, value)| Ok((stream, serde_json::from_str(&value)?)))
            .collect()
    }
    pub fn expired_output_artifacts(&self, now: u64, limit: usize) -> Result<Vec<OutputArtifacts>> {
        if limit == 0 || limit > 100 {
            return Err(RuntimeError::Limit);
        }
        let db = self.lock()?;
        let mut query=db.prepare("SELECT record FROM output_artifacts WHERE state!='purged' AND expires_at<=?1 ORDER BY expires_at,run_id,attempt LIMIT ?2")?;
        let records = query
            .query_map(
                params![i64::try_from(now).map_err(|_| RuntimeError::Limit)?, limit],
                |row| row.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|record| serde_json::from_str(&record).map_err(Into::into))
            .collect()
    }
    pub fn output_artifacts_state(
        &self,
        id: &str,
        attempt: u32,
        next: OutputArtifactsState,
    ) -> Result<OutputArtifacts> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        let record: Option<String> = tx
            .query_row(
                "SELECT record FROM output_artifacts WHERE run_id=?1 AND attempt=?2",
                params![id, attempt],
                |row| row.get(0),
            )
            .optional()?;
        let mut metadata: OutputArtifacts =
            serde_json::from_str(&record.ok_or(RuntimeError::NotFound)?)?;
        use OutputArtifactsState::*;
        if metadata.state == next {
            return Ok(metadata);
        }
        if !matches!(
            (metadata.state, next),
            (Pending, Retained | Purging) | (Retained, Purging) | (Purging, Purged)
        ) {
            return Err(RuntimeError::Transition);
        }
        metadata.state = next;
        let state = match next {
            Pending => "pending",
            Retained => "retained",
            Purging => "purging",
            Purged => "purged",
        };
        tx.execute(
            "UPDATE output_artifacts SET state=?3, record=?4 WHERE run_id=?1 AND attempt=?2",
            params![id, attempt, state, serde_json::to_string(&metadata)?],
        )?;
        append(
            &tx,
            &run,
            &format!("outputs.{state}"),
            &digest("runtime-output-artifacts-v1", &metadata)?,
        )?;
        tx.commit()?;
        Ok(metadata)
    }
    pub fn workflow_node(
        &self,
        instance: &str,
        node: &str,
    ) -> Result<Option<(RunSummary, ModelId, Option<String>)>> {
        workflow_node(&*self.lock()?, instance, node)
    }
    pub fn cancel_requested(&self, id: &str) -> Result<bool> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT 1 FROM control_requests WHERE run_id=?1",
                [id],
                |row| row.get::<_, u32>(0),
            )
            .optional()?
            .is_some())
    }
    pub fn transition(
        &self,
        id: &str,
        next: RunState,
        detail: &str,
        failure: Option<RunFailure>,
    ) -> Result<RunSummary> {
        self.transition_inner(id, next, detail, failure, false)
    }
    pub fn transition_queued(
        &self,
        id: &str,
        next: RunState,
        detail: &str,
        failure: Option<RunFailure>,
    ) -> Result<RunSummary> {
        self.transition_inner(id, next, detail, failure, true)
    }
    fn transition_inner(
        &self,
        id: &str,
        next: RunState,
        detail: &str,
        failure: Option<RunFailure>,
        require_queued: bool,
    ) -> Result<RunSummary> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        if require_queued && run.state == RunState::Cancelled {
            return Err(RuntimeError::Cancelled);
        }
        if !run.state.permits(next) || (require_queued && run.state != RunState::Queued) {
            append(
                &tx,
                &run,
                "integrity.invalid_transition",
                &format!("requested:{next:?}"),
            )?;
            tx.commit()?;
            return Err(RuntimeError::Transition);
        }
        run.state = next;
        run.failure = failure;
        save(&tx, &run)?;
        append(&tx, &run, "transition", detail)?;
        if next.terminal()
            && (!preparation::terminal_safe(&tx, &run)?
                || tx
                    .prepare("SELECT 1 FROM attempt_leases WHERE run_id=?1 AND resolved=0")?
                    .exists([id])?)
        {
            tx.execute("INSERT OR IGNORE INTO cleanup_blocks VALUES(?1)", [id])?;
        }
        if next.terminal() && !cleanup_blocked(&tx, Some(id))? {
            tx.execute("DELETE FROM writer_locks WHERE run_id=?1", [id])?;
        }
        tx.commit()?;
        Ok(run)
    }
    pub fn event(&self, id: &str, kind: &str, detail: &str) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        append(&tx, &run, kind, detail)?;
        tx.commit()?;
        Ok(())
    }
    pub fn require_cleanup_complete(&self) -> Result<()> {
        if cleanup_blocked(&*self.lock()?, None)? {
            return Err(RuntimeError::CleanupUnknown);
        }
        Ok(())
    }
    pub fn cleanup_evidence(&self, id: &str) -> Result<router_types::ProcessCleanupEvidence> {
        cleanup_evidence(&*self.lock()?, id)
    }
    pub fn attempt(&self, id: &str, max: u32, supervised: bool) -> Result<Option<PathBuf>> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        if run.state != RunState::Preparing || workflow_attempts(&tx, &run)? >= max {
            return Err(RuntimeError::Limit);
        }
        if cleanup_blocked(&tx, None)? {
            return Err(RuntimeError::CleanupUnknown);
        }
        preparation::require_complete(&tx, &run)?;
        run.attempts += 1;
        let lease = if supervised {
            let (record, path) = self.leases.create(id, run.attempts)?;
            tx.execute(
                "INSERT INTO attempt_leases(run_id,attempt,record) VALUES(?1,?2,?3)",
                params![id, run.attempts, serde_json::to_string(&record)?],
            )?;
            Some(path)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO run_attempts VALUES(?1,?2,'started')",
            params![id, run.attempts],
        )?;
        save(&tx, &run)?;
        append(&tx, &run, "attempt.started", "")?;
        tx.commit()?;
        Ok(lease)
    }
    pub fn confirm_attempt_cleanup(&self, id: &str) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        let record: Option<String> = tx
            .query_row(
                "SELECT record FROM attempt_leases WHERE run_id=?1 AND attempt=?2",
                params![id, run.attempts],
                |r| r.get(0),
            )
            .optional()?;
        let Some(record) = record else {
            return Ok(());
        };
        let proof = serde_json::from_str::<LeaseRecord>(&record)
            .map_err(|_| RuntimeError::CleanupUnknown)
            .and_then(|record| self.leases.cleanup_proof(id, run.attempts, &record));
        match proof {
            Ok(proof) => {
                let evidence = serde_json::to_string(&proof.evidence)?;
                tx.execute("UPDATE attempt_leases SET resolved=1,evidence=?3 WHERE run_id=?1 AND attempt=?2",params![id,run.attempts,evidence])?;
                append(&tx, &run, "process.cleanup_confirmed", &evidence)?;
                tx.commit()?;
                Ok(())
            }
            Err(_) => {
                tx.execute("INSERT OR IGNORE INTO cleanup_blocks VALUES(?1)", [id])?;
                append(&tx, &run, "process.cleanup_unknown", "authority_retained")?;
                tx.commit()?;
                Err(RuntimeError::CleanupUnknown)
            }
        }
    }
    pub fn finish_attempt(&self, id: &str, status: &str) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        tx.execute(
            "UPDATE run_attempts SET status=?3 WHERE run_id=?1 AND attempt=?2",
            params![id, run.attempts, status],
        )?;
        append(&tx, &run, "attempt.finished", status)?;
        tx.commit()?;
        Ok(())
    }
    pub fn retry(&self, id: &str, max: u32) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if cleanup_blocked(&tx, None)? {
            return Err(RuntimeError::CleanupUnknown);
        }
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        let obsolete = if let Some(binding) = &run.workflow {
            workflow_node(&tx, &binding.instance_id, &run.node_id)?
                .is_none_or(|(current, _, _)| current.run_id != run.run_id)
        } else {
            false
        };
        let claimed = tx
            .prepare("SELECT 1 FROM events WHERE run_id=?1 AND kind='escalation.claim.v1'")?
            .exists([id])?;
        if !retryable(&run) || workflow_attempts(&tx, &run)? >= max || obsolete || claimed {
            return Err(RuntimeError::Invalid(
                "retry requires a bounded read-only attempt",
            ));
        }
        run.state = RunState::AwaitingApproval;
        run.failure = None;
        run.approval_digest = digest(
            "runtime-retry-approval-v1",
            &(&run.run_id, &run.definition_hash, run.attempts),
        )?;
        tx.execute("DELETE FROM control_requests WHERE run_id=?1", [id])?;
        save(&tx, &run)?;
        append(&tx, &run, "retry.awaiting_fresh_approval", "")?;
        tx.commit()?;
        Ok(())
    }
    pub fn writer_lock(&self, id: &str, workspace: &str, scope: &[String]) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut query =
            tx.prepare("SELECT scope FROM writer_locks WHERE workspace=?1 AND run_id<>?2")?;
        let scopes = query
            .query_map(params![workspace, id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for other in scopes {
            let other: Vec<String> = serde_json::from_str(&other)?;
            if scope.iter().any(|a| other.iter().any(|b| overlaps(a, b))) {
                return Err(RuntimeError::Scope);
            }
        }
        drop(query);
        tx.execute(
            "INSERT OR REPLACE INTO writer_locks VALUES(?1,?2,?3)",
            params![id, workspace, serde_json::to_string(scope)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn worktree(&self, id: &str, path: Option<&str>) -> Result<()> {
        self.mutate(id, "worktree", |run| {
            run.worktree = path.map(str::to_owned);
        })
    }
    pub fn artifact(&self, id: &str, hash: &str) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        run.artifact_sha256 = Some(hash.into());
        tx.execute(
            "INSERT OR REPLACE INTO artifacts VALUES(?1,?2,'patch')",
            params![id, hash],
        )?;
        save(&tx, &run)?;
        append(&tx, &run, "artifact", hash)?;
        tx.commit()?;
        Ok(())
    }
    pub fn verification_record(
        &self,
        id: &str,
    ) -> Result<Option<router_types::VerificationRecord>> {
        let db = self.lock()?;
        load_opt(&db, id)?.ok_or(RuntimeError::NotFound)?;
        let value: Option<String> = db
            .query_row(
                "SELECT record FROM verifications WHERE run_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?;
        value.map(|v| Ok(serde_json::from_str(&v)?)).transpose()
    }
    pub fn verification(&self, id: &str, record: &router_types::VerificationRecord) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        tx.execute(
            "INSERT OR REPLACE INTO verifications VALUES(?1,?2)",
            params![id, serde_json::to_string(record)?],
        )?;
        append(&tx, &run, "verification.recorded", &record.evidence_sha256)?;
        tx.commit()?;
        Ok(())
    }
    fn mutate(&self, id: &str, kind: &str, f: impl FnOnce(&mut RunSummary)) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        f(&mut run);
        save(&tx, &run)?;
        append(&tx, &run, kind, "")?;
        tx.commit()?;
        Ok(())
    }
    pub fn events(&self) -> Result<Vec<RuntimeEvent>> {
        read_events(&*self.lock()?)
    }
    pub fn verify_events(&self) -> Result<()> {
        verify_events(&*self.lock()?)
    }
    fn recover(&self) -> Result<()> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let values = {
            let mut q = tx.prepare("SELECT summary FROM runs")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut cleanup_locks = Vec::new();
        for value in values {
            let mut run: RunSummary = serde_json::from_str(&value)?;
            if matches!(run.state, RunState::Preparing | RunState::Running)
                || cleanup_blocked(&tx, Some(&run.run_id))?
            {
                let lease: Option<(String, bool)> = tx
                    .query_row(
                        "SELECT record,resolved FROM attempt_leases WHERE run_id=?1 AND attempt=?2",
                        params![run.run_id, run.attempts],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let preparation_safe = preparation::recover(self, &tx, &run, &mut cleanup_locks)?;
                let model_safe = if let Some((record, resolved)) = lease {
                    if resolved {
                        true
                    } else {
                        match serde_json::from_str::<LeaseRecord>(&record)
                            .ok()
                            .and_then(|record| {
                                self.leases
                                    .cleanup_proof(&run.run_id, run.attempts, &record)
                                    .ok()
                            }) {
                            Some(lock) => {
                                tx.execute("UPDATE attempt_leases SET resolved=1,evidence=?3 WHERE run_id=?1 AND attempt=?2",params![run.run_id,run.attempts,serde_json::to_string(&lock.evidence)?])?;
                                cleanup_locks.push(lock);
                                true
                            }
                            None => false,
                        }
                    }
                } else {
                    // No new model was launched in this preparation. The
                    // cycle gate separately proves every earlier child.
                    preparation::no_model_started(&tx, &run)?
                };
                let safe = preparation_safe && model_safe;
                let interrupted = matches!(run.state, RunState::Preparing | RunState::Running);
                if interrupted {
                    run.state = RunState::Failed;
                    run.failure = Some(RunFailure::Interrupted);
                    save(&tx, &run)?;
                }
                if interrupted || safe {
                    append(
                        &tx,
                        &run,
                        "recovery.interrupted",
                        if safe {
                            "cleanup_confirmed_no_replay"
                        } else {
                            "cleanup_unknown_authority_retained"
                        },
                    )?;
                }
                if safe {
                    tx.execute("DELETE FROM writer_locks WHERE run_id=?1", [&run.run_id])?;
                    tx.execute("DELETE FROM cleanup_blocks WHERE run_id=?1", [&run.run_id])?;
                } else {
                    tx.execute(
                        "INSERT OR IGNORE INTO cleanup_blocks VALUES(?1)",
                        [&run.run_id],
                    )?;
                }
            } else if run.state == RunState::SucceededTransport {
                run.state = RunState::Verifying;
                save(&tx, &run)?;
                append(&tx, &run, "recovery.verification_required", "")?;
            }
        }
        tx.commit()?;
        drop(cleanup_locks);
        Ok(())
    }
}
fn cleanup_blocked(db: &Connection, id: Option<&str>) -> Result<bool> {
    Ok(match id {
        Some(id) => db
            .prepare("SELECT 1 FROM cleanup_blocks WHERE run_id=?1")?
            .exists([id])?,
        None => db
            .prepare("SELECT 1 FROM cleanup_blocks LIMIT 1")?
            .exists([])?,
    })
}
fn load_opt(db: &Connection, id: &str) -> Result<Option<RunSummary>> {
    let text: Option<String> = db
        .query_row("SELECT summary FROM runs WHERE run_id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(text.map(|value| serde_json::from_str(&value)).transpose()?)
}
fn save(tx: &Transaction<'_>, run: &RunSummary) -> Result<()> {
    tx.execute(
        "UPDATE runs SET summary=?2 WHERE run_id=?1",
        params![run.run_id, serde_json::to_string(run)?],
    )?;
    Ok(())
}
fn append(tx: &Transaction<'_>, run: &RunSummary, kind: &str, detail: &str) -> Result<()> {
    let max_detail = if matches!(
        kind,
        "attempt.evidence.v1" | "attempt.bound.v1" | "escalation.claim.v1" | "preparation.cycle.v1"
    ) {
        16_384
    } else {
        2048
    };
    if detail.len() > max_detail || kind.len() > 128 {
        return Err(RuntimeError::Limit);
    }
    let (sequence, previous_hash): (u64, String) = tx
        .query_row(
            "SELECT sequence,event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((0, String::new()));
    let mut event = RuntimeEvent {
        sequence: sequence + 1,
        run_id: run.run_id.clone(),
        kind: kind.into(),
        state: run.state,
        detail: detail.into(),
        previous_hash,
        event_hash: String::new(),
    };
    event.event_hash = event_digest(&event)?;
    tx.execute(
        "INSERT INTO events VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            event.sequence,
            event.run_id,
            event.kind,
            serde_json::to_string(&event.state)?,
            event.detail,
            event.previous_hash,
            event.event_hash
        ],
    )?;
    Ok(())
}
fn event_digest(event: &RuntimeEvent) -> Result<String> {
    digest(
        "runtime-event-v1",
        &(
            event.sequence,
            &event.run_id,
            &event.kind,
            event.state,
            &event.detail,
            &event.previous_hash,
        ),
    )
}
pub(crate) fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b).is_some_and(|s| s.starts_with('/'))
        || b.strip_prefix(a).is_some_and(|s| s.starts_with('/'))
}
pub(crate) fn private_file(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.permissions().mode() & 0o077 != 0 {
        return Err(RuntimeError::Invalid("private regular file"));
    }
    Ok(file)
}

/// Observation/control deliberately does not acquire the supervision flock.
/// Control is a durable request; only the live owner may signal its own child.
pub fn observe(root: &Path, id: &str) -> Result<RunSummary> {
    let db = control_connection(root, false)?;
    verify_events(&db)?;
    load_opt(&db, id)?.ok_or(RuntimeError::NotFound)
}
/// Read-only evidence inspection is available while the supervision owner runs.
pub fn observe_cleanup(root: &Path, id: &str) -> Result<router_types::ProcessCleanupEvidence> {
    let db = control_connection(root, false)?;
    verify_events(&db)?;
    cleanup_evidence(&db, id)
}
fn cleanup_evidence(db: &Connection, id: &str) -> Result<router_types::ProcessCleanupEvidence> {
    let run = load_opt(db, id)?.ok_or(RuntimeError::NotFound)?;
    if db.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))? < 6 {
        return Ok(router_types::ProcessCleanupEvidence::Unknown);
    }
    if cleanup_blocked(db, Some(id))?
        || (db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))? >= 8
            && !preparation::terminal_safe(db, &run)?)
    {
        return Ok(router_types::ProcessCleanupEvidence::Unknown);
    }
    let evidence: Option<Option<String>> = db
        .query_row(
            "SELECT evidence FROM attempt_leases WHERE run_id=?1 AND attempt=?2",
            params![id, run.attempts],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match evidence.flatten() {
        Some(value) => serde_json::from_str(&value)?,
        None => router_types::ProcessCleanupEvidence::Unknown,
    })
}
pub fn observe_events(root: &Path, id: &str) -> Result<Vec<RuntimeEvent>> {
    let db = control_connection(root, false)?;
    verify_events(&db)?;
    load_opt(&db, id)?.ok_or(RuntimeError::NotFound)?;
    Ok(read_events(&db)?
        .into_iter()
        .filter(|event| event.run_id == id)
        .collect())
}
pub fn request_cancel(root: &Path, id: &str) -> Result<()> {
    let mut db = control_connection(root, true)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    verify_events(&tx)?;
    let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
    if run.state == RunState::Cancelled {
        return Ok(());
    }
    if !matches!(
        run.state,
        RunState::Planned
            | RunState::AwaitingApproval
            | RunState::Queued
            | RunState::Preparing
            | RunState::Running
    ) {
        return Err(RuntimeError::Transition);
    }
    tx.execute("INSERT OR IGNORE INTO control_requests VALUES(?1,1)", [id])?;
    append(&tx, &run, "control.cancel_requested", "")?;
    if matches!(
        run.state,
        RunState::Planned | RunState::AwaitingApproval | RunState::Queued
    ) {
        run.state = RunState::Cancelled;
        run.failure = None;
        save(&tx, &run)?;
        tx.execute("DELETE FROM writer_locks WHERE run_id=?1", [id])?;
        append(&tx, &run, "control.cancelled_before_start", "")?;
    }
    tx.commit()?;
    Ok(())
}
fn control_connection(root: &Path, writable: bool) -> Result<Connection> {
    let root_meta = fs::symlink_metadata(root)?;
    let path = root.join("runtime.sqlite3");
    let meta = fs::symlink_metadata(&path)?;
    if !root.is_absolute()
        || !root_meta.is_dir()
        || root_meta.file_type().is_symlink()
        || root_meta.permissions().mode() & 0o077 != 0
        || !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.permissions().mode() & 0o077 != 0
    {
        return Err(RuntimeError::Invalid("private storage"));
    }
    let flags = if writable {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    if !matches!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))?,
        2..=8
    ) {
        return Err(RuntimeError::Integrity("control schema"));
    }
    Ok(db)
}

fn read_events(db: &Connection) -> Result<Vec<RuntimeEvent>> {
    let mut q=db.prepare("SELECT sequence,run_id,kind,state,detail,previous_hash,event_hash FROM events ORDER BY sequence")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(sequence, run_id, kind, state, detail, previous_hash, event_hash)| {
                Ok(RuntimeEvent {
                    sequence,
                    run_id,
                    kind,
                    state: serde_json::from_str(&state)?,
                    detail,
                    previous_hash,
                    event_hash,
                })
            },
        )
        .collect()
}
fn verify_events(db: &Connection) -> Result<()> {
    let mut previous = String::new();
    for (index, event) in read_events(db)?.iter().enumerate() {
        if event.sequence != index as u64 + 1
            || event.previous_hash != previous
            || event.event_hash != event_digest(event)?
        {
            return Err(RuntimeError::Integrity("event chain"));
        }
        previous = event.event_hash.clone();
    }
    // Older schemas have no generated authority. Their legacy records remain
    // readable; the v7 migration never infers a generated pin from mutable data.
    if db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))? >= 7 {
        verify_authority_pins(db)?;
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanAuthorityAnchor {
    schema_version: u8,
    instance_id: String,
    contract_sha256: String,
    pin: String,
}

fn verify_authority_pins(db: &Connection) -> Result<()> {
    let mut anchors = std::collections::BTreeMap::new();
    let mut query = db.prepare(
        "SELECT detail FROM events WHERE kind='workflow.plan.authority.v1' ORDER BY sequence",
    )?;
    for row in query.query_map([], |r| r.get::<_, String>(0))? {
        let detail = row?;
        if detail.len() > 2048 {
            return Err(RuntimeError::Limit);
        }
        let anchor: PlanAuthorityAnchor = serde_json::from_str(&detail)
            .map_err(|_| RuntimeError::Integrity("workflow plan authority anchor"))?;
        if anchor.schema_version != 1
            || anchor.pin == "legacy"
            || !router_adapter_api::is_digest(&anchor.pin)
            || !router_adapter_api::is_digest(&anchor.contract_sha256)
            || anchors.insert(anchor.instance_id.clone(), anchor).is_some()
        {
            return Err(RuntimeError::Integrity("workflow plan authority anchor"));
        }
    }
    let mut query = db.prepare("SELECT w.instance_id,w.contract_hash,p.pin FROM workflow_instances w LEFT JOIN workflow_plan_authority p ON p.instance_id=w.instance_id")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (instance, contract, pin) = row?;
        match (pin.as_deref(), anchors.remove(&instance)) {
            (Some("legacy"), None) => {}
            (Some(pin), Some(anchor))
                if pin == anchor.pin && contract == anchor.contract_sha256 => {}
            _ => return Err(RuntimeError::Integrity("workflow plan authority changed")),
        }
    }
    if !anchors.is_empty() {
        return Err(RuntimeError::Integrity("workflow plan authority changed"));
    }
    Ok(())
}

fn retryable(run: &RunSummary) -> bool {
    matches!(run.state, RunState::Failed | RunState::TimedOut)
        && run.worktree.is_none()
        && matches!(
            run.failure,
            Some(
                RunFailure::RateLimit
                    | RunFailure::Timeout
                    | RunFailure::Crash
                    | RunFailure::Network
                    | RunFailure::Interrupted
                    | RunFailure::Spawn
            )
        )
}
fn workflow_node(
    db: &Connection,
    instance: &str,
    node: &str,
) -> Result<Option<(RunSummary, ModelId, Option<String>)>> {
    let value: Option<(String, String, Option<String>)> = db.query_row(
        "SELECT r.summary,n.model,n.agent FROM workflow_node_runs n JOIN runs r ON r.run_id=n.run_id WHERE n.instance_id=?1 AND n.node_id=?2 ORDER BY n.generation DESC LIMIT 1",
        params![instance,node], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    value
        .map(|(summary, model, agent)| {
            Ok((
                serde_json::from_str(&summary)?,
                serde_json::from_str(&model)?,
                agent,
            ))
        })
        .transpose()
}
fn workflow_attempts(db: &Connection, run: &RunSummary) -> Result<u32> {
    let Some(binding) = &run.workflow else {
        return Ok(run.attempts);
    };
    let mut query = db.prepare("SELECT r.summary FROM workflow_node_runs n JOIN runs r ON r.run_id=n.run_id WHERE n.instance_id=?1 AND n.node_id=?2")?;
    let rows = query.query_map(params![binding.instance_id, run.node_id], |row| {
        row.get::<_, String>(0)
    })?;
    let mut attempts = 0_u32;
    for row in rows {
        attempts = attempts
            .checked_add(serde_json::from_str::<RunSummary>(&row?)?.attempts)
            .ok_or(RuntimeError::Limit)?;
    }
    Ok(attempts)
}

fn plan_pin(generated: Option<&router_types::GeneratedDecisionBinding>) -> Result<String> {
    generated.map_or(Ok("legacy".into()), |g| {
        digest(
            "runtime-global-plan-pin-v1",
            &(
                g.schema_version,
                &g.instance_id,
                &g.workflow_plan_id,
                &g.execution_contract_sha256,
                &g.input_hash,
                &g.proposal_hash,
            ),
        )
    })
}
fn verify_plan_pin(
    db: &Connection,
    workflow: Option<&router_types::WorkflowRunBinding>,
    generated: Option<&router_types::GeneratedDecisionBinding>,
) -> Result<()> {
    let Some(workflow) = workflow else {
        return if generated.is_none() {
            Ok(())
        } else {
            Err(RuntimeError::Integrity("generated plan missing instance"))
        };
    };
    if generated.is_some_and(|g| {
        g.instance_id != workflow.instance_id
            || g.execution_contract_sha256 != workflow.contract_sha256
    }) {
        return Err(RuntimeError::Integrity("generated plan instance"));
    }
    let pin: Option<String> = db
        .query_row(
            "SELECT pin FROM workflow_plan_authority WHERE instance_id=?1",
            [&workflow.instance_id],
            |r| r.get(0),
        )
        .optional()?;
    verify_events(db)?;
    if pin.as_deref() != Some(plan_pin(generated)?.as_str()) {
        return Err(RuntimeError::Integrity("workflow plan authority changed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn summary(id: &str) -> RunSummary {
        RunSummary {
            run_id: id.into(),
            definition_hash: "a".repeat(64),
            approval_digest: "b".repeat(64),
            receipt_id: "c".repeat(64),
            plan_id: format!("plan-{id}"),
            node_id: "execute".into(),
            state: RunState::Planned,
            attempts: 0,
            failure: None,
            worktree: None,
            artifact_sha256: None,
            output_retention: OutputRetentionPolicy::Disabled,
            workflow: None,
            artifact_contract_sha256: None,
        }
    }
    #[test]
    fn interrupted_preparation_without_child_evidence_stays_quarantined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        store.register(&summary("run")).unwrap();
        store.register(&summary("next")).unwrap();
        for state in [
            RunState::AwaitingApproval,
            RunState::Queued,
            RunState::Preparing,
        ] {
            store.transition("run", state, "legacy", None).unwrap();
        }
        store
            .writer_lock("run", "/fixture/repo", &["src".into()])
            .unwrap();
        // Legacy version probes have a lease, but no durable binding to the run.
        // There is no main attempt yet. Preparing alone proves no child cleanup.
        let (_, path) = store.leases.create("unbound-version-probe", 0).unwrap();
        let held = Flock::lock(File::open(path).unwrap(), FlockArg::LockExclusiveNonblock).unwrap();
        drop(store);
        let store = Store::open(&root).unwrap();
        assert!(matches!(
            store.require_cleanup_complete(),
            Err(RuntimeError::CleanupUnknown)
        ));
        assert!(matches!(
            store.writer_lock("next", "/fixture/repo", &["src".into()]),
            Err(RuntimeError::Scope)
        ));
        assert_eq!(store.get("run").unwrap().attempts, 0);
        drop(held);
        drop(store);
        let store = Store::open(&root).unwrap();
        // An unlocked, unbound file cannot retrospectively establish provenance.
        assert!(matches!(
            store.require_cleanup_complete(),
            Err(RuntimeError::CleanupUnknown)
        ));
    }
    #[test]
    fn schema_three_migration_preserves_history_without_inventing_instances() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        let before = store.register(&summary("old")).unwrap();
        let events = store.events().unwrap();
        drop(store);
        let db = Connection::open(root.join("runtime.sqlite3")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF;
            DROP TABLE preparation_cycles; DROP TABLE workflow_plan_authority; DROP TABLE attempt_leases; DROP TABLE cleanup_blocks; DROP TABLE declared_artifacts; DROP TABLE workflow_node_runs; DROP TABLE workflow_instances;
            CREATE TABLE old_runs(run_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, node_id TEXT NOT NULL, summary TEXT NOT NULL, UNIQUE(plan_id,node_id));
            INSERT INTO old_runs SELECT * FROM runs; DROP TABLE runs; ALTER TABLE old_runs RENAME TO runs;
            PRAGMA user_version=3;").unwrap();
        drop(db);
        let store = Store::open(&root).unwrap();
        assert_eq!(store.get("old").unwrap(), before);
        assert_eq!(store.events().unwrap(), events);
        assert!(store.get("old").unwrap().workflow.is_none());
        let db = store.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM workflow_instances", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            0
        );
        assert!(
            !db.prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
        assert!(
            db.execute("UPDATE events SET detail='changed'", [])
                .is_err()
        );
        drop(db);
        let mut unrelated = summary("new");
        unrelated.plan_id = before.plan_id.clone();
        store.register(&unrelated).unwrap();
        assert_eq!(observe(&root, "old").unwrap(), before);
    }
    #[test]
    fn invalid_transition_is_atomic_and_emits_integrity_event() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("data")).unwrap();
        store.register(&summary("run")).unwrap();
        assert!(matches!(
            store.transition("run", RunState::Running, "", None),
            Err(RuntimeError::Transition)
        ));
        assert_eq!(store.get("run").unwrap().state, RunState::Planned);
        assert_eq!(
            store.events().unwrap().last().unwrap().kind,
            "integrity.invalid_transition"
        );
        store.verify_events().unwrap();
    }
    #[test]
    fn pre_evidence_attempts_are_not_backfilled_when_reopened() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
        let store = Store::open(&root).unwrap();
        store.register(&summary("legacy")).unwrap();
        for state in [RunState::AwaitingApproval, RunState::Queued] {
            store.transition("legacy", state, "legacy", None).unwrap();
        }
        let prep = store.begin_preparation("legacy").unwrap();
        prep.no_child_yet().unwrap();
        prep.complete().unwrap();
        store.attempt("legacy", 2, false).unwrap();
        store
            .transition("legacy", RunState::Running, "legacy", None)
            .unwrap();
        store.finish_attempt("legacy", "Exited").unwrap();
        store
            .transition(
                "legacy",
                RunState::Failed,
                "legacy",
                Some(RunFailure::Crash),
            )
            .unwrap();
        let original = store.events().unwrap();
        drop(store);
        let store = Store::open(&root).unwrap();
        let history = store.escalation_history("legacy").unwrap();
        assert_eq!(history.attempts.len(), 1);
        assert!(
            !history
                .events
                .iter()
                .any(|e| e.kind == "attempt.evidence.v1")
        );
        assert_eq!(store.events().unwrap(), original);
    }
    #[test]
    fn planned_cancellation_is_immediate_idempotent_and_releases_writer_locks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        store.register(&summary("run")).unwrap();
        store.register(&summary("next")).unwrap();
        store
            .writer_lock("run", "/fixture/repo", &["src".into()])
            .unwrap();
        assert!(RunState::Planned.permits(RunState::Cancelled));
        request_cancel(&root, "run").unwrap();
        assert_eq!(observe(&root, "run").unwrap().state, RunState::Cancelled);
        let count = store.events().unwrap().len();
        request_cancel(&root, "run").unwrap();
        assert_eq!(store.events().unwrap().len(), count);
        store
            .writer_lock("next", "/fixture/repo", &["src".into()])
            .unwrap();
        assert!(matches!(
            store.transition_queued("run", RunState::Preparing, "", None),
            Err(RuntimeError::Cancelled)
        ));
        assert_eq!(store.events().unwrap().len(), count);
        store.verify_events().unwrap();
    }
    #[test]
    fn recovery_classifies_interrupted_and_preserves_verified_steps() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        store.register(&summary("run")).unwrap();
        for state in [
            RunState::AwaitingApproval,
            RunState::Queued,
            RunState::Preparing,
            RunState::Running,
        ] {
            store.transition("run", state, "", None).unwrap();
        }
        drop(store);
        let store = Store::open(&root).unwrap();
        assert_eq!(
            store.get("run").unwrap().failure,
            Some(RunFailure::Interrupted)
        );
        assert_eq!(store.get("run").unwrap().state, RunState::Failed);
        assert!(matches!(
            store.require_cleanup_complete(),
            Err(RuntimeError::CleanupUnknown)
        ));
        store.verify_events().unwrap();
    }
    #[test]
    fn recovery_requires_private_lease_proof_before_releasing_writer_authority() {
        for status in [
            "pending",
            "group-clean",
            "namespace-clean",
            "active",
            "corrupt",
            "absent",
            "symlink",
            "held",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("data");
            let store = Store::open(&root).unwrap();
            store.register(&summary("run")).unwrap();
            store.register(&summary("next")).unwrap();
            for state in [RunState::AwaitingApproval, RunState::Queued] {
                store.transition("run", state, "", None).unwrap();
            }
            store
                .writer_lock("run", "/fixture/repo", &["src".into()])
                .unwrap();
            let prep = store.begin_preparation("run").unwrap();
            prep.no_child_yet().unwrap();
            prep.complete().unwrap();
            let path = store.attempt("run", 2, true).unwrap().unwrap();
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
            store
                .transition("run", RunState::Running, "", None)
                .unwrap();
            let held = match status {
                "absent" => {
                    fs::remove_file(&path).unwrap();
                    None
                }
                "symlink" => {
                    fs::remove_file(&path).unwrap();
                    std::os::unix::fs::symlink(temp.path().join("unowned"), &path).unwrap();
                    None
                }
                "held" => Some(
                    Flock::lock(File::open(&path).unwrap(), FlockArg::LockExclusiveNonblock)
                        .unwrap(),
                ),
                _ => {
                    fs::write(&path, format!("router-runtime-lease-v1:{status}\n")).unwrap();
                    None
                }
            };
            drop(store);
            let store = Store::open(&root).unwrap();
            assert_eq!(
                store.get("run").unwrap().failure,
                Some(RunFailure::Interrupted)
            );
            let expected = match status {
                "pending" => Some(router_types::ProcessCleanupEvidence::NotStarted),
                "group-clean" => Some(router_types::ProcessCleanupEvidence::GroupTerminated),
                "namespace-clean" => {
                    Some(router_types::ProcessCleanupEvidence::NamespaceTerminated)
                }
                _ => None,
            };
            if let Some(expected) = expected {
                store.require_cleanup_complete().unwrap();
                assert_eq!(store.cleanup_evidence("run").unwrap(), expected);
                store
                    .writer_lock("next", "/fixture/repo", &["src".into()])
                    .unwrap();
                if status == "pending" {
                    assert_eq!(
                        fs::read(&path).unwrap(),
                        b"router-runtime-lease-v1:recovered\n"
                    );
                }
            } else {
                assert!(
                    matches!(
                        store.require_cleanup_complete(),
                        Err(RuntimeError::CleanupUnknown)
                    ),
                    "{status}"
                );
                assert!(
                    matches!(
                        store.writer_lock("next", "/fixture/repo", &["src".into()]),
                        Err(RuntimeError::Scope)
                    ),
                    "{status}"
                );
                assert!(
                    matches!(store.retry("run", 2), Err(RuntimeError::CleanupUnknown)),
                    "{status}"
                );
            }
            assert!(!temp.path().join("unowned").exists());
            drop(held);
        }
    }
    #[test]
    fn unsafe_normal_completion_retains_writer_lock_and_later_proof_reconciles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        store.register(&summary("run")).unwrap();
        store.register(&summary("next")).unwrap();
        for state in [RunState::AwaitingApproval, RunState::Queued] {
            store.transition("run", state, "", None).unwrap();
        }
        store
            .writer_lock("run", "/fixture/repo", &["src".into()])
            .unwrap();
        let prep = store.begin_preparation("run").unwrap();
        prep.no_child_yet().unwrap();
        prep.complete().unwrap();
        let path = store.attempt("run", 2, true).unwrap().unwrap();
        fs::write(&path, b"router-runtime-lease-v1:active\n").unwrap();
        store
            .transition("run", RunState::Running, "", None)
            .unwrap();
        assert!(matches!(
            store.confirm_attempt_cleanup("run"),
            Err(RuntimeError::CleanupUnknown)
        ));
        store
            .transition(
                "run",
                RunState::Failed,
                "cleanup_unknown",
                Some(RunFailure::Interrupted),
            )
            .unwrap();
        assert!(matches!(
            store.writer_lock("next", "/fixture/repo", &["src".into()]),
            Err(RuntimeError::Scope)
        ));
        fs::write(&path, b"router-runtime-lease-v1:group-clean\n").unwrap();
        drop(store);
        let store = Store::open(&root).unwrap();
        store.require_cleanup_complete().unwrap();
        store
            .writer_lock("next", "/fixture/repo", &["src".into()])
            .unwrap();
        assert_eq!(store.get("run").unwrap().attempts, 1);
    }
    #[test]
    fn corrupt_lease_record_cannot_release_a_writer_on_error_transition() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("data")).unwrap();
        store.register(&summary("run")).unwrap();
        store.register(&summary("next")).unwrap();
        for state in [RunState::AwaitingApproval, RunState::Queued] {
            store.transition("run", state, "", None).unwrap();
        }
        store
            .writer_lock("run", "/fixture/repo", &["src".into()])
            .unwrap();
        let prep = store.begin_preparation("run").unwrap();
        prep.no_child_yet().unwrap();
        prep.complete().unwrap();
        store.attempt("run", 2, true).unwrap();
        store
            .transition("run", RunState::Running, "", None)
            .unwrap();
        store
            .lock()
            .unwrap()
            .execute("UPDATE attempt_leases SET record='invalid-json'", [])
            .unwrap();
        assert!(matches!(
            store.confirm_attempt_cleanup("run"),
            Err(RuntimeError::CleanupUnknown)
        ));
        store
            .transition(
                "run",
                RunState::Failed,
                "cleanup_unknown",
                Some(RunFailure::Interrupted),
            )
            .unwrap();
        assert!(matches!(
            store.writer_lock("next", "/fixture/repo", &["src".into()]),
            Err(RuntimeError::Scope)
        ));
        assert!(matches!(
            store.require_cleanup_complete(),
            Err(RuntimeError::CleanupUnknown)
        ));
        assert_eq!(
            observe_cleanup(&store.root, "run").unwrap(),
            router_types::ProcessCleanupEvidence::Unknown
        );
    }
    #[test]
    fn exclusive_lock_private_files_append_only_and_tamper_detection() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("data");
        let store = Store::open(&root).unwrap();
        assert!(matches!(Store::open(&root), Err(RuntimeError::Locked)));
        store.register(&summary("run")).unwrap();
        let db = store.lock().unwrap();
        assert!(
            db.execute("UPDATE events SET detail='changed'", [])
                .is_err()
        );
        assert!(db.execute("DELETE FROM events", []).is_err());
        assert_eq!(
            db.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        drop(db);
        assert_eq!(
            fs::metadata(root.join("runtime.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
        drop(store);
        let db = Connection::open(root.join("runtime.sqlite3")).unwrap();
        db.execute_batch("DROP TRIGGER events_no_update; UPDATE events SET detail='tampered';")
            .unwrap();
        drop(db);
        assert!(matches!(
            Store::open(&root),
            Err(RuntimeError::Integrity("event chain"))
        ));
    }
    #[test]
    fn overlapping_writers_are_rejected_across_workflows() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(&temp.path().join("data")).unwrap();
        store.register(&summary("one")).unwrap();
        store.register(&summary("two")).unwrap();
        store
            .writer_lock("one", "/fixture/repo", &["src".into()])
            .unwrap();
        assert!(matches!(
            store.writer_lock("two", "/fixture/repo", &["src/nested/file".into()]),
            Err(RuntimeError::Scope)
        ));
        store
            .writer_lock("two", "/fixture/repo", &["tests".into()])
            .unwrap();
    }
}
