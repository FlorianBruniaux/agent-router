use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::sync::Mutex;

pub struct AdmissionStore {
    db: Mutex<Connection>,
    files: files::Files,
    policy: AdmissionPolicy,
    policy_hash: String,
    registry_hash: String,
}
impl AdmissionStore {
    /// Explicitly initializes an empty private registry; never imports legacy runs.
    pub fn create(root: &Path, policy: &AdmissionPolicy) -> Result<Self> {
        let hash = admission_policy_hash(policy)?;
        Self::load(root, &hash, Some(policy))
    }
    pub fn open(root: &Path, expected_policy_sha256: &str) -> Result<Self> {
        Self::load(root, expected_policy_sha256, None)
    }
    fn load(root: &Path, expected: &str, create: Option<&AdmissionPolicy>) -> Result<Self> {
        if !router_adapter_api::is_digest(expected) {
            return Err(RuntimeError::Invalid("admission policy pin"));
        }
        let files = files::Files::open(root, create.is_some())?;
        let mut db = Connection::open(files.path.join("admission.sqlite3"))?;
        db.busy_timeout(std::time::Duration::from_millis(250))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        if let Some(policy) = create {
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch("CREATE TABLE config(id INTEGER PRIMARY KEY CHECK(id=1),json TEXT NOT NULL); CREATE TABLE groups(id TEXT PRIMARY KEY,turn INTEGER NOT NULL); CREATE TABLE tickets(sequence INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,owner TEXT NOT NULL,group_id TEXT NOT NULL REFERENCES groups(id),state TEXT NOT NULL CHECK(state IN ('waiting','granted','released')),record TEXT NOT NULL); CREATE TABLE clock(id INTEGER PRIMARY KEY CHECK(id=1),tick INTEGER NOT NULL); INSERT INTO clock VALUES(1,0); PRAGMA user_version=1;")?;
            tx.execute(
                "INSERT INTO config VALUES(1,?1)",
                [serde_json::to_string(policy)?],
            )?;
            tx.commit()?;
        }
        let version: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version != 1 {
            return Err(RuntimeError::Integrity("admission schema"));
        }
        let json: String = db.query_row(
            "SELECT substr(json,1,131073) FROM config WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if json.len() > 128 * 1024 {
            return Err(RuntimeError::Limit);
        }
        let policy: AdmissionPolicy = serde_json::from_str(&json)?;
        let policy_hash = admission_policy_hash(&policy)?;
        if policy_hash != expected || json != serde_json::to_string(&policy)? {
            return Err(RuntimeError::Approval);
        }
        let registry_hash = files.binding_hash()?;
        files.check()?;
        Ok(Self {
            db: Mutex::new(db),
            files,
            policy,
            policy_hash,
            registry_hash,
        })
    }
    pub fn policy(&self) -> &AdmissionPolicy {
        &self.policy
    }
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }
    pub(crate) fn binding(&self, runtime_id: &str) -> Result<AdmissionBinding> {
        if !self.policy.runtimes.contains_key(runtime_id) {
            return Err(RuntimeError::Invalid("admission unknown runtime"));
        }
        Ok(AdmissionBinding {
            schema_version: 1,
            registry_sha256: self.registry_hash.clone(),
            policy_sha256: self.policy_hash.clone(),
            runtime_id: runtime_id.into(),
        })
    }
    pub(super) fn owner(&self, id: &str) -> Result<files::Owner> {
        self.files.check()?;
        self.files.owner(id)
    }
    pub(super) fn waiter(&self, ticket: &Ticket) -> Result<files::Owner> {
        self.files.owner(&format!("ticket:{}", ticket.id))
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.files.check()?;
        let db = self
            .db
            .lock()
            .map_err(|_| RuntimeError::Integrity("admission lock"))?;
        let json: String = db.query_row(
            "SELECT substr(json,1,131073) FROM config WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if json != serde_json::to_string(&self.policy)? {
            return Err(RuntimeError::Approval);
        }
        Ok(db)
    }
    fn validate_ticket(&self, ticket: &Ticket) -> Result<()> {
        ticket.verify()?;
        if ticket.binding != self.binding(&ticket.binding.runtime_id)?
            || !self.policy.providers.contains_key(&ticket.provider)
            || !self.policy.agents.contains_key(&ticket.agent)
        {
            return Err(RuntimeError::Approval);
        }
        Ok(())
    }
    pub(crate) fn enqueue(&self, ticket: &Ticket) -> Result<()> {
        self.validate_ticket(ticket)?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT record FROM tickets WHERE id=?1",
                [&ticket.id],
                |r| r.get(0),
            )
            .optional()?;
        let record = serde_json::to_string(ticket)?;
        if let Some(existing) = existing {
            if existing != record {
                return Err(RuntimeError::Integrity("admission ticket changed"));
            }
            return Ok(());
        }
        let count: u32 = tx.query_row("SELECT count(*) FROM tickets", [], |r| r.get(0))?;
        if count >= MAX_TICKETS {
            return Err(RuntimeError::Limit);
        }
        let tick: i64 = tx.query_row("SELECT tick FROM clock WHERE id=1", [], |r| r.get(0))?;
        tx.execute(
            "INSERT OR IGNORE INTO groups VALUES(?1,?2)",
            params![ticket.group, tick],
        )?;
        tx.execute(
            "INSERT INTO tickets(id,owner,group_id,state,record) VALUES(?1,?2,?3,'waiting',?4)",
            params![ticket.id, ticket.binding.runtime_id, ticket.group, record],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Entire capacity vector is reserved in one immediate transaction. Only the
    /// polling owner can grant its own ticket; abandoned waiters cannot block it.
    pub(crate) fn poll(&self, ticket: &Ticket) -> Result<Vec<AdmissionQueueReason>> {
        self.validate_ticket(ticket)?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: u32 = tx.query_row("SELECT count(*) FROM tickets", [], |r| r.get(0))?;
        if count > MAX_TICKETS
            || tx
                .prepare("SELECT 1 FROM tickets WHERE length(record)>16384 LIMIT 1")?
                .exists([])?
        {
            return Err(RuntimeError::Limit);
        }
        let rows = {
            let mut q=tx.prepare("SELECT t.id,t.state,t.record FROM tickets t JOIN groups g ON g.id=t.group_id WHERE state!='released' ORDER BY g.turn,t.sequence")?;
            q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        if rows.len() > MAX_TICKETS as usize {
            return Err(RuntimeError::Limit);
        }
        let mut granted = Vec::new();
        let mut waiting = Vec::new();
        let mut found = false;
        for (id, state, json) in rows {
            if json.len() > 16384 {
                return Err(RuntimeError::Limit);
            }
            let item: Ticket = serde_json::from_str(&json)?;
            self.validate_ticket(&item)?;
            if id != item.id {
                return Err(RuntimeError::Integrity("admission row identity"));
            }
            if id == ticket.id {
                found = true;
                if item != *ticket {
                    return Err(RuntimeError::Approval);
                }
                if state == "granted" {
                    return Ok(vec![]);
                }
            }
            if state == "granted" {
                granted.push(item);
            } else if self.files.owner_alive(&item.binding.runtime_id)?
                && self.files.owner_alive(&format!("ticket:{}", item.id))?
            {
                waiting.push(item);
            } else {
                tx.execute(
                    "UPDATE tickets SET state='released' WHERE id=?1 AND state='waiting'",
                    [id],
                )?;
            }
        }
        if !found || !waiting.iter().any(|t| t.id == ticket.id) {
            return Err(RuntimeError::Cancelled);
        }
        if granted.len() > self.policy.global_limit as usize {
            return Err(RuntimeError::Integrity("admission overcapacity"));
        }
        let reasons = self.reasons(ticket, &granted);
        if !reasons.is_empty() {
            tx.commit()?;
            return Ok(reasons);
        }
        let mut groups = BTreeSet::new();
        if waiting
            .iter()
            .filter(|t| groups.insert(&t.group))
            .find(|t| self.reasons(t, &granted).is_empty())
            .map(|t| &t.id)
            != Some(&ticket.id)
        {
            tx.commit()?;
            return Ok(vec![AdmissionQueueReason::FairTurn]);
        }
        tx.execute(
            "UPDATE tickets SET state='granted' WHERE id=?1 AND state='waiting'",
            [&ticket.id],
        )?;
        tx.execute("UPDATE clock SET tick=tick+1 WHERE id=1", [])?;
        tx.execute(
            "UPDATE groups SET turn=(SELECT tick FROM clock WHERE id=1) WHERE id=?1",
            [&ticket.group],
        )?;
        tx.commit()?;
        Ok(vec![])
    }
    fn reasons(&self, t: &Ticket, active: &[Ticket]) -> Vec<AdmissionQueueReason> {
        let count = |f: &dyn Fn(&Ticket) -> bool| active.iter().filter(|v| f(v)).count() as u32;
        let mut out = Vec::new();
        if active.len() as u32 >= self.policy.global_limit {
            out.push(AdmissionQueueReason::GlobalCapacity);
        }
        let same_runtime = |x: &Ticket| x.binding.runtime_id == t.binding.runtime_id;
        let minimum =
            |predicate: &dyn Fn(&Ticket) -> bool, field: fn(&Ticket) -> u32, requested: u32| {
                active
                    .iter()
                    .filter(|v| predicate(v))
                    .map(field)
                    .fold(requested, u32::min)
            };
        if count(&same_runtime)
            >= minimum(&same_runtime, |x| x.runtime_limit, t.runtime_limit)
                .min(self.policy.runtimes[&t.binding.runtime_id].limit)
        {
            out.push(AdmissionQueueReason::RuntimeCapacity);
        }
        if count(&|x| x.provider == t.provider) >= self.policy.providers[&t.provider]
            || count(&|x| same_runtime(x) && x.provider == t.provider)
                >= minimum(
                    &|x| same_runtime(x) && x.provider == t.provider,
                    |x| x.provider_limit,
                    t.provider_limit,
                )
        {
            out.push(AdmissionQueueReason::ProviderCapacity);
        }
        if count(&|x| x.agent == t.agent) >= self.policy.agents[&t.agent]
            || count(&|x| same_runtime(x) && x.agent == t.agent)
                >= minimum(
                    &|x| same_runtime(x) && x.agent == t.agent,
                    |x| x.agent_limit,
                    t.agent_limit,
                )
        {
            out.push(AdmissionQueueReason::AgentCapacity);
        }
        if count(&|x| x.group == t.group)
            >= minimum(
                &|x| x.group == t.group,
                |x| x.workflow_limit,
                t.workflow_limit,
            )
        {
            out.push(AdmissionQueueReason::WorkflowCapacity);
        }
        out
    }
    pub(crate) fn release(&self, proof: &ReleaseProof) -> Result<()> {
        let ticket = &proof.ticket;
        self.validate_ticket(ticket)?;
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json: Option<String> = tx
            .query_row(
                "SELECT record FROM tickets WHERE id=?1",
                [&ticket.id],
                |r| r.get(0),
            )
            .optional()?;
        let expected = serde_json::to_string(ticket)?;
        if json.as_deref().is_some_and(|j| j != expected) {
            return Err(RuntimeError::Integrity("admission release identity"));
        }
        // Missing means the runtime intent committed before registry enqueue.
        tx.execute(
            "UPDATE tickets SET state='released' WHERE id=?1",
            [&ticket.id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn status(&self) -> Result<AdmissionStatus> {
        let db = self.lock()?;
        let count = |s: &str| {
            db.query_row("SELECT count(*) FROM tickets WHERE state=?1", [s], |r| {
                r.get(0)
            })
        };
        Ok(AdmissionStatus {
            schema_version: 1,
            policy_sha256: self.policy_hash.clone(),
            waiting: count("waiting")?,
            granted: count("granted")?,
            released: count("released")?,
            max_tickets: MAX_TICKETS,
        })
    }
}
