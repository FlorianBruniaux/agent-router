use crate::{Result, RuntimeError, digest};
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

pub(crate) struct Store {
    db: Mutex<Connection>,
    pub root: PathBuf,
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
        if version > 4 {
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
        for suffix in ["", "-wal", "-shm"] {
            let path = root.join(format!("runtime.sqlite3{suffix}"));
            if path.exists() {
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
        }
        let store = Self {
            db: Mutex::new(db),
            root,
            _lock: lock,
        };
        store.verify_events()?;
        store.recover()?;
        Ok(store)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| RuntimeError::Integrity("storage lock poisoned"))
    }
    #[cfg(test)]
    pub fn register(&self, summary: &RunSummary) -> Result<RunSummary> {
        self.register_inner(summary, None, None, 5)
    }
    pub fn register_workflow(
        &self,
        summary: &RunSummary,
        contract: Option<&WorkflowExecutionContract>,
        request: &ExecutionRequest,
        max_attempts: u32,
    ) -> Result<RunSummary> {
        self.register_inner(summary, contract, Some(request), max_attempts)
    }
    fn register_inner(
        &self,
        summary: &RunSummary,
        contract: Option<&WorkflowExecutionContract>,
        request: Option<&ExecutionRequest>,
        max_attempts: u32,
    ) -> Result<RunSummary> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(existing) = load_opt(&tx, &summary.run_id)? {
            if existing.definition_hash != summary.definition_hash {
                return Err(RuntimeError::Integrity("definition changed"));
            }
            return Ok(existing);
        }
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
            tx.execute(
                "INSERT OR IGNORE INTO workflow_instances VALUES(?1,?2,?3)",
                params![
                    binding.instance_id,
                    binding.contract_sha256,
                    serde_json::to_string(contract)?
                ],
            )?;
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
        } else if contract.is_some() {
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
        tx.commit()?;
        Ok(summary.clone())
    }
    pub fn get(&self, id: &str) -> Result<RunSummary> {
        load_opt(&*self.lock()?, id)?.ok_or(RuntimeError::NotFound)
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
        if next.terminal() {
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
    pub fn attempt(&self, id: &str, max: u32) -> Result<u32> {
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        if run.state != RunState::Preparing || workflow_attempts(&tx, &run)? >= max {
            return Err(RuntimeError::Limit);
        }
        run.attempts += 1;
        tx.execute(
            "INSERT INTO run_attempts VALUES(?1,?2,'started')",
            params![id, run.attempts],
        )?;
        save(&tx, &run)?;
        append(&tx, &run, "attempt.started", "")?;
        tx.commit()?;
        Ok(run.attempts)
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
        let mut run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        let obsolete = if let Some(binding) = &run.workflow {
            workflow_node(&tx, &binding.instance_id, &run.node_id)?
                .is_none_or(|(current, _, _)| current.run_id != run.run_id)
        } else {
            false
        };
        if !retryable(&run) || workflow_attempts(&tx, &run)? >= max || obsolete {
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
        for value in values {
            let mut run: RunSummary = serde_json::from_str(&value)?;
            if matches!(run.state, RunState::Preparing | RunState::Running) {
                run.state = RunState::Failed;
                run.failure = Some(RunFailure::Interrupted);
                save(&tx, &run)?;
                append(
                    &tx,
                    &run,
                    "recovery.interrupted",
                    "process_liveness_unknown_no_replay",
                )?;
                tx.execute("DELETE FROM writer_locks WHERE run_id=?1", [&run.run_id])?;
            } else if run.state == RunState::SucceededTransport {
                run.state = RunState::Verifying;
                save(&tx, &run)?;
                append(&tx, &run, "recovery.verification_required", "")?;
            }
        }
        tx.commit()?;
        Ok(())
    }
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
    if detail.len() > 2048 || kind.len() > 128 {
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
        2..=4
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

#[cfg(test)]
mod tests {
    use super::*;
    fn summary(id: &str) -> RunSummary {
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
        }
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
            DROP TABLE workflow_node_runs; DROP TABLE workflow_instances;
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
        store.verify_events().unwrap();
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
