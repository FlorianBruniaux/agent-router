// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Explicit task intake. Answers are task facts, never execution authority.
use crate::{CliError, Outcome, product::json_output};
use clap::Args;
use router_types::{
    Clarification, RiskEnvironment, RiskReversibility, TaskInput, TaskRiskContext,
    TaskRiskNormalization,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

const MAX_TEXT: usize = 16_384;
const MAX_ANSWER: usize = 64 * 1024;
const MAX_DOCUMENT: usize = 1024 * 1024;
const MAX_TURNS: usize = 32;

#[derive(Args)]
pub(crate) struct GuideArgs {
    /// Objective only. No permissions, budget or task kind are inferred.
    #[arg(long, required_unless_present_any = ["text_file", "task"], conflicts_with_all = ["text_file", "task"])]
    text: Option<String>,
    #[arg(long, conflicts_with = "task")]
    text_file: Option<PathBuf>,
    /// Existing strict TaskInput JSON; explicit fields are preserved.
    #[arg(long)]
    task: Option<PathBuf>,
    #[arg(long)]
    risk_context: Option<PathBuf>,
    /// Opt in to bounded questions on stderr and answers from stdin.
    #[arg(long)]
    interactive: bool,
    /// Export complete facts into a new directory; never replace an existing path.
    #[arg(long)]
    output_directory: Option<PathBuf>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    objective: String,
    input: Option<TaskInput>,
    risk_context: Option<TaskRiskContext>,
    questions: Vec<Clarification>,
    result: Option<TaskRiskNormalization>,
    output_directory: Option<PathBuf>,
}

fn error(code: &'static str) -> CliError {
    CliError::Product(code)
}
fn strict<T: DeserializeOwned>(text: &[u8]) -> Result<T, CliError> {
    let value =
        router_adapter_api::strict_json(text).map_err(|_| error("task_guide.answer.invalid"))?;
    serde_json::from_value(value).map_err(|_| error("task_guide.answer.invalid"))
}
fn choice<T: DeserializeOwned>(text: &str) -> Result<T, CliError> {
    serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|_| error("task_guide.answer.invalid"))
}
fn empty_input(objective: String, kind: router_types::TaskKind) -> TaskInput {
    TaskInput {
        objective,
        kind,
        deliverables: vec![],
        constraints: vec![],
        scope: vec![],
        risk: None,
        sensitivity: None,
        budget: None,
        allowed_mutations: None,
        dependencies: vec![],
        acceptance: vec![],
        required_capabilities: Default::default(),
    }
}

impl Report {
    fn normalize(&mut self) -> Result<(), CliError> {
        if let Some(input) = &self.input {
            let result =
                router_workflow::normalize_with_risk(input.clone(), self.risk_context.as_ref())
                    .map_err(crate::product::workflow_error)?;
            self.questions = result.normalization.questions.clone();
            if let Some(task) = &result.normalization.task {
                router_workflow::validate_task(task).map_err(crate::product::workflow_error)?;
                self.status = "complete";
            }
            self.result = Some(result);
        } else {
            self.questions = vec![Clarification {
                code: "task.kind.unknown".into(),
                question: "Which kind of task is intended?".into(),
            }];
        }
        Ok(())
    }
    fn context(&mut self) -> &mut TaskRiskContext {
        self.risk_context.get_or_insert_with(|| TaskRiskContext {
            schema_version: 1,
            environment: RiskEnvironment::Unknown,
            reversibility: RiskReversibility::Unknown,
            operations: Default::default(),
        })
    }
    fn answer(&mut self, code: &str, answer: &str) -> Result<(), CliError> {
        match code {
            "task.kind.unknown" => {
                self.input = Some(empty_input(self.objective.clone(), choice(answer)?));
            }
            "risk.environment.unknown" => self.context().environment = choice(answer)?,
            "risk.reversibility.unknown" => self.context().reversibility = choice(answer)?,
            "risk.operations.unknown" => self.context().operations = strict(answer.as_bytes())?,
            _ => {
                let input = self
                    .input
                    .as_mut()
                    .ok_or_else(|| error("task_guide.state.invalid"))?;
                match code {
                    "task.risk.unknown" => input.risk = Some(choice(answer)?),
                    "task.sensitivity.unknown" => input.sensitivity = Some(choice(answer)?),
                    "task.budget.unknown" => {
                        let budget: router_types::TaskBudget = strict(answer.as_bytes())?;
                        if budget.max_elapsed_ms == 0 || budget.max_tokens == 0 {
                            return Err(error("task_guide.answer.invalid"));
                        }
                        input.budget = Some(budget);
                    }
                    "task.mutations.unknown" => {
                        input.allowed_mutations = Some(strict(answer.as_bytes())?)
                    }
                    "task.acceptance.missing" => input.acceptance = strict(answer.as_bytes())?,
                    "task.scope.missing" => input.scope = strict(answer.as_bytes())?,
                    _ => return Err(error("task_guide.question.unsupported")),
                }
            }
        }
        self.normalize()
            .map_err(|_| error("task_guide.answer.invalid"))
    }
}

/// None marks a conflict which requires a revised task, not an authority prompt.
fn syntax(code: &str) -> Result<Option<&'static str>, CliError> {
    Ok(Some(match code {
        "task.kind.unknown" => "explore | implement | review | architecture | security | migration",
        "task.risk.unknown" => "low | medium | high | critical",
        "task.sensitivity.unknown" => "public | internal | secret",
        "task.budget.unknown" => {
            "JSON object: {\"max_cost_microusd\":0,\"max_elapsed_ms\":60000,\"max_tokens\":1000}. Supply your own authorized limits; no default."
        }
        "task.mutations.unknown" => {
            "JSON array of workspace_write, network, execute_skills, integrate; [] permits none."
        }
        "task.acceptance.missing" => {
            "JSON array of observable success criteria, e.g. [\"Report identifies the affected files\"]."
        }
        "task.scope.missing" => {
            "JSON array of relative workspace paths; no absolute paths, traversal or globs."
        }
        "risk.environment.unknown" => "local | test | staging | production",
        "risk.reversibility.unknown" => "reversible | irreversible",
        "risk.operations.unknown" => {
            "JSON array: read_only, workspace_change, security_change, credential_handling, data_migration, data_deletion, deployment, financial_transfer, external_publication."
        }
        "task.write.not_authorized"
        | "risk.write.not_authorized"
        | "risk.network.not_authorized"
        | "risk.context.conflict" => return Ok(None),
        _ => return Err(error("task_guide.question.unsupported")),
    }))
}

fn dialogue<R: BufRead, W: Write>(
    report: &mut Report,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), CliError> {
    let mut total = 0;
    for _ in 0..MAX_TURNS {
        if report.status == "complete" {
            return Ok(());
        }
        // Stop on an explicit conflict before asking for any other missing fact.
        for question in &report.questions {
            if syntax(&question.code)?.is_none() {
                return Ok(());
            }
        }
        let question = report
            .questions
            .first()
            .ok_or_else(|| error("task_guide.state.invalid"))?;
        let code = question.code.clone();
        let format = syntax(&code)?.ok_or_else(|| error("task_guide.state.invalid"))?;
        // Both strings come from fixed application questions, never user input.
        writeln!(
            writer,
            "{}\n{}\nEnter unknown to leave unresolved, or cancel to stop.\n> ",
            question.question, format
        )
        .and_then(|()| writer.flush())
        .map_err(|_| error("task_guide.io"))?;
        let mut bytes = Vec::new();
        reader
            .by_ref()
            .take((MAX_ANSWER + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| error("task_guide.io"))?;
        total += bytes.len();
        if bytes.len() > MAX_ANSWER || total > MAX_DOCUMENT {
            return Err(error("task_guide.dialogue.limit"));
        }
        let answer = std::str::from_utf8(&bytes)
            .map_err(|_| error("task_guide.answer.invalid"))?
            .trim();
        if answer.is_empty() || answer == "unknown" {
            return Ok(());
        }
        if answer == "cancel" {
            report.status = "cancelled";
            return Ok(());
        }
        report.answer(&code, answer)?;
    }
    if report.status == "complete" {
        Ok(())
    } else {
        Err(error("task_guide.dialogue.limit"))
    }
}

pub(crate) fn run(args: GuideArgs, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let input: Option<TaskInput> = args
        .task
        .as_deref()
        .map(|path| crate::phases::read_json(path, MAX_DOCUMENT))
        .transpose()?;
    let objective = if let Some(input) = &input {
        input.objective.clone()
    } else if let Some(text) = args.text {
        text
    } else if let Some(path) = &args.text_file {
        String::from_utf8(crate::read_bounded(
            path,
            MAX_TEXT,
            CliError::RequestRead,
            CliError::RequestTooLarge,
        )?)
        .map_err(|_| error("task_guide.text.invalid"))?
    } else {
        return Err(CliError::InvalidCommand);
    };
    if objective.trim().is_empty() || objective.len() > MAX_TEXT || objective.contains('\0') {
        return Err(error("task_guide.text.invalid"));
    }
    let risk_context: Option<TaskRiskContext> = args
        .risk_context
        .as_deref()
        .map(|path| crate::phases::read_json(path, MAX_ANSWER))
        .transpose()?;
    if risk_context
        .as_ref()
        .is_some_and(|context| context.schema_version != 1)
    {
        return Err(error("workflow.invalid"));
    }
    let mut report = Report {
        schema_version: 1,
        status: "needs_clarification",
        objective,
        input,
        risk_context,
        questions: vec![],
        result: None,
        output_directory: None,
    };
    report.normalize()?;
    if args.interactive {
        dialogue(
            &mut report,
            &mut io::stdin().lock(),
            &mut io::stderr().lock(),
        )?;
    }
    if report.status == "complete" {
        if let Some(path) = args.output_directory {
            report.output_directory = Some(publish(&path, &report)?);
        }
    }
    let exit = if report.status == "complete" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    };
    json_output(&report, exit)
}

/// Publishes the complete facts; `normalization.json` is the last marker.
#[cfg(unix)]
fn publish(path: &Path, report: &Report) -> Result<PathBuf, CliError> {
    let fail = || error("task_guide.output");
    let input = report.input.as_ref().ok_or_else(fail)?;
    let result = report.result.as_ref().ok_or_else(fail)?;
    let mut documents = vec![(
        "input.json",
        serde_json::to_vec_pretty(input).map_err(|_| fail())?,
    )];
    if let Some(context) = &report.risk_context {
        documents.push((
            "risk-context.json",
            serde_json::to_vec_pretty(context).map_err(|_| fail())?,
        ));
    }
    documents.push((
        "normalization.json",
        serde_json::to_vec_pretty(result).map_err(|_| fail())?,
    ));
    crate::private_output::publish(path, documents, "task_guide.output")
}

#[cfg(not(unix))]
fn publish(_: &Path, _: &Report) -> Result<PathBuf, CliError> {
    Err(error("task_guide.output.unsupported"))
}
