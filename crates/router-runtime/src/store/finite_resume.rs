// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Append-only, one-shot application recovery. The store flock remains held.
use super::*;
impl Store {
    pub(crate) fn claim_finite_integration(
        &self,
        id: &str,
        intent: &str,
        context: &str,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = self.verified_transaction(&mut db, rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        if run.state != RunState::Verified {
            return Err(RuntimeError::Verification);
        }
        if tx
            .prepare("SELECT 1 FROM events WHERE run_id=?1 AND kind LIKE 'integration.%'")?
            .exists([id])?
        {
            return Err(RuntimeError::Integrity("integration already attempted"));
        }
        append(&tx, &run, "integration.context.v1", context)?;
        append(&tx, &run, "integration.intent", intent)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn claim_finite_resume(
        &self,
        id: &str,
        history: &str,
        approval: &str,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = self.verified_transaction(&mut db, rusqlite::TransactionBehavior::Immediate)?;
        let run = load_opt(&tx, id)?.ok_or(RuntimeError::NotFound)?;
        let tip: String = tx.query_row(
            "SELECT event_hash FROM events ORDER BY sequence DESC LIMIT 1",
            [],
            |r| r.get(0),
        )?;
        if run.state != RunState::Verified || tip != history {
            return Err(RuntimeError::Approval);
        }
        let kinds = tx.prepare("SELECT kind FROM events WHERE run_id=?1 AND kind LIKE 'integration.%' ORDER BY sequence LIMIT 4")?
            .query_map([id], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        if kinds != ["integration.context.v1", "integration.intent"] {
            return Err(RuntimeError::Integrity(
                "resume already attempted or unsupported",
            ));
        }
        append(&tx, &run, "integration.resume_intent.v1", approval)?;
        tx.commit()?;
        Ok(())
    }
}
