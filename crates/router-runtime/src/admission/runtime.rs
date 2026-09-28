use super::*;
use crate::{Cancellation, PendingRun, RuntimeLimits, store::Store};

pub(crate) struct SharedAdmission {
    registry: AdmissionStore,
    binding: AdmissionBinding,
    _owner: files::Owner,
}
impl SharedAdmission {
    pub fn open(config: &AdmissionConfig, root: &Path) -> Result<Self> {
        if config.schema_version != 1 {
            return Err(RuntimeError::Invalid("admission configuration"));
        }
        let registry = AdmissionStore::open(&config.registry, &config.expected_policy_sha256)?;
        let binding = registry.binding(&config.runtime_id)?;
        if registry.policy().runtimes[&config.runtime_id].root_sha256
            != admission_runtime_root_hash(root)?
        {
            return Err(RuntimeError::Approval);
        }
        let owner = registry.owner(&config.runtime_id)?;
        Ok(Self {
            registry,
            binding,
            _owner: owner,
        })
    }
    pub fn binding(&self) -> &AdmissionBinding {
        &self.binding
    }
    pub fn validate(&self, pending: &PendingRun) -> Result<(String, String)> {
        self.validate_request(&pending.context, &pending.request, &pending.node.owner)
    }
    pub fn validate_request(
        &self,
        context: &router_policy::ValidatedPhaseContext,
        request: &ExecutionRequest,
        owner: &str,
    ) -> Result<(String, String)> {
        let provider = context
            .asset_requirements(&request.candidate_id)
            .ok_or(RuntimeError::Invalid("admission unknown provider"))?
            .provider
            .clone();
        if !self.registry.policy().providers.contains_key(&provider) {
            return Err(RuntimeError::Invalid("admission unknown provider"));
        }
        let candidate_agent = request.agent.as_deref();
        let mapping = self
            .registry
            .policy()
            .agent_mappings
            .iter()
            .find(|m| {
                m.runtime_id == self.binding.runtime_id
                    && m.plan_owner == owner
                    && m.candidate_agent.as_deref() == candidate_agent
            })
            .ok_or(RuntimeError::Invalid("admission unknown logical agent"))?;
        Ok((provider, mapping.quota_id.clone()))
    }
    pub fn start(
        &self,
        store: &Store,
        pending: &PendingRun,
        limits: &RuntimeLimits,
    ) -> Result<Ticket> {
        let (provider, agent) = self.validate(pending)?;
        let group = group_key(
            &self.binding.runtime_id,
            pending.run.workflow.as_ref(),
            &pending.run.run_id,
        )?;
        let t = Ticket {
            id: String::new(),
            binding: self.binding.clone(),
            run_id: pending.run.run_id.clone(),
            definition_hash: pending.run.definition_hash.clone(),
            approval_digest: pending.run.approval_digest.clone(),
            workflow: pending.run.workflow.clone(),
            admission_sequence: 0,
            cycle: 0,
            attempt: 0,
            group,
            provider,
            agent,
            runtime_limit: limits.global_concurrency as u32,
            provider_limit: limits.provider_concurrency as u32,
            agent_limit: limits.agent_concurrency as u32,
            workflow_limit: pending.plan.max_width.min(
                pending
                    .context
                    .request()
                    .restrictions
                    .max_concurrency
                    .unwrap_or(32),
            ) as u32,
        };
        store.start_admission(t)
    }
    pub async fn wait(&self, store: &Store, ticket: &Ticket, cancel: &Cancellation) -> Result<()> {
        let _waiter = self.registry.waiter(ticket)?;
        self.registry.enqueue(ticket)?;
        let mut previous = Vec::new();
        loop {
            if cancel.is_cancelled() || store.cancel_requested(&ticket.run_id)? {
                return Err(RuntimeError::Cancelled);
            }
            let reasons = self.registry.poll(ticket)?;
            if reasons.is_empty() {
                return Ok(());
            }
            if reasons != previous {
                store.event(
                    &ticket.run_id,
                    "admission.queued",
                    &serde_json::to_string(&reasons)?,
                )?;
                previous = reasons;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    pub fn release(&self, store: &Store, ticket: &Ticket) -> Result<()> {
        let proof = store.admission_release_proof(ticket)?;
        // Model cleanup flock survives both journal commits. No Drop release.
        self.registry.release(&proof)?;
        store.admission_released(ticket)
    }
    pub fn recover(&self, store: &Store) -> Result<()> {
        for ticket in store.admission_tickets()? {
            if ticket.binding != self.binding {
                return Err(RuntimeError::Approval);
            }
            match self.release(store, &ticket) {
                Ok(()) => {}
                Err(RuntimeError::CleanupUnknown) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

fn group_key(
    runtime_id: &str,
    workflow: Option<&WorkflowRunBinding>,
    run_id: &str,
) -> Result<String> {
    match workflow {
        Some(w) => digest(
            "admission-workflow-v1",
            &(&w.instance_id, &w.contract_sha256),
        ),
        None => digest("admission-standalone-v1", &(runtime_id, run_id)),
    }
}
#[cfg(test)]
mod group_tests {
    use super::*;
    #[test]
    fn workflow_capacity_is_shared_across_runtime_roots_and_generations() {
        let mut workflow = WorkflowRunBinding {
            instance_id: "same-instance".into(),
            contract_sha256: "a".repeat(64),
            generation: 0,
        };
        let one = group_key("one", Some(&workflow), "run-a").unwrap();
        workflow.generation = 1;
        assert_eq!(one, group_key("two", Some(&workflow), "run-b").unwrap());
        assert_ne!(
            group_key("one", None, "run").unwrap(),
            group_key("two", None, "run").unwrap()
        );
    }
}
