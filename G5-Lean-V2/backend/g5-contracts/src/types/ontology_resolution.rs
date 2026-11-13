//! Shared ontology semantic-read DTOs and resolver.
//!
//! The resolver is deliberately pure: callers provide an in-memory corpus of
//! parsed ontology documents and receive a deterministic bundle. It never reads
//! or writes repository state.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ontology_markup::{
    parse_ontology_document, BindingFact, IdBlockFact, OntologyMarkupDocument, ReferenceFact,
    ReferencePlacement, ReferenceSpecificity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyReadMode {
    Raw,
    Resolved,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyReadOptions {
    pub component_id: String,
    pub mode: OntologyReadMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<usize>,
    #[serde(default)]
    pub include_transitive: bool,
    #[serde(default)]
    pub include_diagnostics: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targeted_ref: Option<OntologyTargetRef>,
}

impl OntologyReadOptions {
    #[must_use]
    pub fn new(component_id: impl Into<String>, mode: OntologyReadMode) -> Self {
        Self {
            component_id: component_id.into(),
            mode,
            token_budget: None,
            include_transitive: false,
            include_diagnostics: true,
            start_block_id: None,
            targeted_ref: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyTargetRef {
    pub component_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_block: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologySourceDocument {
    pub component_id: String,
    pub path: String,
    pub content: String,
    pub content_hash: String,
    pub markup: OntologyMarkupDocument,
}

impl OntologySourceDocument {
    #[must_use]
    pub fn from_content(
        component_id: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let component_id = component_id.into();
        let path = path.into();
        let content = content.into();
        let content_hash = sha256_hex(content.as_bytes());
        let markup = parse_ontology_document(&component_id, &content);
        Self {
            component_id,
            path,
            content,
            content_hash,
            markup,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyResolutionSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyResolutionDiagnostic {
    pub code: String,
    pub severity: OntologyResolutionSeverity,
    pub component_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedNeededBy {
    pub source_component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_block: Option<String>,
    // Cleared (and then omitted from JSON) when the read_ontology caller sets
    // omit_block_source_text — resolvedMarkdown carries the attribution text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub raw_ref_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOntologyBlock {
    pub target_component: String,
    pub target_block: String,
    pub source_hash: String,
    pub source_line: usize,
    // Cleared (and then omitted from JSON) when the read_ontology caller sets
    // omit_block_source_text — resolvedMarkdown stays the single text carrier.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_text: String,
    #[serde(default)]
    pub needed_by: Vec<ResolvedNeededBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binding_overlays: Vec<String>,
    pub transitive_depth: usize,
    pub cycle_cut: bool,
    pub token_estimate: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OmittedOntologyBlock {
    pub target_component: String,
    pub target_block: String,
    pub reason: String,
    pub token_estimate: usize,
    #[serde(default)]
    pub needed_by: Vec<ResolvedNeededBy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOntologyBundle {
    pub root_component_id: String,
    pub mode: OntologyReadMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_markdown: Option<String>,
    #[serde(default)]
    pub resolved_blocks: Vec<ResolvedOntologyBlock>,
    #[serde(default)]
    pub omitted_blocks: Vec<OmittedOntologyBlock>,
    #[serde(default)]
    pub diagnostics: Vec<OntologyResolutionDiagnostic>,
    /// Set when `start_block_id` was requested but the ROOT component carries
    /// blocking diagnostics: block-span attribution is unreliable (a duplicate
    /// or phantom id can steal the seed block's refs), so consumers must fail
    /// loudly instead of trusting a silently-empty block-scoped resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_block_abort: Option<String>,
    pub total_token_estimate: usize,
    pub cache_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BlockKey {
    component_id: String,
    block_id: String,
}

#[derive(Debug, Clone)]
struct CandidateBlock {
    block: ResolvedOntologyBlock,
    order: usize,
}

#[derive(Debug, Clone)]
struct ReferencedCacheEntry {
    target_component: String,
    target_block: String,
    source_hash: String,
    token_estimate: usize,
    omitted_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct RefContext {
    source_component: String,
    source_block: Option<String>,
    raw_ref_text: String,
    rationale: Option<String>,
    binding_texts: Vec<String>,
    depth: usize,
}

#[must_use]
pub fn resolve_ontology_bundle(
    documents: &[OntologySourceDocument],
    options: &OntologyReadOptions,
) -> ResolvedOntologyBundle {
    let docs_by_component = documents
        .iter()
        .map(|doc| (doc.component_id.as_str(), doc))
        .collect::<BTreeMap<_, _>>();
    let root = docs_by_component
        .get(options.component_id.as_str())
        .copied();

    let mut diagnostics = Vec::new();
    if root.is_none() {
        diagnostics.push(OntologyResolutionDiagnostic {
            code: "root_not_found".to_string(),
            severity: OntologyResolutionSeverity::Blocking,
            component_id: options.component_id.clone(),
            block_id: None,
            line: None,
            message: format!(
                "Ontology component '{}' was not found",
                options.component_id
            ),
            evidence: Vec::new(),
        });
    }

    let mut block_index: BTreeMap<BlockKey, (&OntologySourceDocument, &IdBlockFact)> =
        BTreeMap::new();
    for doc in documents {
        for block in &doc.markup.id_blocks {
            let key = BlockKey {
                component_id: doc.component_id.clone(),
                block_id: block.raw_id.clone(),
            };
            if let Some((first_doc, _)) = block_index.get(&key) {
                diagnostics.push(OntologyResolutionDiagnostic {
                    code: "duplicate_block_id".to_string(),
                    severity: OntologyResolutionSeverity::Blocking,
                    component_id: doc.component_id.clone(),
                    block_id: Some(block.raw_id.clone()),
                    line: Some(block.start_line),
                    message: format!(
                        "Ontology component '{}' defines block id '{}' more than once",
                        doc.component_id, block.raw_id
                    ),
                    evidence: vec![
                        format!("firstPath={}", first_doc.path),
                        format!("duplicatePath={}", doc.path),
                    ],
                });
                continue;
            }
            block_index.insert(key, (doc, block));
        }
    }

    // Spec D7: a nonexistent seed block must fail loudly (Warning), never as a
    // silently-empty resolution. Blocking root diagnostics take precedence —
    // the D6 abort already covers attribution-unsafe roots.
    if options.mode != OntologyReadMode::Raw {
        if let (Some(root_doc), Some(start_block)) = (root, options.start_block_id.as_deref()) {
            let root_has_blocking = diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == OntologyResolutionSeverity::Blocking
                    && diagnostic.component_id == options.component_id
            });
            let seed_exists = root_doc
                .markup
                .id_blocks
                .iter()
                .any(|block| block.raw_id == start_block);
            if !root_has_blocking && !seed_exists {
                diagnostics.push(OntologyResolutionDiagnostic {
                    code: "start_block_not_found".to_string(),
                    severity: OntologyResolutionSeverity::Warning,
                    component_id: options.component_id.clone(),
                    block_id: Some(start_block.to_string()),
                    line: None,
                    message: format!(
                        "start_block_id '{start_block}' does not exist in root component '{}'; block-scoped resolution is empty",
                        options.component_id
                    ),
                    evidence: Vec::new(),
                });
            }
        }
    }

    let mut candidates = BTreeMap::<BlockKey, CandidateBlock>::new();
    let mut order = 0usize;
    let mut active_stack = HashSet::<BlockKey>::new();
    if options.mode != OntologyReadMode::Raw {
        if let Some(root_doc) = root {
            collect_refs_from_document(
                root_doc,
                options.start_block_id.as_deref(),
                0,
                options,
                &docs_by_component,
                &block_index,
                &mut candidates,
                &mut order,
                &mut active_stack,
                &mut diagnostics,
            );
        }
    }

    let mut ordered = candidates.into_values().collect::<Vec<_>>();
    ordered.sort_by_key(|candidate| candidate.order);
    let candidate_count = ordered.len();

    let mut resolved_blocks = Vec::new();
    let mut omitted_blocks = Vec::new();
    let mut referenced_cache_entries = Vec::new();
    let mut total_token_estimate = 0usize;
    let budget = options.token_budget.unwrap_or(usize::MAX);
    for candidate in ordered {
        let block = candidate.block;
        let estimate = block.token_estimate;
        if total_token_estimate.saturating_add(estimate) <= budget {
            total_token_estimate += estimate;
            referenced_cache_entries.push(ReferencedCacheEntry {
                target_component: block.target_component.clone(),
                target_block: block.target_block.clone(),
                source_hash: block.source_hash.clone(),
                token_estimate: estimate,
                omitted_reason: None,
            });
            resolved_blocks.push(block);
        } else {
            referenced_cache_entries.push(ReferencedCacheEntry {
                target_component: block.target_component.clone(),
                target_block: block.target_block.clone(),
                source_hash: block.source_hash.clone(),
                token_estimate: estimate,
                omitted_reason: Some("token_budget_exceeded".to_string()),
            });
            omitted_blocks.push(OmittedOntologyBlock {
                target_component: block.target_component,
                target_block: block.target_block,
                reason: "token_budget_exceeded".to_string(),
                token_estimate: estimate,
                needed_by: block.needed_by,
            });
        }
    }

    if options.token_budget.is_some() && resolved_blocks.is_empty() && !omitted_blocks.is_empty() {
        let omitted_count = omitted_blocks.len();
        let smallest_omitted = omitted_blocks
            .iter()
            .map(|block| block.token_estimate)
            .min()
            .unwrap_or(0);
        diagnostics.push(OntologyResolutionDiagnostic {
            code: "token_budget_omitted_all_blocks".to_string(),
            severity: OntologyResolutionSeverity::Warning,
            component_id: options.component_id.clone(),
            block_id: options.start_block_id.clone(),
            line: None,
            message: format!(
                "token_budget={} omitted all {omitted_count} candidate block(s); smallest omitted estimate is {smallest_omitted} tokens. Retry with mode=\"resolved\" plus omit_block_source_text=true, raise token_budget, or omit start_block_id for whole-node context.",
                options.token_budget.unwrap_or_default()
            ),
            evidence: omitted_blocks
                .iter()
                .take(5)
                .map(|block| {
                    format!(
                        "{}#{} estimated_tokens={}",
                        block.target_component, block.target_block, block.token_estimate
                    )
                })
                .collect(),
        });
    }

    if options.mode != OntologyReadMode::Raw
        && options.start_block_id.is_some()
        && candidate_count == 0
        && omitted_blocks.is_empty()
        && !diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "start_block_not_found"
                || diagnostic.code == "target_component_not_found"
                || diagnostic.code == "target_block_not_found"
                || diagnostic.code == "whole_node_ref_not_materialized"
                || (diagnostic.severity == OntologyResolutionSeverity::Blocking
                    && diagnostic.component_id == options.component_id)
        })
    {
        diagnostics.push(OntologyResolutionDiagnostic {
            code: "start_block_has_no_inline_refs".to_string(),
            severity: OntologyResolutionSeverity::Warning,
            component_id: options.component_id.clone(),
            block_id: options.start_block_id.clone(),
            line: None,
            message: format!(
                "start_block_id '{}' exists but contributes no materialized block-targeted inline @ref entries; block-scoped resolved context is empty",
                options.start_block_id.as_deref().unwrap_or_default()
            ),
            evidence: Vec::new(),
        });
    }

    dedup_exact_diagnostics(&mut diagnostics);

    let start_block_abort = options.start_block_id.as_deref().and_then(|start_block| {
        let blocking = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.severity == OntologyResolutionSeverity::Blocking
                    && diagnostic.component_id == options.component_id
            })
            .map(|diagnostic| {
                let block = diagnostic
                    .block_id
                    .as_deref()
                    .map(|id| format!(" block '{id}'"))
                    .unwrap_or_default();
                let line = diagnostic
                    .line
                    .map(|line| format!(" line {line}"))
                    .unwrap_or_default();
                format!("{}{}{}", diagnostic.code, block, line)
            })
            .collect::<Vec<_>>();
        if blocking.is_empty() {
            None
        } else {
            Some(format!(
                "start_block_id '{}' resolution aborted: root component '{}' has blocking diagnostics: {}. Fix these (a line-start '[Dn]' bracket defines a block and can steal the seed block's refs) or omit start_block_id for whole-node resolution.",
                start_block,
                options.component_id,
                blocking.join("; ")
            ))
        }
    });

    if !options.include_diagnostics {
        diagnostics.clear();
    }

    let resolved_markdown = match options.mode {
        OntologyReadMode::Raw => None,
        OntologyReadMode::Resolved | OntologyReadMode::Both => {
            Some(render_resolved_markdown(&resolved_blocks, &omitted_blocks))
        }
    };
    let raw_source = match options.mode {
        OntologyReadMode::Raw | OntologyReadMode::Both => root.map(|doc| doc.content.clone()),
        OntologyReadMode::Resolved => None,
    };
    let cache_key = compute_cache_key(root, options, &referenced_cache_entries, &diagnostics);

    ResolvedOntologyBundle {
        root_component_id: options.component_id.clone(),
        start_block_abort,
        mode: options.mode,
        raw_source,
        resolved_markdown,
        resolved_blocks,
        omitted_blocks,
        diagnostics,
        total_token_estimate,
        cache_key,
    }
}

/// Drop byte-identical duplicate diagnostics, keeping first-occurrence order.
/// Transitive resolution re-walks an already-resolved block's refs when a
/// second traversal path reaches it, re-emitting the same diagnostic (one
/// `cycle_cut` per extra path); runs before abort computation and cache-key
/// derivation so bundle identity matches the deduplicated list.
fn dedup_exact_diagnostics(diagnostics: &mut Vec<OntologyResolutionDiagnostic>) {
    let mut seen: Vec<OntologyResolutionDiagnostic> = Vec::with_capacity(diagnostics.len());
    diagnostics.retain(|diagnostic| {
        if seen.contains(diagnostic) {
            false
        } else {
            seen.push(diagnostic.clone());
            true
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn collect_refs_from_document(
    doc: &OntologySourceDocument,
    owner_filter: Option<&str>,
    depth: usize,
    options: &OntologyReadOptions,
    docs_by_component: &BTreeMap<&str, &OntologySourceDocument>,
    block_index: &BTreeMap<BlockKey, (&OntologySourceDocument, &IdBlockFact)>,
    candidates: &mut BTreeMap<BlockKey, CandidateBlock>,
    order: &mut usize,
    active_stack: &mut HashSet<BlockKey>,
    diagnostics: &mut Vec<OntologyResolutionDiagnostic>,
) {
    let targeted_root = depth == 0 && options.targeted_ref.is_some();
    let mut source_ref_indexes = BTreeMap::<Option<String>, usize>::new();
    for (ref_index, reference) in doc.markup.refs.iter().enumerate() {
        let source_ref_index = source_ref_indexes
            .entry(reference.owner_id.clone())
            .and_modify(|index| *index += 1)
            .or_insert(0);
        let local_source_ref_index = *source_ref_index;
        let is_targeted_ref = options
            .targeted_ref
            .as_ref()
            .is_some_and(|target| targeted_ref_matches(reference, local_source_ref_index, target));
        if targeted_root {
            if !is_targeted_ref {
                continue;
            }
        } else if reference.placement != ReferencePlacement::Inline {
            continue;
        }
        if owner_filter.is_some_and(|owner| reference.owner_id.as_deref() != Some(owner)) {
            continue;
        }
        resolve_reference(
            doc,
            ref_index,
            reference,
            depth,
            options,
            docs_by_component,
            block_index,
            candidates,
            order,
            active_stack,
            diagnostics,
        );
    }
}

fn targeted_ref_matches(
    reference: &ReferenceFact,
    source_ref_index: usize,
    target: &OntologyTargetRef,
) -> bool {
    if reference.target_component != target.component_id {
        return false;
    }
    if reference.target_block.as_deref() != target.block_id.as_deref() {
        return false;
    }
    if let Some(source_block) = target.source_block.as_deref() {
        if reference.owner_id.as_deref() != Some(source_block) {
            return false;
        }
    }
    if let Some(expected_index) = target.source_ref_index {
        if source_ref_index != expected_index {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn resolve_reference(
    source_doc: &OntologySourceDocument,
    ref_index: usize,
    reference: &ReferenceFact,
    depth: usize,
    options: &OntologyReadOptions,
    docs_by_component: &BTreeMap<&str, &OntologySourceDocument>,
    block_index: &BTreeMap<BlockKey, (&OntologySourceDocument, &IdBlockFact)>,
    candidates: &mut BTreeMap<BlockKey, CandidateBlock>,
    order: &mut usize,
    active_stack: &mut HashSet<BlockKey>,
    diagnostics: &mut Vec<OntologyResolutionDiagnostic>,
) {
    if reference.specificity == ReferenceSpecificity::WholeNode {
        diagnostics.push(OntologyResolutionDiagnostic {
            code: "whole_node_ref_not_materialized".to_string(),
            severity: OntologyResolutionSeverity::Info,
            component_id: reference.target_component.clone(),
            block_id: None,
            line: Some(reference.line),
            message: "Whole-node @ref is not materialized by read_ontology; use a block-targeted @ref for semantic injection".to_string(),
            evidence: vec![reference.raw_text.clone()],
        });
        return;
    }

    let Some(target_block) = reference.target_block.as_ref() else {
        return;
    };
    let target_key = BlockKey {
        component_id: reference.target_component.clone(),
        block_id: target_block.clone(),
    };
    let ref_context = RefContext {
        source_component: source_doc.component_id.clone(),
        source_block: reference.owner_id.clone(),
        raw_ref_text: reference.raw_text.clone(),
        rationale: reference.rationale.clone(),
        binding_texts: attached_bindings(&source_doc.markup.bindings, ref_index),
        depth,
    };

    let Some((target_doc, id_block)) = block_index.get(&target_key).copied() else {
        let doc_exists = docs_by_component.contains_key(reference.target_component.as_str());
        diagnostics.push(OntologyResolutionDiagnostic {
            code: if doc_exists {
                "target_block_not_found".to_string()
            } else {
                "target_component_not_found".to_string()
            },
            severity: OntologyResolutionSeverity::Warning,
            component_id: reference.target_component.clone(),
            block_id: Some(target_block.clone()),
            line: Some(reference.line),
            message: format!(
                "Could not resolve block-targeted @ref {}#{}",
                reference.target_component, target_block
            ),
            evidence: vec![reference.raw_text.clone()],
        });
        return;
    };

    let cycle_cut = active_stack.contains(&target_key);
    upsert_candidate(
        candidates,
        order,
        target_key.clone(),
        target_doc,
        id_block,
        &ref_context,
        cycle_cut,
    );

    if cycle_cut {
        diagnostics.push(OntologyResolutionDiagnostic {
            code: "cycle_cut".to_string(),
            severity: OntologyResolutionSeverity::Info,
            component_id: target_key.component_id.clone(),
            block_id: Some(target_key.block_id.clone()),
            line: Some(reference.line),
            message: "Transitive ontology resolution stopped at a cycle".to_string(),
            evidence: vec![reference.raw_text.clone()],
        });
        return;
    }

    if options.include_transitive {
        active_stack.insert(target_key.clone());
        collect_refs_from_document(
            target_doc,
            Some(target_block),
            depth + 1,
            options,
            docs_by_component,
            block_index,
            candidates,
            order,
            active_stack,
            diagnostics,
        );
        active_stack.remove(&target_key);
    }
}

fn upsert_candidate(
    candidates: &mut BTreeMap<BlockKey, CandidateBlock>,
    order: &mut usize,
    key: BlockKey,
    target_doc: &OntologySourceDocument,
    id_block: &IdBlockFact,
    ref_context: &RefContext,
    cycle_cut: bool,
) {
    let binding_text = ref_context.binding_texts.first().cloned();
    let needed_by = ResolvedNeededBy {
        source_component: ref_context.source_component.clone(),
        source_block: ref_context.source_block.clone(),
        raw_ref_text: ref_context.raw_ref_text.clone(),
        rationale: ref_context.rationale.clone(),
        binding_text,
    };

    if let Some(existing) = candidates.get_mut(&key) {
        if !existing.block.needed_by.contains(&needed_by) {
            existing.block.needed_by.push(needed_by);
        }
        for binding in &ref_context.binding_texts {
            if !existing.block.binding_overlays.contains(binding) {
                existing.block.binding_overlays.push(binding.clone());
            }
        }
        existing.block.transitive_depth = existing.block.transitive_depth.min(ref_context.depth);
        existing.block.cycle_cut |= cycle_cut;
        return;
    }

    let token_estimate = estimate_ontology_tokens(&id_block.raw_text);
    let relationship_reason = ref_context.rationale.clone();
    let candidate = CandidateBlock {
        block: ResolvedOntologyBlock {
            target_component: target_doc.component_id.clone(),
            target_block: id_block.raw_id.clone(),
            source_hash: block_source_hash(target_doc, id_block),
            source_line: id_block.start_line,
            source_text: id_block.raw_text.clone(),
            needed_by: vec![needed_by],
            relationship_reason,
            binding_overlays: ref_context.binding_texts.clone(),
            transitive_depth: ref_context.depth,
            cycle_cut,
            token_estimate,
        },
        order: *order,
    };
    *order += 1;
    candidates.insert(key, candidate);
}

fn attached_bindings(bindings: &[BindingFact], ref_index: usize) -> Vec<String> {
    let mut attached = bindings
        .iter()
        .filter(|binding| binding.attached_ref_indices.contains(&ref_index))
        .map(|binding| binding.raw_text.clone())
        .collect::<Vec<_>>();
    attached.sort();
    attached.dedup();
    attached
}

fn render_resolved_markdown(
    resolved_blocks: &[ResolvedOntologyBlock],
    omitted_blocks: &[OmittedOntologyBlock],
) -> String {
    let mut out = String::new();
    for block in resolved_blocks {
        out.push_str(&format!(
            "<!-- g5-resolved-ontology-block source=\"{}\" block=\"{}\" hash=\"{}\" read_only=\"true\" -->\n",
            block.target_component,
            block.target_block,
            short_hash(&block.source_hash)
        ));
        out.push_str(&format!(
            "### {}#{}\n",
            block.target_component, block.target_block
        ));
        out.push_str("Needed by:");
        for need in &block.needed_by {
            match &need.source_block {
                Some(source_block) => out.push_str(&format!(
                    " {}#{} via `{}`;",
                    need.source_component, source_block, need.raw_ref_text
                )),
                None => out.push_str(&format!(
                    " {} via `{}`;",
                    need.source_component, need.raw_ref_text
                )),
            }
        }
        out.push('\n');
        for binding in &block.binding_overlays {
            out.push_str(binding);
            out.push('\n');
        }
        out.push_str(block.source_text.trim_end());
        out.push('\n');
        out.push_str("<!-- /g5-resolved-ontology-block -->\n\n");
    }

    if !omitted_blocks.is_empty() {
        out.push_str("<!-- g5-resolved-ontology-omissions -->\n");
        for omitted in omitted_blocks {
            out.push_str(&format!(
                "- {}#{} omitted: {} (estimated_tokens={})\n",
                omitted.target_component,
                omitted.target_block,
                omitted.reason,
                omitted.token_estimate
            ));
        }
        out.push_str("<!-- /g5-resolved-ontology-omissions -->\n");
    }

    out
}

fn compute_cache_key(
    root: Option<&OntologySourceDocument>,
    options: &OntologyReadOptions,
    referenced_blocks: &[ReferencedCacheEntry],
    diagnostics: &[OntologyResolutionDiagnostic],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "mode={:?};budget={:?};transitive={};start={:?};",
        options.mode, options.token_budget, options.include_transitive, options.start_block_id
    ));
    if let Some(root) = root {
        hasher.update(format!("root={}:{};", root.component_id, root.content_hash));
    } else {
        hasher.update(format!("root-missing={};", options.component_id));
    }

    for block in referenced_blocks {
        hasher.update(format!(
            "ref={}#{}:{}:{}:{:?};",
            block.target_component,
            block.target_block,
            block.source_hash,
            block.token_estimate,
            block.omitted_reason
        ));
    }
    for diagnostic in diagnostics {
        hasher.update(format!(
            "diagnostic={}:{}:{:?};",
            diagnostic.code, diagnostic.component_id, diagnostic.block_id
        ));
    }
    format!("{:x}", hasher.finalize())
}

fn block_source_hash(target_doc: &OntologySourceDocument, id_block: &IdBlockFact) -> String {
    sha256_hex(
        format!(
            "{}#{}\n{}",
            target_doc.component_id, id_block.raw_id, id_block.raw_text
        )
        .as_bytes(),
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

#[must_use]
pub fn estimate_ontology_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(component_id: &str, body: &str) -> OntologySourceDocument {
        OntologySourceDocument::from_content(
            component_id,
            format!(".semlift/specs/{component_id}.semlift.md"),
            body,
        )
    }

    fn architecture_doc() -> &'static str {
        r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Orders flow]
Use @ref data_model/company#D2 because orders persist a company pointer.
**Binding:** maps company_id onto orders.company_id.

## References
- @ref data_model/company #D2
"#
    }

    fn concept_doc() -> &'static str {
        r#"---
component_id: data_model/company
---
# Company

[D1. Company identity]
Company is the tenant boundary.

[D2. Company persistence]
Company rows include id, name, and tax identifier.
"#
    }

    #[test]
    fn raw_mode_preserves_old_format_source_without_resolution() {
        let old = r#"---
component_id: legacy/orders
---
# Legacy Orders

[I1. Old behavior]
No v4 references here.
"#;
        let options = OntologyReadOptions::new("legacy/orders", OntologyReadMode::Raw);
        let bundle = resolve_ontology_bundle(&[doc("legacy/orders", old)], &options);
        assert_eq!(bundle.raw_source.as_deref(), Some(old));
        assert!(bundle.resolved_markdown.is_none());
        assert!(bundle.resolved_blocks.is_empty());
        assert!(bundle.omitted_blocks.is_empty());
    }

    #[test]
    fn resolves_inline_block_refs_and_attached_bindings() {
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Both);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );

        assert_eq!(bundle.resolved_blocks.len(), 1);
        let block = &bundle.resolved_blocks[0];
        assert_eq!(block.target_component, "data_model/company");
        assert_eq!(block.target_block, "D2");
        assert!(block.source_text.contains("Company rows include"));
        assert_eq!(
            block.binding_overlays,
            vec!["**Binding:** maps company_id onto orders.company_id.".to_string()]
        );
        assert_eq!(block.needed_by[0].source_block.as_deref(), Some("D1"));
        assert!(bundle
            .resolved_markdown
            .as_deref()
            .unwrap()
            .contains("g5-resolved-ontology-block"));
    }

    #[test]
    fn resolves_inline_refs_to_heading_prefixed_target_blocks() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Orders flow]
Use @ref performance_requirement#D6 because the workflow must meet latency goals.
"#;
        let target = r#"---
component_id: performance_requirement
---
# Performance

### [D6. Heading target]

Visible target details for the resolver.
"#;
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("performance_requirement", target),
            ],
            &options,
        );

        assert_eq!(bundle.resolved_blocks.len(), 1);
        let block = &bundle.resolved_blocks[0];
        assert_eq!(block.target_component, "performance_requirement");
        assert_eq!(block.target_block, "D6");
        assert_eq!(block.source_line, 6);
        assert!(block.source_text.contains("Visible target details"));
        assert!(bundle
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "target_block_not_found"));
    }

    #[test]
    fn derived_references_index_is_not_authoritative() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Orders flow]
This block has no inline reference.

## References
- @ref data_model/company #D2
"#;
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        assert!(bundle.resolved_blocks.is_empty());
        assert!(bundle.omitted_blocks.is_empty());
    }

    #[test]
    fn targeted_ref_resolves_derived_reference_index_entry() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Orders flow]
This inline ref points at the same target but must not be merged into the
source-located targeted read: @ref data_model/company#D2
**Binding:** inline binding should stay out of the selected popover.

## References

[I2. Reference index]
@ref data_model/company#D2
**Binding:** selected reference-index binding.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.targeted_ref = Some(OntologyTargetRef {
            component_id: "data_model/company".to_string(),
            block_id: Some("D2".to_string()),
            source_block: Some("I2".to_string()),
            source_ref_index: Some(0),
        });
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );

        assert_eq!(bundle.resolved_blocks.len(), 1);
        let block = &bundle.resolved_blocks[0];
        assert_eq!(block.target_component, "data_model/company");
        assert_eq!(block.target_block, "D2");
        assert!(block.source_text.contains("Company rows include"));
        assert_eq!(block.needed_by.len(), 1);
        assert_eq!(block.needed_by[0].source_block.as_deref(), Some("I2"));
        assert!(block.needed_by[0].binding_text.is_none());
        assert!(!block
            .binding_overlays
            .iter()
            .any(|binding| binding.contains("inline binding should stay out")));
    }

    #[test]
    fn whole_node_refs_are_diagnostics_not_expansions() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Orders flow]
Use @ref data_model/company for the concept family.
"#;
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle =
            resolve_ontology_bundle(&[doc("software_architecture/orders", root)], &options);
        assert!(bundle.resolved_blocks.is_empty());
        assert_eq!(
            bundle.diagnostics[0].code,
            "whole_node_ref_not_materialized"
        );
    }

    #[test]
    fn token_budget_omits_whole_blocks_without_slicing_text() {
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.token_budget = Some(1);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        assert!(bundle.resolved_blocks.is_empty());
        assert_eq!(bundle.omitted_blocks.len(), 1);
        assert_eq!(bundle.omitted_blocks[0].reason, "token_budget_exceeded");
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "token_budget_omitted_all_blocks"
                && diagnostic.severity == OntologyResolutionSeverity::Warning
        }));
    }

    #[test]
    fn start_block_without_materialized_refs_emits_warning() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. No refs]
This block is intentionally self-contained.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D1".to_string());
        let bundle =
            resolve_ontology_bundle(&[doc("software_architecture/orders", root)], &options);

        assert!(bundle.resolved_blocks.is_empty());
        assert!(bundle.omitted_blocks.is_empty());
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "start_block_has_no_inline_refs"
                && diagnostic.block_id.as_deref() == Some("D1")
        }));
    }

    #[test]
    fn start_block_without_refs_still_warns_with_non_root_blocking_diagnostics() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. No refs]
This block is intentionally self-contained.
"#;
        let duplicate_reference = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
First block.

[D2. Company persistence duplicate]
Second block.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D1".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", duplicate_reference),
            ],
            &options,
        );

        assert!(bundle.start_block_abort.is_none());
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "duplicate_block_id"
                && diagnostic.severity == OntologyResolutionSeverity::Blocking
                && diagnostic.component_id == "data_model/company"
        }));
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "start_block_has_no_inline_refs"
                && diagnostic.component_id == "software_architecture/orders"
                && diagnostic.block_id.as_deref() == Some("D1")
        }));
    }

    #[test]
    fn cache_key_changes_when_target_block_changes() {
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let original = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        let changed_concept = concept_doc().replace(
            "Company rows include id, name, and tax identifier.",
            "Company rows include id, legal name, and tax identifier.",
        );
        let changed = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", &changed_concept),
            ],
            &options,
        );

        assert_ne!(original.cache_key, changed.cache_key);
        assert_ne!(
            original.resolved_blocks[0].source_hash,
            changed.resolved_blocks[0].source_hash
        );
    }

    #[test]
    fn cache_key_stable_when_unreferenced_block_changes() {
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let original = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        let changed_concept = concept_doc().replace(
            "Company is the tenant boundary.",
            "Company owns tenant-visible identity metadata.",
        );
        let changed = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", &changed_concept),
            ],
            &options,
        );

        assert_eq!(original.cache_key, changed.cache_key);
        assert_eq!(
            original.resolved_blocks[0].source_hash,
            changed.resolved_blocks[0].source_hash
        );
    }

    #[test]
    fn start_block_limits_resolution_to_one_owner() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First]
Use @ref data_model/company#D1.

[D2. Second]
Use @ref data_model/company#D2.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D2".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        assert_eq!(bundle.resolved_blocks.len(), 1);
        assert_eq!(bundle.resolved_blocks[0].target_block, "D2");
    }

    #[test]
    fn include_transitive_resolves_references_from_resolved_blocks() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Root]
Use @ref data_model/company#D2.
"#;
        let concept = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
Company rows use @ref security_policy/tenant#D1.
"#;
        let security = r#"---
component_id: security_policy/tenant
---
# Tenant

[D1. Tenant guard]
Every row is scoped by company id.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.include_transitive = true;
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept),
                doc("security_policy/tenant", security),
            ],
            &options,
        );
        assert_eq!(bundle.resolved_blocks.len(), 2);
        assert_eq!(
            bundle.resolved_blocks[1].target_component,
            "security_policy/tenant"
        );
        assert_eq!(bundle.resolved_blocks[1].transitive_depth, 1);
    }

    #[test]
    fn duplicate_refs_emit_one_block_with_multiple_needed_by_entries() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First use]
Use @ref data_model/company#D2 because the order row stores company_id.

[D2. Second use]
Use @ref data_model/company#D2 because audit rows also store company_id.
"#;
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );

        assert_eq!(bundle.resolved_blocks.len(), 1);
        let block = &bundle.resolved_blocks[0];
        assert_eq!(block.target_component, "data_model/company");
        assert_eq!(block.target_block, "D2");
        assert_eq!(block.needed_by.len(), 2);
        assert_eq!(block.needed_by[0].source_block.as_deref(), Some("D1"));
        assert_eq!(block.needed_by[1].source_block.as_deref(), Some("D2"));
    }

    #[test]
    fn duplicate_block_ids_emit_blocking_diagnostic() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First]
Use @ref data_model/company#D2.
"#;
        let duplicate_blocks = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
First block.

[D2. Company persistence duplicate]
Second block.
"#;
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", duplicate_blocks),
            ],
            &options,
        );

        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "duplicate_block_id"
                && diagnostic.severity == OntologyResolutionSeverity::Blocking
                && diagnostic.component_id == "data_model/company"
                && diagnostic.block_id.as_deref() == Some("D2")
        }));
    }

    #[test]
    fn start_block_on_root_with_blocking_duplicates_sets_abort() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First]
Use @ref data_model/company#D2.

[D1. First duplicate]
Duplicate id on the root component.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D1".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );

        let abort = bundle.start_block_abort.expect("abort message expected");
        assert!(abort.contains("start_block_id 'D1'"));
        assert!(abort.contains("duplicate_block_id"));
        assert!(abort.contains("software_architecture/orders"));
    }

    #[test]
    fn nonexistent_start_block_emits_start_block_not_found_warning() {
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D9".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", architecture_doc()),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );

        assert!(bundle.resolved_blocks.is_empty());
        assert!(bundle.start_block_abort.is_none());
        assert!(bundle.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "start_block_not_found"
                && diagnostic.severity == OntologyResolutionSeverity::Warning
                && diagnostic.component_id == "software_architecture/orders"
                && diagnostic.block_id.as_deref() == Some("D9")
        }));
    }

    #[test]
    fn existing_seed_and_whole_node_reads_emit_no_start_block_not_found() {
        let docs = [
            doc("software_architecture/orders", architecture_doc()),
            doc("data_model/company", concept_doc()),
        ];

        let mut seeded =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        seeded.start_block_id = Some("D1".to_string());
        let bundle = resolve_ontology_bundle(&docs, &seeded);
        assert_eq!(bundle.resolved_blocks.len(), 1);
        assert!(bundle
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "start_block_not_found"));

        let whole_node =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(&docs, &whole_node);
        assert!(bundle
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "start_block_not_found"));
    }

    #[test]
    fn start_block_abort_absent_for_whole_node_clean_root_and_non_root_duplicates() {
        let duplicate_root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First]
Use @ref data_model/company#D2.

[D1. First duplicate]
Duplicate id on the root component.
"#;
        // Whole-node resolution on a root with duplicates: diagnostics only, no abort.
        let options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", duplicate_root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        assert!(bundle.start_block_abort.is_none());

        // Block-scoped resolution on a clean root: no abort, the ref resolves.
        let clean_root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. First]
Use @ref data_model/company#D2.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D1".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", clean_root),
                doc("data_model/company", concept_doc()),
            ],
            &options,
        );
        assert!(bundle.start_block_abort.is_none());
        assert_eq!(bundle.resolved_blocks.len(), 1);

        // Blocking duplicates on a NON-root component do not abort the read.
        let duplicate_reference = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
First block.

[D2. Company persistence duplicate]
Second block.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.start_block_id = Some("D1".to_string());
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", clean_root),
                doc("data_model/company", duplicate_reference),
            ],
            &options,
        );
        assert!(bundle.start_block_abort.is_none());
    }

    #[test]
    fn transitive_cycle_sets_cycle_cut_and_reports_diagnostic() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Root]
Use @ref data_model/company#D2.
"#;
        let concept = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
Company rows use @ref security_policy/tenant#D1.
"#;
        let security = r#"---
component_id: security_policy/tenant
---
# Tenant

[D1. Tenant guard]
Tenant checks use @ref data_model/company#D2.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.include_transitive = true;
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept),
                doc("security_policy/tenant", security),
            ],
            &options,
        );

        let company = bundle
            .resolved_blocks
            .iter()
            .find(|block| block.target_component == "data_model/company")
            .expect("company block should resolve");
        assert!(company.cycle_cut);
        assert!(bundle
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "cycle_cut"));
    }

    #[test]
    fn identical_diagnostics_from_multiple_traversal_paths_dedup_to_one() {
        let root = r#"---
component_id: software_architecture/orders
---
# Orders

[D1. Root first]
Use @ref data_model/company#D2.

[D2. Root second]
Also binds @ref data_model/company#D2.
"#;
        let concept = r#"---
component_id: data_model/company
---
# Company

[D2. Company persistence]
Company rows use @ref security_policy/tenant#D1.
"#;
        let security = r#"---
component_id: security_policy/tenant
---
# Tenant

[D1. Tenant guard]
Tenant checks use @ref data_model/company#D2.
"#;
        let mut options =
            OntologyReadOptions::new("software_architecture/orders", OntologyReadMode::Resolved);
        options.include_transitive = true;
        let bundle = resolve_ontology_bundle(
            &[
                doc("software_architecture/orders", root),
                doc("data_model/company", concept),
                doc("security_policy/tenant", security),
            ],
            &options,
        );

        let cycle_cuts = bundle
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "cycle_cut")
            .collect::<Vec<_>>();
        assert_eq!(cycle_cuts.len(), 1, "{:?}", bundle.diagnostics);
    }
}
