use crate::{Result, WorkflowError, safe_path, text};
use router_types::{
    Clarification, Mutation, NormalizedTask, TaskInput, TaskKind, TaskNormalization,
};

/// Deterministic normalization of explicit task facts. This does not infer
/// permission, budget, sensitivity or risk from natural-language wording.
pub fn normalize(mut input: TaskInput) -> Result<TaskNormalization> {
    if !text(&input.objective) {
        return Err(WorkflowError::Invalid("objective"));
    }
    for list in [
        &input.deliverables,
        &input.constraints,
        &input.scope,
        &input.dependencies,
        &input.acceptance,
    ] {
        if list.len() > 64 {
            return Err(WorkflowError::Limit("task fields"));
        }
        if list.iter().any(|value| !text(value)) {
            return Err(WorkflowError::Invalid("task field"));
        }
    }
    if input.scope.iter().any(|path| !safe_path(path)) {
        return Err(WorkflowError::Invalid(
            "scope must use relative paths without globs",
        ));
    }
    input.objective = input.objective.trim().to_owned();
    input.scope.sort();
    input.scope.dedup();
    input.dependencies.sort();
    input.dependencies.dedup();
    let mut questions = Vec::new();
    let mut ask = |code: &str, question: &str| {
        questions.push(Clarification {
            code: code.into(),
            question: question.into(),
        })
    };
    if input.risk.is_none() {
        ask("task.risk.unknown", "What risk level applies to this task?");
    }
    if input.sensitivity.is_none() {
        ask(
            "task.sensitivity.unknown",
            "Does the task contain public, internal or secret data?",
        );
    }
    if input.budget.is_none() {
        ask(
            "task.budget.unknown",
            "What cost, token and elapsed-time limits are authorized?",
        );
    }
    if input.allowed_mutations.is_none() {
        ask(
            "task.mutations.unknown",
            "Which mutations are authorized? An explicit empty set permits none.",
        );
    }
    if input.acceptance.is_empty() {
        ask(
            "task.acceptance.missing",
            "Which observable criteria establish success?",
        );
    }
    if input.scope.is_empty() {
        ask(
            "task.scope.missing",
            "Which relative workspace files or directories are in scope?",
        );
    }
    if matches!(input.kind, TaskKind::Implement | TaskKind::Migration)
        && input
            .allowed_mutations
            .as_ref()
            .is_some_and(|set| !set.contains(&Mutation::WorkspaceWrite))
    {
        ask(
            "task.write.not_authorized",
            "The task requires edits, but workspace writes are not authorized. Should the task become a read-only proposal or receive explicit write authority?",
        );
    }
    let mut assumptions = vec![];
    if input.deliverables.is_empty() {
        input.deliverables.push(input.objective.clone());
        assumptions.push(
            "The objective is the deliverable because no separate deliverables were supplied."
                .into(),
        );
    }
    if !questions.is_empty() {
        return Ok(TaskNormalization {
            schema_version: 1,
            task: None,
            questions,
            assumptions,
        });
    }
    let risk = input.risk.ok_or(WorkflowError::Invalid("risk"))?;
    let sensitivity = input
        .sensitivity
        .ok_or(WorkflowError::Invalid("sensitivity"))?;
    let budget = input.budget.ok_or(WorkflowError::Invalid("budget"))?;
    if budget.max_tokens == 0 || budget.max_elapsed_ms == 0 {
        return Err(WorkflowError::Invalid("zero token or time budget"));
    }
    Ok(TaskNormalization {
        schema_version: 1,
        task: Some(NormalizedTask {
            objective: input.objective,
            kind: input.kind,
            deliverables: input.deliverables,
            constraints: input.constraints,
            scope: input.scope,
            risk,
            sensitivity,
            budget,
            allowed_mutations: input
                .allowed_mutations
                .ok_or(WorkflowError::Invalid("mutations"))?,
            dependencies: input.dependencies,
            acceptance: input.acceptance,
            required_capabilities: input.required_capabilities,
        }),
        questions,
        assumptions,
    })
}

pub fn validate_task(task: &NormalizedTask) -> Result<()> {
    let result = normalize(TaskInput {
        objective: task.objective.clone(),
        kind: task.kind,
        deliverables: task.deliverables.clone(),
        constraints: task.constraints.clone(),
        scope: task.scope.clone(),
        risk: Some(task.risk),
        sensitivity: Some(task.sensitivity),
        budget: Some(task.budget.clone()),
        allowed_mutations: Some(task.allowed_mutations.clone()),
        dependencies: task.dependencies.clone(),
        acceptance: task.acceptance.clone(),
        required_capabilities: task.required_capabilities.clone(),
    })?;
    if result.task.as_ref() != Some(task) {
        return Err(WorkflowError::Invalid(
            "task is not normalized or authority is missing",
        ));
    }
    Ok(())
}
