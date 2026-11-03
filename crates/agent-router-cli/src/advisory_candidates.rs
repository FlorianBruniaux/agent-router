// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Skill candidates declared by an external router (ADR 0055). The document is
//! operator input: parsed with the existing candidate document v1 rules, bounded
//! further here, and never granted authority. A candidate outside the catalog is
//! only reported.
use crate::{CliError, read_bounded};
use router_catalog::Catalog;
use router_policy::{
    MAX_CANDIDATE_DOCUMENT_BYTES, MAX_STAGE_SKILLS, OPERATOR_DECLARED_SOURCE, SKILL_FLOOR_MICROS,
    parse_candidate_document,
};
use router_types::{SkillCandidate, SkillId};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{cmp::Reverse, collections::BTreeSet, path::Path};

/// Request limit (`RouteRequest` accepts at most 256 candidates).
const MAX_CANDIDATES: usize = 256;
const MAX_ID_BYTES: usize = 96;
const MAX_LABEL_BYTES: usize = 64;
const SOURCE_PREFIX: &str = "external-declared:";

/// One candidate of the document, after validation.
pub(crate) struct FileCandidate {
    pub(crate) skill: SkillId,
    pub(crate) score_micros: i64,
    /// The operator named the same id with `--skill`: the operator row is used.
    pub(crate) operator_wins: bool,
}

pub(crate) struct Declared {
    pub(crate) label: String,
    pub(crate) sha256: String,
    /// Sorted by skill id, whatever the order in the document.
    pub(crate) candidates: Vec<FileCandidate>,
}

fn error(code: &'static str) -> CliError {
    CliError::Product(code)
}

/// The documented code of a parser refusal, kept as is.
fn parser_code(code: &str) -> &'static str {
    match code {
        "retrieval.document.too_large" => "retrieval.document.too_large",
        "retrieval.json.invalid" => "retrieval.json.invalid",
        "retrieval.schema.unsupported" => "retrieval.schema.unsupported",
        "retrieval.source.empty" => "retrieval.source.empty",
        "retrieval.source.too_long" => "retrieval.source.too_long",
        "retrieval.candidates.too_many" => "retrieval.candidates.too_many",
        "retrieval.candidate.invalid_skill" => "retrieval.candidate.invalid_skill",
        "retrieval.candidate.duplicate_skill" => "retrieval.candidate.duplicate_skill",
        "retrieval.candidate.score_out_of_range" => "retrieval.candidate.score_out_of_range",
        _ => "retrieval.document.invalid",
    }
}

/// Same path-free identifier rule as the advisory outcome record.
fn valid_id(value: &str) -> bool {
    value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LABEL_BYTES
        && value != OPERATOR_DECLARED_SOURCE
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

/// Reads and validates the document. Nothing is truncated: any bound exceeded
/// is a refusal.
pub(crate) fn load(path: &Path, operator: &BTreeSet<SkillId>) -> Result<Declared, CliError> {
    let bytes = read_bounded(
        path,
        MAX_CANDIDATE_DOCUMENT_BYTES,
        CliError::RequestRead,
        error("retrieval.document.too_large"),
    )?;
    let parsed =
        parse_candidate_document(&bytes).map_err(|failure| error(parser_code(failure.code())))?;
    // The parser puts the label on each candidate; an empty list has none, so
    // the label is read from the document itself.
    let label = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|document| document.get("source")?.as_str().map(str::to_owned))
        .ok_or_else(|| error("retrieval.json.invalid"))?;
    if !valid_label(&label) {
        return Err(error("advisory.skill_candidates.source_invalid"));
    }
    if parsed.len() > MAX_CANDIDATES {
        return Err(error("advisory.skill_candidates.too_many"));
    }
    if parsed.iter().any(|row| !valid_id(row.skill.as_str())) {
        return Err(error("advisory.skill_candidate.invalid_id"));
    }
    let mut candidates: Vec<FileCandidate> = parsed
        .into_iter()
        .map(|row| FileCandidate {
            operator_wins: operator.contains(&row.skill),
            skill: row.skill,
            score_micros: row.score_micros,
        })
        .collect();
    candidates.sort_by(|left, right| left.skill.cmp(&right.skill));
    let kept = candidates.iter().filter(|row| !row.operator_wins).count();
    if kept + operator.len() > MAX_CANDIDATES {
        return Err(error("advisory.skill_candidates.too_many"));
    }
    Ok(Declared {
        label,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        candidates,
    })
}

impl Declared {
    /// Request source of every file candidate. The prefix is reserved: a
    /// document cannot produce `operator-declared`.
    pub(crate) fn source(&self) -> String {
        format!("{SOURCE_PREFIX}{}", self.label)
    }

    /// File rows that enter the request: an operator `--skill` of the same id
    /// replaces its file row.
    pub(crate) fn request_candidates(&self) -> impl Iterator<Item = SkillCandidate> + '_ {
        self.candidates
            .iter()
            .filter(|row| !row.operator_wins)
            .map(|row| SkillCandidate {
                skill: row.skill.clone(),
                source: self.source(),
                score_micros: row.score_micros,
            })
    }

    /// The two best candidates the catalog does not know, above the floor.
    /// They are advice to a human; none of them can attach.
    pub(crate) fn outside_suggested(&self, catalog: &Catalog) -> Vec<SkillId> {
        let mut outside: Vec<_> = self
            .candidates
            .iter()
            .filter(|row| {
                !row.operator_wins
                    && catalog.skill(&row.skill).is_none()
                    && row.score_micros >= SKILL_FLOOR_MICROS
            })
            .collect();
        // Rows are sorted by id and the sort is stable: id breaks score ties.
        outside.sort_by_key(|row| Reverse(row.score_micros));
        outside
            .into_iter()
            .take(MAX_STAGE_SKILLS)
            .map(|row| row.skill.clone())
            .collect()
    }
}
