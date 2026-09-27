use crate::*;
use router_types::{SkillCandidate, SkillId, SkillTrust};
use std::collections::{BTreeMap, BTreeSet};

type TermKey = (AssetKind, bool, Field, String);
type GroupKey = (AssetKind, bool, Field);

#[derive(Debug)]
struct Posting {
    asset: usize,
    frequency: u32,
}

#[derive(Debug, Default)]
struct Statistics {
    documents: u32,
    total_length: u64,
}

#[derive(Debug)]
struct IndexedAsset {
    metadata: AssetMetadata,
    lengths: BTreeMap<Field, u32>,
}

/// Rebuildable metadata index. No corpus body is retained or read while building.
#[derive(Debug)]
pub struct Bm25Index {
    assets: Vec<IndexedAsset>,
    postings: BTreeMap<TermKey, Vec<Posting>>,
    stats: BTreeMap<GroupKey, Statistics>,
    anti_triggers: BTreeMap<String, BTreeSet<usize>>,
    source_hash: String,
    index_hash: String,
    catalog_version: String,
}

impl Bm25Index {
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        Self::build(parse(bytes)?)
    }

    pub fn build(document: MetadataDocument) -> Result<Self> {
        let document = canonical(document)?;
        let source_hash = hash_document(&document)?;
        let index_hash = digest(format!("{INDEX_VERSION}\0{source_hash}\0k1=1.2;b=0.75;positive-idf;boosts=8000,6000,5000,3000,2500,2000,2000").as_bytes());
        let mut index = Self {
            assets: Vec::new(),
            postings: BTreeMap::new(),
            stats: BTreeMap::new(),
            anti_triggers: BTreeMap::new(),
            source_hash,
            index_hash,
            catalog_version: document.catalog_version,
        };
        let mut total_terms = 0usize;
        for metadata in document.assets {
            let asset_id = index.assets.len();
            let mut lengths = BTreeMap::new();
            if metadata.trust != SkillTrust::Blocked {
                let group = (metadata.kind, trusted(metadata.trust));
                for (field, text) in fields(&metadata)? {
                    let terms = tokenize::terms(&text);
                    let length: u32 = terms.values().sum();
                    total_terms += length as usize;
                    if total_terms > MAX_INDEX_TERMS {
                        return Err(RetrievalError::Limit("index terms"));
                    }
                    lengths.insert(field, length);
                    let stat = index.stats.entry((group.0, group.1, field)).or_default();
                    stat.documents += 1;
                    stat.total_length += u64::from(length);
                    for (term, frequency) in terms {
                        index
                            .postings
                            .entry((group.0, group.1, field, term))
                            .or_default()
                            .push(Posting {
                                asset: asset_id,
                                frequency,
                            });
                    }
                }
                for trigger in &metadata.anti_triggers {
                    let phrase = tokenize::phrase(trigger);
                    if !phrase.is_empty() {
                        index
                            .anti_triggers
                            .entry(phrase)
                            .or_default()
                            .insert(asset_id);
                    }
                }
            }
            index.assets.push(IndexedAsset { metadata, lengths });
        }
        Ok(index)
    }

    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }
    pub fn index_hash(&self) -> &str {
        &self.index_hash
    }
    pub fn catalog_version(&self) -> &str {
        &self.catalog_version
    }
    pub fn len(&self) -> usize {
        self.assets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Canonical metadata order/whitespace does not invalidate an index; changed
    /// metadata, catalog version, trust, body references or hashes do.
    pub fn is_current(&self, source: &[u8]) -> Result<bool> {
        Ok(hash_document(&canonical(parse(source)?)?)? == self.source_hash)
    }

    pub fn search_current(&self, source: &[u8], query: &SearchQuery) -> Result<SearchResult> {
        if !self.is_current(source)? {
            return Err(RetrievalError::Stale);
        }
        self.search(query)
    }

    pub(crate) fn asset(&self, kind: AssetKind, id: &str) -> Option<&AssetMetadata> {
        self.assets
            .binary_search_by(|a| (a.metadata.kind, a.metadata.id.as_str()).cmp(&(kind, id)))
            .ok()
            .map(|index| &self.assets[index].metadata)
    }

    pub fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        validate_query(query)?;
        let terms = tokenize::terms(&query.text);
        if terms.len() > MAX_QUERY_TERMS {
            return Err(RetrievalError::Limit("query terms"));
        }
        let mut result = SearchResult {
            schema_version: "1".into(),
            catalog_version: self.catalog_version.clone(),
            source_hash: self.source_hash.clone(),
            index_hash: self.index_hash.clone(),
            policy: query.policy,
            candidates: Vec::new(),
            selected: None,
            abstention: None,
            exclusions: Vec::new(),
            excluded_count: 0,
            postings_visited: 0,
        };
        if terms.is_empty() {
            result.abstention = Some(Abstention::EmptyQuery);
            return Ok(result);
        }
        let anti = self.excluded_by_trigger(&query.text)?;
        let required_phrases = tokenize::quoted_phrases(&query.text);
        let mut excluded = BTreeSet::new();
        for (i, asset) in self.assets.iter().enumerate() {
            if query.kind.is_some_and(|kind| kind != asset.metadata.kind) {
                continue;
            }
            let reason = if asset.metadata.trust == SkillTrust::Blocked {
                Some(ExclusionReason::Blocked)
            } else if anti.contains(&i) {
                Some(ExclusionReason::AntiTrigger)
            } else {
                None
            };
            if let Some(reason) = reason {
                excluded.insert(i);
                result.excluded_count += 1;
                if result.exclusions.len() < MAX_RESULTS {
                    result.exclusions.push(Exclusion {
                        asset: asset.metadata.key(),
                        reason,
                    });
                }
            }
        }
        let mut matches: BTreeMap<usize, Vec<MatchEvidence>> = BTreeMap::new();
        for (&(kind, is_trusted, field), stat) in &self.stats {
            if query.kind.is_some_and(|requested| requested != kind) {
                continue;
            }
            if stat.total_length == 0 {
                continue;
            }
            for term in terms.keys() {
                let Some(postings) = self.postings.get(&(kind, is_trusted, field, term.clone()))
                else {
                    continue;
                };
                result.postings_visited += postings.len();
                if result.postings_visited > MAX_QUERY_POSTINGS {
                    return Err(RetrievalError::Limit("query postings"));
                }
                let df = postings.len() as u32;
                let idf =
                    (1.0 + (f64::from(stat.documents - df) + 0.5) / (f64::from(df) + 0.5)).ln();
                let avg_length = stat.total_length as f64 / f64::from(stat.documents);
                for posting in postings {
                    if excluded.contains(&posting.asset) {
                        continue;
                    }
                    let length = self.assets[posting.asset].lengths[&field];
                    let tf = f64::from(posting.frequency);
                    let score = idf * tf * 2.2
                        / (tf + 1.2 * (0.25 + 0.75 * f64::from(length) / avg_length));
                    let contribution =
                        (score * f64::from(field.boost_millis()) * 1000.0).round() as u64;
                    matches
                        .entry(posting.asset)
                        .or_default()
                        .push(MatchEvidence {
                            field,
                            term: term.clone(),
                            term_frequency: posting.frequency,
                            document_frequency: df,
                            field_length: length,
                            boost_millis: field.boost_millis(),
                            contribution_micros: contribution,
                        });
                }
            }
        }
        for (i, evidence) in matches {
            let asset = &self.assets[i].metadata;
            let mut phrase_evidence = Vec::new();
            if !required_phrases.is_empty() {
                let texts = fields(asset)?;
                for phrase in &required_phrases {
                    for (field, text) in &texts {
                        if format!(" {} ", tokenize::phrase(text)).contains(&format!(" {phrase} "))
                        {
                            phrase_evidence.push(PhraseEvidence {
                                field: *field,
                                phrase: phrase.clone(),
                            });
                        }
                    }
                }
                if required_phrases
                    .iter()
                    .any(|phrase| !phrase_evidence.iter().any(|e| &e.phrase == phrase))
                {
                    result.excluded_count += 1;
                    if result.exclusions.len() < MAX_RESULTS {
                        result.exclusions.push(Exclusion {
                            asset: asset.key(),
                            reason: ExclusionReason::RequiredPhraseMissing,
                        });
                    }
                    continue;
                }
            }
            let bm25_micros: u64 = evidence.iter().map(|entry| entry.contribution_micros).sum();
            if bm25_micros == 0 {
                continue;
            }
            let matched: BTreeSet<_> = evidence.iter().map(|e| &e.term).collect();
            let mut breakdown = BTreeMap::new();
            for item in &evidence {
                *breakdown.entry(item.field).or_insert(0) += item.contribution_micros;
            }
            let utility = if trusted(asset.trust) {
                // Absolute monotonic transform; adding an untrusted match cannot
                // renormalize a trusted match upward or downward.
                ((u128::from(bm25_micros) * 1_000_000) / (u128::from(bm25_micros) + 1_000_000))
                    as i64
            } else {
                0
            };
            result.candidates.push(SearchCandidate {
                asset: asset.key(),
                trust: asset.trust,
                bm25_micros,
                route_utility_micros: utility,
                coverage_micros: (matched.len() * 1_000_000 / terms.len()) as u32,
                fields: breakdown
                    .into_iter()
                    .map(|(field, score_micros)| FieldScore {
                        field,
                        score_micros,
                    })
                    .collect(),
                evidence,
                phrase_evidence,
            });
        }
        result.candidates.sort_by(|a, b| {
            trusted(b.trust)
                .cmp(&trusted(a.trust))
                .then_with(|| b.bm25_micros.cmp(&a.bm25_micros))
                .then_with(|| a.asset.cmp(&b.asset))
        });
        result.abstention = abstention(&result.candidates, query.policy);
        if result.abstention.is_none() {
            result.selected = result.candidates.first().map(|c| c.asset.clone());
        }
        // Decide before truncating, including top_k=1, so ambiguity stays visible.
        result.candidates.truncate(query.top_k);
        Ok(result)
    }

    /// Adapter for the existing policy contract. Trust is independently checked
    /// again by router-policy against its authoritative catalog.
    pub fn skill_candidates(&self, query: &SearchQuery) -> Result<Vec<SkillCandidate>> {
        let mut query = query.clone();
        query.kind = Some(AssetKind::Skill);
        let result = self.search(&query)?;
        if result.abstention.is_some() {
            return Ok(Vec::new());
        }
        result
            .candidates
            .into_iter()
            .filter(|c| trusted(c.trust))
            .map(|candidate| {
                Ok(SkillCandidate {
                    skill: SkillId::parse(candidate.asset.id)
                        .map_err(|_| RetrievalError::Invalid("skill identifier"))?,
                    source: format!("{INDEX_VERSION}:{}", self.index_hash),
                    score_micros: candidate.route_utility_micros,
                })
            })
            .collect()
    }

    fn excluded_by_trigger(&self, text: &str) -> Result<BTreeSet<usize>> {
        let normalized = tokenize::phrase(text);
        let words: Vec<_> = normalized.split_whitespace().collect();
        if words.len() > MAX_QUERY_TERMS {
            return Err(RetrievalError::Limit("query words"));
        }
        let mut excluded = BTreeSet::new();
        for start in 0..words.len() {
            let mut phrase = String::new();
            for word in &words[start..] {
                if !phrase.is_empty() {
                    phrase.push(' ');
                }
                phrase.push_str(word);
                if let Some(ids) = self.anti_triggers.get(&phrase) {
                    excluded.extend(ids);
                }
            }
        }
        Ok(excluded)
    }
}

fn validate_query(query: &SearchQuery) -> Result<()> {
    if query.text.len() > MAX_QUERY_BYTES {
        return Err(RetrievalError::Limit("query bytes"));
    }
    if query.top_k == 0 || query.top_k > MAX_RESULTS {
        return Err(RetrievalError::Limit("top_k"));
    }
    if query.policy.minimum_coverage_micros > 1_000_000
        || query.policy.ambiguity_margin_micros > 1_000_000
    {
        return Err(RetrievalError::Invalid("query policy fractions"));
    }
    Ok(())
}

fn abstention(candidates: &[SearchCandidate], policy: SearchPolicy) -> Option<Abstention> {
    let Some(first) = candidates.first() else {
        return Some(Abstention::NoMatches);
    };
    if !trusted(first.trust) {
        return Some(Abstention::UntrustedOnly);
    }
    if first.bm25_micros < policy.minimum_score_micros
        || first.coverage_micros < policy.minimum_coverage_micros
    {
        return Some(Abstention::WeakMatch);
    }
    if let Some(second) = candidates.get(1).filter(|c| trusted(c.trust)) {
        let difference = first.bm25_micros - second.bm25_micros;
        if u128::from(difference) * 1_000_000
            <= u128::from(first.bm25_micros) * u128::from(policy.ambiguity_margin_micros)
        {
            return Some(Abstention::Ambiguous);
        }
    }
    None
}

fn fields(asset: &AssetMetadata) -> Result<Vec<(Field, String)>> {
    let kinds = serde_json::to_value(&asset.task_kinds)
        .map_err(|_| RetrievalError::Invalid("task kinds"))?;
    let kind_text = kinds
        .as_array()
        .ok_or(RetrievalError::Invalid("task kinds"))?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    Ok(vec![
        (Field::Identifier, asset.id.clone()),
        (Field::TaskKinds, kind_text),
        (Field::Triggers, asset.triggers.join(" \n")),
        (Field::Description, asset.description.clone()),
        (Field::Examples, asset.examples.join(" \n")),
        (Field::Tags, asset.tags.join(" \n")),
        (Field::Paths, asset.paths.join(" \n")),
    ])
}

fn parse(bytes: &[u8]) -> Result<MetadataDocument> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(RetrievalError::Limit("metadata document bytes"));
    }
    serde_json::from_slice(bytes).map_err(|_| RetrievalError::Invalid("metadata JSON schema"))
}

fn hash_document(document: &MetadataDocument) -> Result<String> {
    let bytes = serde_json::to_vec(document)
        .map_err(|_| RetrievalError::Invalid("metadata serialization"))?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(RetrievalError::Limit("metadata document bytes"));
    }
    Ok(digest(&bytes))
}

fn canonical(mut document: MetadataDocument) -> Result<MetadataDocument> {
    if document.schema_version != "1"
        || document.catalog_version.trim().is_empty()
        || document.catalog_version.len() > 128
    {
        return Err(RetrievalError::Invalid("schema or catalog version"));
    }
    if document.assets.len() > MAX_ASSETS {
        return Err(RetrievalError::Limit("asset count"));
    }
    let mut total_bytes = 0usize;
    let mut keys = BTreeSet::new();
    for asset in &mut document.assets {
        if asset.id.is_empty()
            || asset.id.len() > 128
            || !asset
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-_.:/".contains(&c))
        {
            return Err(RetrievalError::Invalid("canonical asset identifier"));
        }
        if !keys.insert(asset.key()) {
            return Err(RetrievalError::Invalid(
                "duplicate asset identifier within kind",
            ));
        }
        if asset.description.len() > MAX_FIELD_BYTES {
            return Err(RetrievalError::Limit("description bytes"));
        }
        let mut asset_bytes = asset.id.len() + asset.description.len();
        for values in [
            &mut asset.triggers,
            &mut asset.examples,
            &mut asset.tags,
            &mut asset.paths,
            &mut asset.anti_triggers,
        ] {
            if values.len() > MAX_LIST_ITEMS {
                return Err(RetrievalError::Limit("metadata list items"));
            }
            let size: usize = values.iter().map(String::len).sum();
            if size > MAX_FIELD_BYTES {
                return Err(RetrievalError::Limit("metadata field bytes"));
            }
            if values.iter().any(|s| s.trim().is_empty()) {
                return Err(RetrievalError::Invalid("empty metadata list entry"));
            }
            asset_bytes += size;
            values.sort();
            values.dedup();
        }
        if asset.task_kinds.len() > MAX_LIST_ITEMS {
            return Err(RetrievalError::Limit("task kinds"));
        }
        asset.task_kinds.sort();
        asset.task_kinds.dedup();
        if let Some(body) = &asset.body {
            body::validate_relative(&body.path)?;
            if body.sha256.len() != 64
                || !body
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(RetrievalError::Invalid("body SHA-256"));
            }
            asset_bytes += body.path.len() + body.sha256.len();
        }
        if asset_bytes > MAX_ASSET_BYTES {
            return Err(RetrievalError::Limit("asset metadata bytes"));
        }
        total_bytes += asset_bytes;
        if total_bytes > MAX_DOCUMENT_BYTES {
            return Err(RetrievalError::Limit("metadata bytes"));
        }
    }
    document
        .assets
        .sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
    Ok(document)
}
