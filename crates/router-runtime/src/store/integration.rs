use super::*;
use router_adapter_api::is_digest;
use router_types::{
    StagedIntegrationPreview, StagedIntegrationResult, VerificationOutcome, VerificationRecord,
};

fn verified_record(db: &Connection, expected: &RunSummary) -> Result<VerificationRecord> {
    let run = load_opt(db, &expected.run_id)?.ok_or(RuntimeError::NotFound)?;
    if run.state != RunState::Verified
        || run.definition_hash != expected.definition_hash
        || run.approval_digest != expected.approval_digest
    {
        return Err(RuntimeError::Verification);
    }
    let text: String = db
        .query_row(
            "SELECT record FROM verifications WHERE run_id=?1",
            [&run.run_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(RuntimeError::Verification)?;
    let record: VerificationRecord = serde_json::from_str(&text)?;
    if record.validation != VerificationOutcome::Passed || record.objective != VerificationOutcome::Passed
        || record.artifact_sha256.is_none() || record.artifact_sha256 != run.artifact_sha256
        || !is_digest(&record.evidence_sha256) || record.verifier.is_empty() || record.verifier.len() > 128
        || !db.prepare("SELECT 1 FROM events WHERE run_id=?1 AND kind='verification.recorded' AND detail=?2")?
            .exists(params![run.run_id, record.evidence_sha256])? {
        return Err(RuntimeError::Verification);
    }
    Ok(record)
}

impl Store {
    pub(crate) fn staged_verification(&self, expected: &RunSummary) -> Result<VerificationRecord> {
        let mut db = self.lock()?;
        let tx = self.verified_transaction(&mut db, rusqlite::TransactionBehavior::Deferred)?;
        verified_record(&tx, expected)
    }

    /// The claim survives every later failure. No automatic retry can decide
    /// whether a publication happened before an interrupted response.
    pub(crate) fn claim_staged(
        &self,
        expected: &RunSummary,
        preview: &StagedIntegrationPreview,
    ) -> Result<()> {
        let mut db = self.lock()?;
        let tx = self.verified_transaction(&mut db, rusqlite::TransactionBehavior::Immediate)?;
        if verified_record(&tx, expected)? != preview.verification {
            return Err(RuntimeError::Verification);
        }
        if tx
            .prepare("SELECT 1 FROM artifacts WHERE run_id=?1 AND kind='staged-integration-claim'")?
            .exists([&expected.run_id])?
            || tx
                .prepare(
                    "SELECT 1 FROM events WHERE run_id=?1 AND kind='integration.staged.claimed'",
                )?
                .exists([&expected.run_id])?
        {
            return Err(RuntimeError::Integrity(
                "staged integration already claimed",
            ));
        }
        tx.execute(
            "INSERT INTO artifacts VALUES(?1,?2,'staged-integration-claim')",
            params![expected.run_id, preview.approval_digest],
        )?;
        let run = load_opt(&tx, &expected.run_id)?.ok_or(RuntimeError::NotFound)?;
        append(
            &tx,
            &run,
            "integration.staged.claimed",
            &preview.approval_digest,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn complete_staged(&self, result: &StagedIntegrationResult) -> Result<()> {
        let mut db = self.lock()?;
        let tx = self.verified_transaction(&mut db, rusqlite::TransactionBehavior::Immediate)?;
        let claimed: Option<String> = tx
            .query_row(
                "SELECT sha256 FROM artifacts WHERE run_id=?1 AND kind='staged-integration-claim'",
                [&result.run_id],
                |r| r.get(0),
            )
            .optional()?;
        if claimed.as_deref() != Some(&result.approval_digest)
            || !tx.prepare("SELECT 1 FROM events WHERE run_id=?1 AND kind='integration.staged.claimed' AND detail=?2")?.exists(params![result.run_id, result.approval_digest])? {
            return Err(RuntimeError::Integrity("staged integration claim changed"));
        }
        let run = load_opt(&tx, &result.run_id)?.ok_or(RuntimeError::NotFound)?;
        tx.execute(
            "INSERT INTO artifacts VALUES(?1,?2,'staged-integration-result')",
            params![result.run_id, digest("runtime-staged-result-v1", result)?],
        )?;
        append(
            &tx,
            &run,
            "integration.staged.completed",
            &serde_json::to_string(result)?,
        )?;
        tx.commit()?;
        Ok(())
    }
}
