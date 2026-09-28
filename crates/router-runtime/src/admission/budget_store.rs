//! Bounded event journal and projections inside the admission transaction.
use super::*;
use budget::{BudgetError, add, sub, within};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::BTreeMap;
const MAX_EVENTS: u64 = 16384;
const MAX_STATE: usize = 16384;
pub(super) fn load(
    db: &Connection,
    policy: &SessionBudgetPolicy,
    registry: &str,
) -> Result<BTreeMap<String, SessionBudgetStatus>> {
    let count: u64 = db.query_row("SELECT count(*) FROM budget_events", [], |r| r.get(0))?;
    if count > MAX_EVENTS {
        return Err(RuntimeError::Limit);
    }
    // Check every variable field before retrieving it, including indirect metadata.
    if db.prepare("SELECT 1 FROM budget_events WHERE length(session)>160 OR length(kind)>32 OR length(ticket)>64 OR length(previous_hash)>64 OR length(event_hash)>64 OR length(state)>16384 LIMIT 1")?.exists([])?{return Err(RuntimeError::Limit)}
    let mut q=db.prepare("SELECT sequence,session,kind,ticket,previous_hash,state,event_hash FROM budget_events ORDER BY sequence")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, u64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let hash = budget::session_budget_policy_hash(policy)?;
    let mut previous = String::new();
    let mut states = BTreeMap::new();
    for (index, row) in rows.enumerate() {
        let (seq, id, kind, ticket, prev, json, event_hash) = row?;
        if seq != index as u64 + 1 || json.len() > MAX_STATE || previous != prev {
            return Err(RuntimeError::Integrity("budget journal"));
        }
        let mut state: SessionBudgetStatus = serde_json::from_str(&json)?;
        if json != serde_json::to_string(&state)?
            || state.schema_version != 1
            || state.binding.session_id != id
            || state.binding.policy_sha256 != hash
            || state.registry_sha256 != registry
            || !policy.sessions.contains_key(&id)
            || state.parent_ceiling != policy.sessions[&id].parent_ceiling
            || !within(state.limit, state.parent_ceiling)
            || state.limit_revision == 0
            || state.limit_revision > 1024
            || state.warnings.len() > 16
            || state.observed_cost_microusd.is_some()
            || state.observed_tokens.is_some()
            || state.ledger_head != event_hash
        {
            return Err(RuntimeError::Integrity("budget projection"));
        }
        state.ledger_head.clear();
        if digest(
            "session-budget-event-v1",
            &(seq, &prev, &id, &kind, &ticket, &state),
        )? != event_hash
        {
            return Err(RuntimeError::Integrity("budget event hash"));
        }
        state.ledger_head = event_hash.clone();
        previous = event_hash;
        states.insert(id, state);
    }
    if states.len() != policy.sessions.len() {
        return Err(RuntimeError::Integrity("budget sessions"));
    }
    let projections: u64 =
        db.query_row("SELECT count(*) FROM budget_sessions", [], |r| r.get(0))?;
    if projections != states.len() as u64 {
        return Err(RuntimeError::Integrity("budget sessions"));
    }
    for (id, state) in &states {
        let json: String = db.query_row(
            "SELECT substr(json,1,16385) FROM budget_sessions WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if json != serde_json::to_string(state)? {
            return Err(RuntimeError::Integrity("budget projection anchor"));
        }
    }
    let count: u64 = db.query_row("SELECT count(*) FROM budget_reservations", [], |r| r.get(0))?;
    if count > u64::from(MAX_TICKETS) {
        return Err(RuntimeError::Limit);
    }
    if db.prepare("SELECT 1 FROM budget_reservations WHERE length(ticket)>64 OR length(session)>160 OR length(state)>16 OR length(amount)>512 LIMIT 1")?.exists([])?{return Err(RuntimeError::Limit)}
    let mut q=db.prepare("SELECT b.ticket,b.session,b.state,b.amount,substr(t.record,1,16385) FROM budget_reservations b JOIN tickets t ON t.id=b.ticket")?;
    let mut sums: BTreeMap<String, (BudgetAmount, BudgetAmount)> = BTreeMap::new();
    let mut seen = 0u64;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (id, session, kind, json, record) = row?;
        if record.len() > 16384 || json.len() > 512 {
            return Err(RuntimeError::Limit);
        }
        let ticket: Ticket = serde_json::from_str(&record)?;
        ticket.verify()?;
        let c = ticket.session_budget.ok_or(BudgetError::Authority)?;
        let amount: BudgetAmount = serde_json::from_str(&json)?;
        if id != ticket.id
            || c.session_id != session
            || amount != c.attempt_allocation
            || !states.contains_key(&session)
            || !budget::valid_amount(amount)
            || c.expected_policy_sha256 != hash
            || policy.runtime_sessions.get(&ticket.binding.runtime_id) != Some(&session)
        {
            return Err(RuntimeError::Integrity("budget reservation"));
        }
        let sum = sums.entry(session).or_default();
        match kind.as_str() {
            "reserved" => sum.0 = add(sum.0, amount)?,
            "committed" => sum.1 = add(sum.1, amount)?,
            "released" => {}
            _ => return Err(RuntimeError::Integrity("budget reservation state")),
        }
        seen += 1;
    }
    if seen != count {
        return Err(RuntimeError::Integrity("budget missing ticket"));
    }
    for (id, state) in &states {
        let (reserved, committed) = sums.get(id).copied().unwrap_or_default();
        if state.reserved_allocation != reserved || state.committed_allocation != committed {
            return Err(RuntimeError::Integrity("budget counters"));
        }
    }
    Ok(states)
}
pub(super) fn persist(
    tx: &Transaction<'_>,
    mut state: SessionBudgetStatus,
    kind: &str,
    ticket: Option<&str>,
    rule: &SessionBudgetRule,
) -> Result<SessionBudgetStatus> {
    let used = add(state.reserved_allocation, state.committed_allocation)?;
    for dimension in [BudgetDimension::Cost, BudgetDimension::Tokens] {
        let (amount, limit) = match dimension {
            BudgetDimension::Cost => (used.cost_microusd, state.limit.cost_microusd),
            BudgetDimension::Tokens => (used.tokens, state.limit.tokens),
        };
        for threshold in &rule.warning_basis_points {
            let warning = BudgetWarning {
                limit_revision: state.limit_revision,
                dimension,
                basis_points: *threshold,
            };
            if amount > 0
                && u128::from(amount) * 10000 >= u128::from(limit) * u128::from(*threshold)
                && !state.warnings.contains(&warning)
            {
                state.warnings.push(warning)
            }
        }
    }
    let count: u64 = tx.query_row("SELECT count(*) FROM budget_events", [], |r| r.get(0))?;
    if count >= MAX_EVENTS {
        return Err(RuntimeError::Limit);
    }
    let previous: String = tx
        .query_row(
            "SELECT event_hash FROM budget_events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default();
    state.ledger_head.clear();
    let seq = count + 1;
    let head = digest(
        "session-budget-event-v1",
        &(
            seq,
            &previous,
            &state.binding.session_id,
            kind,
            ticket,
            &state,
        ),
    )?;
    state.ledger_head = head.clone();
    let json = serde_json::to_string(&state)?;
    if json.len() > MAX_STATE {
        return Err(RuntimeError::Limit);
    }
    tx.execute(
        "INSERT INTO budget_events VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            seq,
            state.binding.session_id,
            kind,
            ticket,
            previous,
            json,
            head
        ],
    )?;
    tx.execute("INSERT INTO budget_sessions VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",params![state.binding.session_id,json])?;
    Ok(state)
}
pub(super) fn reserve(
    tx: &Transaction<'_>,
    ticket: &Ticket,
    policy: &SessionBudgetPolicy,
    registry: &str,
) -> Result<()> {
    let c = ticket
        .session_budget
        .as_ref()
        .ok_or(BudgetError::Authority)?;
    let mut states = load(tx, policy, registry)?;
    let reasons = budget::reasons(ticket, &states);
    if reasons.contains(&AdmissionQueueReason::SessionCostBudget) {
        return Err(BudgetError::Cost.into());
    }
    if reasons.contains(&AdmissionQueueReason::SessionTokenBudget) {
        return Err(BudgetError::Tokens.into());
    }
    let mut state = states.remove(&c.session_id).ok_or(BudgetError::Authority)?;
    tx.execute(
        "INSERT INTO budget_reservations VALUES(?1,?2,'reserved',?3)",
        params![
            ticket.id,
            c.session_id,
            serde_json::to_string(&c.attempt_allocation)?
        ],
    )?;
    state.reserved_allocation = add(state.reserved_allocation, c.attempt_allocation)?;
    persist(
        tx,
        state,
        "reserved",
        Some(&ticket.id),
        &policy.sessions[&c.session_id],
    )?;
    Ok(())
}
pub(super) fn commit(
    tx: &Transaction<'_>,
    ticket: &Ticket,
    policy: &SessionBudgetPolicy,
    registry: &str,
    dispatch: bool,
) -> Result<()> {
    let c = ticket
        .session_budget
        .as_ref()
        .ok_or(BudgetError::Authority)?;
    let mut state = load(tx, policy, registry)?
        .remove(&c.session_id)
        .ok_or(BudgetError::Authority)?;
    let status: Option<String> = tx
        .query_row(
            "SELECT state FROM budget_reservations WHERE ticket=?1",
            [&ticket.id],
            |r| r.get(0),
        )
        .optional()?;
    match status.as_deref() {
        Some("committed") => {
            return if dispatch {
                Err(BudgetError::Authority.into())
            } else {
                Ok(())
            };
        }
        Some("reserved") => {}
        _ => return Err(BudgetError::Unknown.into()),
    }
    if dispatch {
        let used = add(state.reserved_allocation, state.committed_allocation)?;
        if used.cost_microusd
            > state
                .limit
                .cost_microusd
                .min(c.approved_limit.cost_microusd)
        {
            return Err(BudgetError::Cost.into());
        }
        if used.tokens > state.limit.tokens.min(c.approved_limit.tokens) {
            return Err(BudgetError::Tokens.into());
        }
        let granted: bool = tx.query_row(
            "SELECT state='granted' FROM tickets WHERE id=?1",
            [&ticket.id],
            |r| r.get(0),
        )?;
        if !granted {
            return Err(BudgetError::Authority.into());
        }
    }
    tx.execute(
        "UPDATE budget_reservations SET state='committed' WHERE ticket=?1",
        [&ticket.id],
    )?;
    state.reserved_allocation = sub(state.reserved_allocation, c.attempt_allocation)?;
    state.committed_allocation = add(state.committed_allocation, c.attempt_allocation)?;
    persist(
        tx,
        state,
        if dispatch {
            "dispatch_committed"
        } else {
            "armed_recovered"
        },
        Some(&ticket.id),
        &policy.sessions[&c.session_id],
    )?;
    Ok(())
}
pub(super) fn release(
    tx: &Transaction<'_>,
    proof: &ReleaseProof,
    policy: &SessionBudgetPolicy,
    registry: &str,
) -> Result<()> {
    let ticket = &proof.ticket;
    let c = ticket
        .session_budget
        .as_ref()
        .ok_or(BudgetError::Authority)?;
    let mut state = load(tx, policy, registry)?
        .remove(&c.session_id)
        .ok_or(BudgetError::Authority)?;
    let status: Option<String> = tx
        .query_row(
            "SELECT state FROM budget_reservations WHERE ticket=?1",
            [&ticket.id],
            |r| r.get(0),
        )
        .optional()?;
    match status.as_deref() {
        Some("committed" | "released") => Ok(()),
        None if !proof.budget_armed => Ok(()),
        Some("reserved") if proof.budget_armed => commit(tx, ticket, policy, registry, false),
        Some("reserved") => {
            tx.execute(
                "UPDATE budget_reservations SET state='released' WHERE ticket=?1",
                [&ticket.id],
            )?;
            state.reserved_allocation = sub(state.reserved_allocation, c.attempt_allocation)?;
            persist(
                tx,
                state,
                "no_dispatch_released",
                Some(&ticket.id),
                &policy.sessions[&c.session_id],
            )?;
            Ok(())
        }
        _ => Err(BudgetError::Unknown.into()),
    }
}
