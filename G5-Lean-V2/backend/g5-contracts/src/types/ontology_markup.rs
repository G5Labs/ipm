// =============================================================================
// G5 Common Types — ontology_markup
// =============================================================================
// Shared data types and parsing functions for the ontology format contract.
// Consumed by g5-api (OntologyFormatService) and g5d (spec_ops, worker).
//
// See spec: g5/g5-common/types/ontology_markup
// =============================================================================

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ─── Enums ──────────────────────────────────────────────────────────────────

/// Severity a `Q` record's label may carry, mirroring the `g5-question`
/// payload's escalation vocabulary (center-panel D69).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRecordSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl SemanticRecordSeverity {
    /// Canonical token spelling, for reporting a label/payload disagreement.
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    #[doc(hidden)]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Parsed semantic metadata from an end-of-label `(Tags: ...)` suffix.
///
/// `human`/`uncertain` are authored on any record. `answered` and `severity`
/// are written from the record's `g5-question` payload and are valid on `Q`
/// records ONLY — question state is stored metadata, so every consumer reading
/// the label sees it without parsing the fence. New fields default so an older
/// serialized payload still deserializes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRecordMetadata {
    pub human: bool,
    pub uncertain: bool,
    #[serde(default)]
    pub answered: bool,
    #[serde(default)]
    pub severity: Option<SemanticRecordSeverity>,
}

impl SemanticRecordMetadata {
    /// `answered` establishes human authorship exactly as `human` does: a user
    /// answered the question, so the record carries a person's decision.
    #[must_use]
    pub fn is_human(&self) -> bool {
        self.human || self.answered
    }
}

/// A malformed or conflicting semantic-record metadata suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMetadataSuffixError {
    detail: String,
}

impl SemanticMetadataSuffixError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for SemanticMetadataSuffixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for SemanticMetadataSuffixError {}

// ─── Structs ────────────────────────────────────────────────────────────────

/// A single block within a parsed spec, with inherited record metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockSpan {
    pub block_index: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub raw_content: String,
    /// Canonical I/D/Q record metadata inherited by this markdown block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_metadata: Option<SemanticRecordMetadata>,
}

/// Informational ontology format era detected from body-level markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyEra {
    V2,
    V4Candidate,
}

/// Informational v4 node class inferred from component path and body facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OntologyNodeClass {
    LegacyOrUnknown,
    SoftwareArchitecture,
    Concept,
}

/// I/D/Q label kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdBlockKind {
    Intent,
    Design,
    Question,
}

/// Whether a reference appears inline or only in the derived References index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferencePlacement {
    Inline,
    DerivedIndex,
}

/// Whether a reference targets a specific I/D/Q block or a whole node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSpecificity {
    Block,
    WholeNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdBlockFact {
    pub kind: IdBlockKind,
    pub raw_id: String,
    pub numeric_id: Option<u32>,
    pub title: String,
    pub label_start_offset: usize,
    pub label_end_offset: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub raw_text: String,
    pub block_indexes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFact {
    pub target_component: String,
    pub target_block: Option<String>,
    pub specificity: ReferenceSpecificity,
    pub placement: ReferencePlacement,
    pub owner_id: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
    pub line: usize,
    pub raw_text: String,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingFact {
    pub owner_id: Option<String>,
    pub attached_ref_indices: Vec<usize>,
    pub attached: bool,
    pub start_offset: usize,
    pub end_offset: usize,
    pub line: usize,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepFact {
    pub owner_id: Option<String>,
    pub row_number: Option<u32>,
    pub step: String,
    pub implemented_by: Vec<String>,
    pub touches: Vec<String>,
    pub raw_cells: Vec<String>,
    pub explicit_ref_syntax: bool,
    pub start_offset: usize,
    pub end_offset: usize,
    pub line: usize,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBranchFact {
    pub owner_id: Option<String>,
    pub branch: String,
    pub diverges_after: String,
    pub trigger: String,
    pub path: String,
    pub rejoin_terminal: String,
    pub raw_cells: Vec<String>,
    pub start_offset: usize,
    pub end_offset: usize,
    pub line: usize,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendsFact {
    pub owner_id: Option<String>,
    pub target: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
    pub line: usize,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeArtifactFact {
    pub owner_id: Option<String>,
    pub language: String,
    pub normativity: Option<String>,
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedReferenceIndexFact {
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub ref_indices: Vec<usize>,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyMarkupWarning {
    pub line: Option<usize>,
    pub start_offset: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyMarkupSummary {
    pub era: OntologyEra,
    pub node_class: OntologyNodeClass,
    pub id_block_count: usize,
    pub ref_count: usize,
    pub inline_ref_count: usize,
    pub derived_ref_count: usize,
    pub binding_count: usize,
    pub workflow_step_count: usize,
    pub workflow_branch_count: usize,
    pub extends_count: usize,
    pub native_artifact_count: usize,
    pub derived_reference_index_count: usize,
    pub warning_count: usize,
    pub warnings: Vec<OntologyMarkupWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyMarkupDocument {
    pub blocks: Vec<BlockSpan>,
    pub id_blocks: Vec<IdBlockFact>,
    pub refs: Vec<ReferenceFact>,
    pub bindings: Vec<BindingFact>,
    pub workflow_steps: Vec<WorkflowStepFact>,
    pub workflow_branches: Vec<WorkflowBranchFact>,
    pub extends: Vec<ExtendsFact>,
    pub native_artifacts: Vec<NativeArtifactFact>,
    pub derived_reference_indexes: Vec<DerivedReferenceIndexFact>,
    pub era: OntologyEra,
    pub node_class: OntologyNodeClass,
    pub warnings: Vec<OntologyMarkupWarning>,
}

impl OntologyMarkupDocument {
    #[must_use]
    pub fn summary(&self) -> OntologyMarkupSummary {
        OntologyMarkupSummary {
            era: self.era,
            node_class: self.node_class,
            id_block_count: self.id_blocks.len(),
            ref_count: self.refs.len(),
            inline_ref_count: self
                .refs
                .iter()
                .filter(|reference| reference.placement == ReferencePlacement::Inline)
                .count(),
            derived_ref_count: self
                .refs
                .iter()
                .filter(|reference| reference.placement == ReferencePlacement::DerivedIndex)
                .count(),
            binding_count: self.bindings.len(),
            workflow_step_count: self.workflow_steps.len(),
            workflow_branch_count: self.workflow_branches.len(),
            extends_count: self.extends.len(),
            native_artifact_count: self.native_artifacts.len(),
            derived_reference_index_count: self.derived_reference_indexes.len(),
            warning_count: self.warnings.len(),
            warnings: self.warnings.clone(),
        }
    }
}

/// Blocking status for a v4 lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyLintSeverity {
    Advisory,
    Blocking,
}

/// Stable Phase 1 ontology-v4 lint rule identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OntologyLintRuleId {
    ValidateReferences,
    RefQualityLint,
    DedupBarLint,
    AntiScaffoldingLint,
    RootRichnessGate,
    RefSetPreservationCheck,
    LegacyUncertaintyTagSpelling,
    NonAuthorableUncertaintyLevel,
    UnrecognizedBlockLabelKind,
    UppercaseLabelInsertionSuffix,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyLintLocation {
    pub component_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyLintDiagnostic {
    pub rule_id: OntologyLintRuleId,
    pub severity: OntologyLintSeverity,
    pub message: String,
    pub location: OntologyLintLocation,
    pub evidence: Vec<String>,
    pub related_locations: Vec<OntologyLintLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OntologyLintReport {
    pub valid: bool,
    pub diagnostics: Vec<OntologyLintDiagnostic>,
}

impl Default for OntologyLintReport {
    fn default() -> Self {
        Self {
            valid: true,
            diagnostics: Vec::new(),
        }
    }
}

/// A SemLift document participating in Phase 1 corpus-level linting.
pub struct OntologyLintInput<'a> {
    pub component_id: &'a str,
    pub content: &'a str,
}

#[derive(Debug, Clone)]
pub struct RootRichnessBaseline {
    pub original_byte_len: usize,
    pub required_markers: Vec<String>,
}

/// Tunables for deterministic ontology-v4 Phase 1 linting.
#[derive(Debug, Clone)]
pub struct OntologyLintOptions {
    pub dedup_min_group_size: usize,
    pub dedup_similarity_threshold: f32,
    pub dedup_max_token_doc_frequency: usize,
    pub dedup_min_candidate_shared_tokens: usize,
    pub root_min_retention_ratio: f32,
    pub root_baselines: BTreeMap<String, RootRichnessBaseline>,
    /// `component_id#LABEL` keys for migration-era uppercase-suffixed labels
    /// that are grandfathered live identities and must not be flagged.
    pub grandfathered_uppercase_labels: BTreeSet<String>,
    pub target_component_ids: Option<BTreeSet<String>>,
}

impl Default for OntologyLintOptions {
    fn default() -> Self {
        Self {
            dedup_min_group_size: 3,
            dedup_similarity_threshold: 0.8,
            dedup_max_token_doc_frequency: 100,
            dedup_min_candidate_shared_tokens: 3,
            root_min_retention_ratio: 0.5,
            root_baselines: BTreeMap::new(),
            grandfathered_uppercase_labels: BTreeSet::new(),
            target_component_ids: None,
        }
    }
}

impl OntologyLintOptions {
    #[must_use]
    pub fn scoped_to_component(component_id: impl Into<String>) -> Self {
        Self::default().with_target_components([component_id.into()])
    }

    #[must_use]
    pub fn with_target_components<I, S>(mut self, component_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let component_ids = component_ids
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<_>>();
        self.target_component_ids = Some(component_ids);
        self
    }
}

#[derive(Debug, Clone)]
struct ParsedLintDocument<'a> {
    component_id: &'a str,
    content: &'a str,
    document: OntologyMarkupDocument,
    is_file_spec: bool,
}

fn lint_component_in_scope(component_id: &str, options: &OntologyLintOptions) -> bool {
    options
        .target_component_ids
        .as_ref()
        .is_none_or(|component_ids| component_ids.contains(component_id))
}

fn lint_doc_in_scope(doc: &ParsedLintDocument<'_>, options: &OntologyLintOptions) -> bool {
    lint_component_in_scope(doc.component_id, options)
}

// ─── Tag Parsing Helpers ────────────────────────────────────────────────────

/// Canonical token order. A suffix must list its tags in this order to parse;
/// `(Tags: uncertain, human)` stays rejected, because one fact must have one
/// spelling or diffs and live workspace file splices fight over reorderings.
const SEMANTIC_TAG_ORDER: [&str; 4] = ["human", "uncertain", "answered", "severity"];

/// Parse a canonical semantic-record tag suffix.
///
/// Fail-closed on anything not exactly canonical: unknown tokens, duplicates,
/// out-of-order tokens, loose spacing, and `answered`/severity on a record that
/// is not a `Q`. `is_question` gates the Q-only vocabulary — on an I/D record
/// those tokens would claim metadata the record does not carry, so they are
/// rejected rather than ignored.
pub fn parse_semantic_record_metadata_suffix(
    raw_suffix: &str,
    is_question: bool,
) -> Result<SemanticRecordMetadata, SemanticMetadataSuffixError> {
    let body = raw_suffix
        .strip_prefix("(Tags: ")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            SemanticMetadataSuffixError::new(
                "expected a canonical `(Tags: ...)` suffix with exactly one space after the colon",
            )
        })?;

    let mut metadata = SemanticRecordMetadata {
        human: false,
        uncertain: false,
        answered: false,
        severity: None,
    };
    let mut last_order: isize = -1;
    for token in body.split(", ") {
        let (slot, apply): (&str, fn(&mut SemanticRecordMetadata, &str)) = match token {
            "human" => ("human", |m, _| m.human = true),
            "uncertain" => ("uncertain", |m, _| m.uncertain = true),
            "answered" => ("answered", |m, _| m.answered = true),
            other if SemanticRecordSeverity::from_token(other).is_some() => ("severity", |m, t| {
                m.severity = SemanticRecordSeverity::from_token(t);
            }),
            other => {
                return Err(SemanticMetadataSuffixError::new(format!(
                    "unknown semantic tag `{other}`; expected `human`, `uncertain`, or (on a Q record) `answered` and one of `low`/`medium`/`high`/`critical`"
                )));
            }
        };
        let question_only = slot == "answered" || slot == "severity";
        if question_only && !is_question {
            return Err(SemanticMetadataSuffixError::new(format!(
                "`{token}` is valid on Q records only"
            )));
        }
        let order = SEMANTIC_TAG_ORDER
            .iter()
            .position(|candidate| *candidate == slot)
            .expect("slot is one of the canonical tokens") as isize;
        if order <= last_order {
            return Err(SemanticMetadataSuffixError::new(
                "semantic tags must be canonically ordered `human, uncertain, answered, <severity>` with no duplicates",
            ));
        }
        last_order = order;
        apply(&mut metadata, token);
    }
    Ok(metadata)
}

/// Whether trailing label bytes attempt to use the semantic metadata syntax.
/// Candidate recognition is case-insensitive so malformed case variants are
/// rejected by every parser/validator instead of silently treated as prose.
#[must_use]
pub fn is_semantic_record_metadata_suffix_candidate(value: &str) -> bool {
    value
        .get(.."(Tags:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("(Tags:"))
}

// ─── Block Parsing ──────────────────────────────────────────────────────────

fn current_anchor_id(line: &str) -> Option<String> {
    static ANCHOR: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"^<a\s+(?:id|name)=["']([^"']+)["']\s*></a>$"#).unwrap()
    });
    ANCHOR.captures(line.trim()).map(|m| m[1].to_string())
}

fn current_identity_line(line: &str) -> bool {
    static MARKER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\[([IDQ]) · ([A-Za-z0-9][A-Za-z0-9._-]*)\]$").unwrap()
    });
    current_anchor_id(line).is_some() || MARKER.is_match(line.trim())
}

fn fence_run(line: &str) -> Option<(char, usize, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let fence_char = rest.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = rest.chars().take_while(|c| *c == fence_char).count();
    (run >= 3).then(|| (fence_char, run, &rest[run..]))
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    fence_run(line).map(|(fence_char, run, _)| (fence_char, run))
}

fn closes_fence(line: &str, fence_char: char, fence_len: usize) -> bool {
    fence_run(line).is_some_and(|(found, run, rest)| {
        found == fence_char && run >= fence_len && rest.trim().is_empty()
    })
}

/// The editor's `createMarkdownHeadingSlug`.
#[must_use]
pub fn markdown_heading_slug(content: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let folded: String = content
        .to_lowercase()
        .nfkd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect::<String>()
        .replace('&', " and ");
    let mut slug = String::with_capacity(folded.len());
    let mut dash = false;
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
            dash = false;
        } else if !dash {
            slug.push('-');
            dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

/// Navigation targets of a current-format page and how often each occurs,
/// like the editor's `ontologyAnchors`: explicit `<a id|name>` anchors outside
/// fences and inline code, plus generated heading IDs (duplicates numbered
/// `-2`, `-3`, …) up to the `## Metadata` footer. A fragment resolves only when
/// it occurs exactly once.
#[must_use]
pub fn current_ontology_anchor_counts(content: &str) -> HashMap<String, usize> {
    anchor_counts(&parse_blocks(content))
}

fn anchor_counts(blocks: &[BlockSpan]) -> HashMap<String, usize> {
    static IDS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)<a\b[^>]*?\s(?:id|name)\s*=\s*["']([^"']+)["'][^>]*>"#).unwrap()
    });
    static INLINE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"`+[^`]*`+").unwrap());
    let footer = blocks
        .iter()
        .position(|b| parse_atx_heading(b.raw_content.trim()) == Some((2, "Metadata")));
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut slugs: HashMap<String, usize> = HashMap::new();
    for (index, block) in blocks.iter().enumerate() {
        if let Some((_, text)) = parse_atx_heading(block.raw_content.trim()) {
            let base = match markdown_heading_slug(text) {
                slug if slug.is_empty() => format!("section-{index}"),
                slug => slug,
            };
            let n = slugs.entry(base.clone()).or_default();
            *n += 1;
            if footer.is_none_or(|footer| index <= footer) {
                let id = if *n > 1 { format!("{base}-{n}") } else { base };
                *counts.entry(id).or_default() += 1;
            }
            continue;
        }
        if opening_fence(&block.raw_content).is_some() {
            continue;
        }
        let visible = INLINE.replace_all(&block.raw_content, "");
        for m in IDS.captures_iter(&visible) {
            *counts.entry(m[1].to_string()).or_default() += 1;
        }
    }
    counts
}

/// Parse raw markdown content into a sequence of `BlockSpan` values.
///
/// Recognizes: frontmatter, ATX headings, code fences, bullet/numbered lists,
/// blockquotes, GFM tables, horizontal rules, and paragraphs.
///
/// Projects exact I/D/Q `(Tags: ...)` suffix metadata onto owned blocks.
/// Tracks byte offsets and block ordering.
#[must_use]
pub fn parse_blocks(content: &str) -> Vec<BlockSpan> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    let lines: Vec<&str> = content.split('\n').collect();
    let mut i = 0;
    let mut global_block_index: usize = 0;

    // Helper: compute byte offset for line index
    let line_offsets: Vec<usize> = {
        let mut offsets = Vec::with_capacity(lines.len());
        let mut off = 0;
        for (idx, line) in lines.iter().enumerate() {
            offsets.push(off);
            off += line.len();
            // Add 1 for newline unless it's the last line and content doesn't end with newline
            if idx < lines.len() - 1 || content.ends_with('\n') {
                off += 1;
            }
        }
        offsets
    };

    let get_offset = |line_idx: usize| -> usize {
        if line_idx < line_offsets.len() {
            line_offsets[line_idx]
        } else {
            content.len()
        }
    };

    // Handle frontmatter
    if lines.first().is_some_and(|l| l.trim() == "---") {
        let mut end = None;
        for (j, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                end = Some(j);
                break;
            }
        }
        if let Some(end_idx) = end {
            let start = get_offset(0);
            let end_off = get_offset(end_idx) + lines[end_idx].len();
            let raw = &content[start..end_off];
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end_off,
                raw_content: raw.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i = end_idx + 1;
        }
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Keep anchor and marker source ranges separate without requiring blank lines.
        if current_identity_line(trimmed) {
            let start = get_offset(i);
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: start + line.len(),
                raw_content: line.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i += 1;
            continue;
        }

        // Horizontal rule (---, ***, ___)
        if (trimmed == "---" || trimmed == "***" || trimmed == "___") && i > 0 {
            let start = get_offset(i);
            let end = start + line.len();
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end,
                raw_content: line.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i += 1;
            continue;
        }

        // ATX Heading
        if parse_atx_heading(trimmed).is_some() {
            let start = get_offset(i);
            let end = start + line.len();

            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end,
                raw_content: line.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i += 1;
            continue;
        }

        // Fenced code block, with the editor's rule: up to three spaces of
        // indent and three or more backticks or tildes; it closes only at a
        // line holding the same character at least as many times and nothing else.
        if let Some((fence_char, fence_len)) = opening_fence(line) {
            let start = get_offset(i);
            let mut j = i + 1;
            while j < lines.len() && !closes_fence(lines[j], fence_char, fence_len) {
                j += 1;
            }
            if j < lines.len() {
                j += 1; // include closing ```
            }
            let end = get_offset(j.min(lines.len()));
            // Trim trailing newline if we're at the end after the closing fence
            let end = if end > 0 && j <= lines.len() {
                // end should be the byte after the closing ``` line
                if j - 1 < lines.len() {
                    get_offset(j - 1) + lines[j - 1].len()
                } else {
                    end
                }
            } else {
                end
            };
            let raw = &content[start..end.min(content.len())];
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end.min(content.len()),
                raw_content: raw.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i = j;
            continue;
        }

        // GFM table
        if trimmed.contains('|') && i + 1 < lines.len() {
            let next_trimmed = lines[i + 1].trim();
            if is_table_separator(next_trimmed) {
                let start = get_offset(i);
                let mut j = i + 2; // skip header + separator
                while j < lines.len() && lines[j].trim().contains('|') {
                    j += 1;
                }
                let last_table_line = j - 1;
                let end = get_offset(last_table_line) + lines[last_table_line].len();
                let raw = &content[start..end.min(content.len())];
                blocks.push(BlockSpan {
                    block_index: global_block_index,
                    start_offset: start,
                    end_offset: end.min(content.len()),
                    raw_content: raw.to_string(),
                    semantic_metadata: None,
                });
                global_block_index += 1;
                i = j;
                continue;
            }
        }

        // Bullet list
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let start = get_offset(i);
            let mut j = i;
            while j < lines.len()
                && (lines[j].trim().starts_with("- ")
                    || lines[j].trim().starts_with("* ")
                    || (lines[j].starts_with("  ") && !lines[j].trim().is_empty()))
            {
                j += 1;
            }
            let last_line = j - 1;
            let end = get_offset(last_line) + lines[last_line].len();
            let raw = &content[start..end.min(content.len())];
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end.min(content.len()),
                raw_content: raw.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i = j;
            continue;
        }

        // Numbered list
        if is_numbered_list_start(trimmed) {
            let start = get_offset(i);
            let mut j = i;
            while j < lines.len()
                && (is_numbered_list_start(lines[j].trim())
                    || (lines[j].starts_with("  ") && !lines[j].trim().is_empty()))
            {
                j += 1;
            }
            let last_line = j - 1;
            let end = get_offset(last_line) + lines[last_line].len();
            let raw = &content[start..end.min(content.len())];
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end.min(content.len()),
                raw_content: raw.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i = j;
            continue;
        }

        // Blockquote
        if trimmed.starts_with("> ") || trimmed == ">" {
            let start = get_offset(i);
            let mut j = i;
            while j < lines.len() && (lines[j].trim().starts_with("> ") || lines[j].trim() == ">") {
                j += 1;
            }
            let last_line = j - 1;
            let end = get_offset(last_line) + lines[last_line].len();
            let raw = &content[start..end.min(content.len())];
            blocks.push(BlockSpan {
                block_index: global_block_index,
                start_offset: start,
                end_offset: end.min(content.len()),
                raw_content: raw.to_string(),
                semantic_metadata: None,
            });
            global_block_index += 1;
            i = j;
            continue;
        }

        // Paragraph (fallback)
        {
            let start = get_offset(i);
            let mut j = i;
            while j < lines.len()
                && (j == i || !current_identity_line(lines[j]))
                && !lines[j].trim().is_empty()
                && !lines[j].trim().starts_with('#')
                && opening_fence(lines[j]).is_none()
                && !lines[j].trim().starts_with("> ")
                && lines[j].trim() != ">"
                && !lines[j].trim().starts_with("- ")
                && !lines[j].trim().starts_with("* ")
                && !is_numbered_list_start(lines[j].trim())
                && !(lines[j].trim() == "---"
                    || lines[j].trim() == "***"
                    || lines[j].trim() == "___")
            {
                // Check if next line starts a table
                if lines[j].trim().contains('|')
                    && j + 1 < lines.len()
                    && is_table_separator(lines[j + 1].trim())
                {
                    break;
                }
                j += 1;
            }

            if j > i {
                let last_line = j - 1;
                let end = get_offset(last_line) + lines[last_line].len();
                let raw = &content[start..end.min(content.len())];
                blocks.push(BlockSpan {
                    block_index: global_block_index,
                    start_offset: start,
                    end_offset: end.min(content.len()),
                    raw_content: raw.to_string(),
                    semantic_metadata: None,
                });
                global_block_index += 1;
                i = j;
            } else {
                i += 1;
            }
        }
    }

    annotate_semantic_record_metadata(content, &mut blocks);
    blocks
}

/// Parse an ATX heading line. Returns (level, heading_text) or None.
fn parse_atx_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes, rest[1..].trim()))
}

/// Check if a line is a GFM table separator (e.g., `|---|---|`).
fn is_table_separator(line: &str) -> bool {
    if !line.contains('-') {
        return false;
    }
    let stripped: String = line
        .chars()
        .filter(|c| !matches!(c, '|' | '-' | ':' | ' '))
        .collect();
    stripped.is_empty()
}

/// Check if a line starts a numbered list item (e.g., `1. item`).
fn is_numbered_list_start(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(dot_pos) = trimmed.find(". ") {
        trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) && dot_pos > 0
    } else {
        false
    }
}

// ─── Ontology v4 Fact Parsing ──────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FenceZone {
    start_line: usize,
    start_offset: usize,
    end_offset: usize,
    language: String,
}

fn compute_line_offsets(lines: &[&str], content: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(lines.len());
    let mut off = 0;
    for (idx, line) in lines.iter().enumerate() {
        offsets.push(off);
        off += line.len();
        if idx < lines.len() - 1 || content.ends_with('\n') {
            off += 1;
        }
    }
    offsets
}

fn line_end_offset(lines: &[&str], offsets: &[usize], idx: usize) -> usize {
    offsets
        .get(idx)
        .copied()
        .map(|start| start + lines[idx].len())
        .unwrap_or(0)
}

fn mark_frontmatter_exclusions(lines: &[&str], excluded: &mut [bool]) {
    if lines.first().is_none_or(|line| line.trim() != "---") {
        return;
    }
    if let Some(end_idx) = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(idx, line)| (line.trim() == "---").then_some(idx))
    {
        for item in excluded.iter_mut().take(end_idx + 1) {
            *item = true;
        }
    }
}

fn collect_fence_zones(
    lines: &[&str],
    offsets: &[usize],
    content: &str,
    excluded: &mut [bool],
) -> Vec<FenceZone> {
    let mut zones = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if !trimmed.starts_with("```") {
            i += 1;
            continue;
        }

        let language = trimmed
            .trim_start_matches("```")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let start_line = i;
        let mut end_line = i;
        i += 1;
        while i < lines.len() {
            end_line = i;
            if lines[i].trim().starts_with("```") {
                i += 1;
                break;
            }
            i += 1;
        }
        for item in excluded.iter_mut().take(end_line + 1).skip(start_line) {
            *item = true;
        }
        zones.push(FenceZone {
            start_line,
            start_offset: offsets[start_line],
            end_offset: line_end_offset(lines, offsets, end_line).min(content.len()),
            language,
        });
    }
    zones
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedIdLabel {
    kind: IdBlockKind,
    raw_id: String,
    numeric_id: Option<u32>,
    title: String,
    label_start_col: usize,
    label_end_col: usize,
    heading_level: Option<usize>,
}

fn parse_id_label(line: &str) -> Option<ParsedIdLabel> {
    let leading_ws = line.len() - line.trim_start().len();
    let mut label_start_col = leading_ws;
    let mut label_source = line.trim_start();
    let heading_level = if label_source.starts_with('[') {
        None
    } else {
        let hashes = label_source.bytes().take_while(|&b| b == b'#').count();
        if hashes == 0 || hashes > 6 {
            return None;
        }
        let after_hashes = &label_source[hashes..];
        let after_first_space = after_hashes.strip_prefix(' ')?;
        let extra_spaces = after_first_space.len() - after_first_space.trim_start().len();
        let after_heading_prefix = after_first_space.trim_start();
        if !after_heading_prefix.starts_with('[') {
            return None;
        }
        label_start_col += hashes + 1 + extra_spaces;
        label_source = after_heading_prefix;
        Some(hashes)
    };
    let close = label_source.find(']')?;
    let inside = &label_source[1..close];
    let first = inside.chars().next()?;
    let kind = match first {
        'I' => IdBlockKind::Intent,
        'D' => IdBlockKind::Design,
        'Q' => IdBlockKind::Question,
        _ => return None,
    };
    let (raw_id, title) = if let Some(dot) = inside.find('.') {
        (
            inside[..dot].trim().to_string(),
            inside[dot + 1..].trim().to_string(),
        )
    } else {
        (inside.trim().to_string(), String::new())
    };
    if raw_id.len() < 2 {
        return None;
    }
    let numeric_id = raw_id[1..].parse::<u32>().ok();
    Some(ParsedIdLabel {
        kind,
        raw_id,
        numeric_id,
        title,
        label_start_col,
        label_end_col: label_start_col + close + 1,
        heading_level,
    })
}

fn semantic_record_metadata_ranges(content: &str) -> Vec<(usize, usize, SemanticRecordMetadata)> {
    let lines: Vec<&str> = content.split('\n').collect();
    let offsets = compute_line_offsets(&lines, content);
    let mut excluded = vec![false; lines.len()];
    mark_frontmatter_exclusions(&lines, &mut excluded);
    let _ = collect_fence_zones(&lines, &offsets, content, &mut excluded);

    let headings = lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !excluded[*idx])
        .filter_map(|(idx, line)| {
            parse_atx_heading(line.trim()).map(|(level, title)| (idx, level, title.to_string()))
        })
        .collect::<Vec<_>>();
    let labels = lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| !excluded[*idx])
        .filter_map(|(idx, line)| parse_id_label(line).map(|label| (idx, label)))
        .collect::<Vec<_>>();

    labels
        .iter()
        .enumerate()
        .filter_map(|(label_idx, (line_idx, label))| {
            let suffix = lines[*line_idx]
                .get(label.label_end_col..)
                .unwrap_or_default()
                .trim();
            // A bare Q label still gets a range: its `answered` state is
            // derived from the PAYLOAD at annotation time, so a real user
            // answer counts as human authority even before the card backfills
            // the label token (the label is only an index). Non-Q records with
            // no suffix carry no metadata at all.
            let metadata = if suffix.is_empty() {
                if label.kind != IdBlockKind::Question {
                    return None;
                }
                SemanticRecordMetadata {
                    human: false,
                    uncertain: false,
                    answered: false,
                    severity: None,
                }
            } else {
                parse_semantic_record_metadata_suffix(suffix, label.kind == IdBlockKind::Question)
                    .ok()?
            };
            let section_level = label.heading_level.unwrap_or_else(|| {
                headings
                    .iter()
                    .rev()
                    // H1 is the document title, not a record-owning section.
                    .find(|(heading_line, level, _)| heading_line < line_idx && *level >= 2)
                    .map(|(_, level, _)| *level)
                    .unwrap_or(0)
            });
            let next_label_line = labels
                .get(label_idx + 1)
                .map(|(idx, _)| *idx)
                .unwrap_or(lines.len());
            let next_section_line = headings
                .iter()
                .find(|(heading_line, level, _)| {
                    heading_line > line_idx && (section_level == 0 || *level <= section_level)
                })
                .map(|(idx, _, _)| *idx)
                .unwrap_or(lines.len());
            let end_line_exclusive = next_label_line.min(next_section_line).min(lines.len());
            let start_offset = offsets[*line_idx];
            let end_offset = if end_line_exclusive < lines.len() {
                offsets[end_line_exclusive]
            } else {
                content.len()
            };
            Some((start_offset, end_offset, metadata))
        })
        .collect()
}

fn annotate_semantic_record_metadata(content: &str, blocks: &mut [BlockSpan]) {
    for (start_offset, end_offset, mut metadata) in semantic_record_metadata_ranges(content) {
        // `answered` is VALIDATED against the record's payload before any
        // consumer sees it (D3: the token alone must not grant authority).
        // A label-only `answered` over an open, agent, missing, malformed, or
        // oversized payload is downgraded here, so `is_human()` — and the
        // public human-blocks API built on `extract_tagged_blocks` — cannot
        // expose unbacked authority. The payload is equally authoritative in
        // the other direction: a real user answer whose label has not been
        // backfilled yet still counts, because the label is only an index.
        let range_content = content.get(start_offset..end_offset).unwrap_or_default();
        metadata.answered = payload_records_user_answer(range_content);
        for block in blocks
            .iter_mut()
            .filter(|block| block.start_offset >= start_offset && block.start_offset < end_offset)
        {
            block.semantic_metadata = Some(metadata.clone());
        }
    }
}

fn current_owner_id(id_blocks: &[IdBlockFact], offset: usize) -> Option<String> {
    id_blocks
        .iter()
        .find(|block| block.start_offset <= offset && offset < block.end_offset)
        .map(|block| block.raw_id.clone())
}

fn find_owner_index(id_blocks: &[IdBlockFact], offset: usize) -> Option<usize> {
    id_blocks
        .iter()
        .position(|block| block.start_offset <= offset && offset < block.end_offset)
}

fn classify_node(component_id: &str) -> OntologyNodeClass {
    let first_segment = component_id.split('/').next().unwrap_or(component_id);
    if component_id == "software_architecture" || component_id.starts_with("software_architecture/")
    {
        return OntologyNodeClass::SoftwareArchitecture;
    }
    const CONCEPT_ROOTS: &[&str] = &[
        "data_model",
        "core_workflow",
        "business_rule_model",
        "api_contract",
        "security_policy",
        "operation_policy",
        "system_invariants",
        "integration_model",
        "user_experience_model",
        "testing_policy",
        "performance_requirement",
        "deployment_model",
        "ci_cd_pipeline",
    ];
    if CONCEPT_ROOTS.contains(&first_segment) {
        OntologyNodeClass::Concept
    } else {
        OntologyNodeClass::LegacyOrUnknown
    }
}

fn trim_ref_syntax(target: &str) -> &str {
    target
        .trim()
        .trim_matches(|c: char| matches!(c, '`' | ',' | '.' | ')' | '(' | ';' | ':'))
}

fn parse_ref_target(target: &str) -> Option<(String, Option<String>)> {
    let cleaned = trim_ref_syntax(target);
    if cleaned.is_empty() || cleaned.starts_with('#') {
        return None;
    }
    if let Some((component, block)) = cleaned.split_once('#') {
        if !is_valid_ref_component(component) || !is_valid_ref_block(block) {
            return None;
        }
        Some((component.to_string(), Some(block.to_string())))
    } else {
        if !is_valid_ref_component(cleaned) {
            return None;
        }
        Some((cleaned.to_string(), None))
    }
}

fn is_valid_ref_component(component: &str) -> bool {
    !component.is_empty()
        && component.chars().any(|ch| ch.is_ascii_alphanumeric())
        && component
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.'))
}

fn is_valid_ref_block(block: &str) -> bool {
    !block.is_empty()
        && block.chars().any(|ch| ch.is_ascii_alphanumeric())
        && block
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn token_end(line: &str, start: usize) -> usize {
    let mut end = start;
    for (rel, ch) in line[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '`' | ',' | ')' | '(' | ';') {
            break;
        }
        end = start + rel + ch.len_utf8();
    }
    end
}

fn rationale_tail(line: &str, start: usize) -> Option<String> {
    let tail = line[start..]
        .trim()
        .trim_start_matches([',', '-', ':', ')', '(', ';', '`'])
        .trim();
    rationale_tail_has_prose(tail).then(|| tail.to_string())
}

fn rationale_tail_has_prose(tail: &str) -> bool {
    let mut without_refs = String::new();
    let mut cursor = 0;
    while let Some(rel) = tail[cursor..].find("@ref") {
        without_refs.push_str(&tail[cursor..cursor + rel]);
        let after_ref = cursor + rel + "@ref".len();
        let token_start =
            after_ref + tail[after_ref..].len() - tail[after_ref..].trim_start().len();
        cursor = token_end(tail, token_start).min(tail.len());
    }
    without_refs.push_str(&tail[cursor..]);

    without_refs
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .map(str::to_ascii_lowercase)
        .any(|token| {
            !matches!(
                token.as_str(),
                "and"
                    | "also"
                    | "because"
                    | "for"
                    | "with"
                    | "from"
                    | "the"
                    | "this"
                    | "that"
                    | "into"
                    | "ref"
            )
        })
}

struct ShorthandRefParseContext<'a> {
    component: &'a str,
    placement: ReferencePlacement,
    owner_id: Option<String>,
    line_start_offset: usize,
    line_number: usize,
}

fn parse_shorthand_refs(
    line: &str,
    search_start: usize,
    search_end: usize,
    context: &ShorthandRefParseContext<'_>,
    refs: &mut Vec<ReferenceFact>,
) {
    let mut cursor = search_start;
    let search_end = search_end.min(line.len());
    while cursor < search_end {
        let Some(rel) = line[cursor..search_end].find('#') else {
            break;
        };
        let start = cursor + rel;
        let rest = &line[start + 1..];
        let Some(first) = rest.chars().next() else {
            break;
        };
        if !matches!(first, 'I' | 'D' | 'Q') {
            cursor = start + 1;
            continue;
        }
        let mut end = start + 1;
        for (rel, ch) in rest.char_indices() {
            if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
                break;
            }
            end = start + 1 + rel + ch.len_utf8();
        }
        let raw_text = &line[start..end];
        refs.push(ReferenceFact {
            target_component: context.component.to_string(),
            target_block: Some(raw_text.trim_start_matches('#').to_string()),
            specificity: ReferenceSpecificity::Block,
            placement: context.placement,
            owner_id: context.owner_id.clone(),
            start_offset: context.line_start_offset + start,
            end_offset: context.line_start_offset + end,
            line: context.line_number,
            raw_text: raw_text.to_string(),
            rationale: None,
        });
        cursor = end;
    }
}

fn is_supported_native_language(language: &str) -> bool {
    matches!(
        language,
        "dbml" | "sql" | "mermaid" | "gherkin" | "rego" | "cedar" | "yaml" | "yml" | "hcl"
    )
}

fn floor_char_boundary(content: &str, index: usize) -> usize {
    let mut index = index.min(content.len());
    while index > 0 && !content.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(content: &str, index: usize) -> usize {
    let mut index = index.min(content.len());
    while index < content.len() && !content.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn detect_normativity_near(content: &str, start: usize, end: usize) -> Option<String> {
    let before_start = floor_char_boundary(content, start.saturating_sub(500));
    let after_end = ceil_char_boundary(content, end.saturating_add(500));
    let context = content[before_start..after_end].to_ascii_lowercase();
    for marker in ["normative", "explanatory", "derived"] {
        if context.contains(marker) {
            return Some(marker.to_string());
        }
    }
    None
}

fn split_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().trim_matches('`').to_string())
        .collect()
}

fn looks_like_target(token: &str) -> bool {
    let cleaned = trim_ref_syntax(token);
    if cleaned.is_empty() || cleaned.contains(' ') {
        return false;
    }
    cleaned.contains('/')
        || cleaned.contains('#')
        || classify_node(cleaned) == OntologyNodeClass::Concept
}

fn extract_cell_targets(cell: &str) -> (Vec<String>, bool) {
    let mut targets = Vec::new();
    let mut explicit = false;
    let mut cursor = 0;
    while let Some(rel) = cell[cursor..].find("@ref") {
        explicit = true;
        let after_ref = cursor + rel + "@ref".len();
        let token_start =
            after_ref + cell[after_ref..].len() - cell[after_ref..].trim_start().len();
        let end = token_end(cell, token_start);
        if let Some((component, block)) = parse_ref_target(&cell[token_start..end]) {
            targets.push(match block {
                Some(block) => format!("{component}#{block}"),
                None => component,
            });
        }
        cursor = end;
    }

    for part in cell.split([',', '→']) {
        let target = trim_ref_syntax(part);
        if looks_like_target(target) {
            targets.push(target.to_string());
        }
    }
    targets.sort();
    targets.dedup();
    (targets, explicit)
}

fn parse_extends_target(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let pos = lower.find("extends")?;
    let start = pos + "extends".len();
    let mut token_start = start + line[start..].len() - line[start..].trim_start().len();
    while token_start < line.len() && line[token_start..].starts_with('`') {
        token_start += 1;
    }
    if !line[token_start..].starts_with("@ref") {
        return None;
    }
    token_start += "@ref".len();
    token_start += line[token_start..].len() - line[token_start..].trim_start().len();
    while token_start < line.len() && line[token_start..].starts_with('`') {
        token_start += 1;
    }
    let end = token_end(line, token_start);
    let token = line[token_start..end].trim();
    parse_ref_target(token).map(|(component, block)| {
        let target = match block {
            Some(block) => format!("{component}#{block}"),
            None => component,
        };
        target
    })
}

fn in_level_two_section(section_stack: &[(usize, String)], title: &str) -> bool {
    section_stack
        .iter()
        .rev()
        .any(|(level, section)| *level == 2 && section == title)
}

/// One duplicate id-block occurrence inside a single document: `block_id` was
/// first defined at `first_line` and defined again at `duplicate_line`.
/// Line numbers are 1-based, matching `IdBlockFact::start_line`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateIdBlock {
    pub block_id: String,
    pub first_line: usize,
    pub duplicate_line: usize,
}

/// Document-local duplicate id-block scan over an already-parsed document.
///
/// Groups `id_blocks` by raw id in document order and reports every
/// re-definition. Bare `[Dn]` brackets at the start of a line are legitimate
/// block-definition syntax, so a wrapped prose reference that lands at a line
/// start silently defines a duplicate (or phantom) block — the semantic
/// resolver treats that as a blocking `duplicate_block_id`, and validation
/// surfaces the same fact through this helper. O(blocks); no corpus access.
#[must_use]
pub fn duplicate_id_blocks(document: &OntologyMarkupDocument) -> Vec<DuplicateIdBlock> {
    let mut first_seen: HashMap<&str, usize> = HashMap::new();
    let mut duplicates = Vec::new();
    for block in &document.id_blocks {
        match first_seen.get(block.raw_id.as_str()) {
            Some(first_line) => duplicates.push(DuplicateIdBlock {
                block_id: block.raw_id.clone(),
                first_line: *first_line,
                duplicate_line: block.start_line,
            }),
            None => {
                first_seen.insert(block.raw_id.as_str(), block.start_line);
            }
        }
    }
    duplicates
}

/// Current ontology facts. Offsets are UTF-8 byte offsets into authored source.
/// Metadata is paired by exact ID; it is never interpreted as an approval.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentOntologyRecord {
    pub kind: IdBlockKind,
    pub id: String,
    pub heading: Option<usize>,
    pub anchor: Option<usize>,
    /// Alias anchors directly above `anchor` (former IDs), and the first one's block.
    pub aliases: Vec<String>,
    pub alias_start: Option<usize>,
    pub marker: usize,
    pub end: usize,
    pub metadata: Option<(usize, usize)>,
    pub nested: bool,
    pub diagnostic: Option<String>,
}

pub fn current_ontology_records(content: &str) -> Vec<CurrentOntologyRecord> {
    let blocks = parse_blocks(content);
    // Index headings, anchors and footer entries once. Comparison parses files
    // with thousands of statements; rescanning the document per statement made
    // capped inputs quadratic.
    let headings: Vec<Option<(usize, &str)>> = blocks
        .iter()
        .map(|b| parse_atx_heading(b.raw_content.trim()))
        .collect();
    let heading = |i: usize| headings.get(i).copied().flatten();
    let limit = (0..blocks.len())
        .find(|&i| heading(i) == Some((2, "Metadata")))
        .unwrap_or(blocks.len());
    let metadata_limit = ((limit + 1)..blocks.len())
        .find(|&j| heading(j).is_some_and(|(l, _)| l <= 2))
        .unwrap_or(blocks.len());
    let marker_re = regex::Regex::new(r"^\[([IDQ]) · ([A-Za-z0-9][A-Za-z0-9._-]*)\]$").unwrap();
    let candidates: Vec<_> = blocks
        .iter()
        .enumerate()
        .take(limit)
        .filter_map(|(i, b)| {
            let m = marker_re.captures(b.raw_content.trim())?;
            let kind = match &m[1] {
                "I" => IdBlockKind::Intent,
                "D" => IdBlockKind::Design,
                _ => IdBlockKind::Question,
            };
            let anchor = i
                .checked_sub(1)
                .filter(|&a| current_anchor_id(&blocks[a].raw_content).is_some());
            // Consecutive anchor-only blocks above the own anchor are aliases.
            let mut aliases = Vec::new();
            if let Some(anchor) = anchor {
                let mut a = anchor;
                while let Some(prev) = a.checked_sub(1) {
                    if current_anchor_id(&blocks[prev].raw_content).is_none() {
                        break;
                    }
                    aliases.insert(0, prev);
                    a = prev;
                }
            }
            Some((
                i,
                i.checked_sub(1).filter(|&h| heading(h).is_some()),
                kind,
                m[2].to_string(),
                anchor,
                aliases,
            ))
        })
        .collect();
    let mut id_counts: HashMap<&str, usize> = HashMap::new();
    for (_, _, _, id, _, _) in &candidates {
        *id_counts.entry(id.as_str()).or_default() += 1;
    }
    let anchor_counts = anchor_counts(&blocks);
    // Footer entries by ID, each ending at the next heading of level <= 3.
    let mut entries_by_id: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut entry_ends: HashMap<usize, usize> = HashMap::new();
    let mut next_entry_heading = metadata_limit;
    for j in ((limit + 1)..metadata_limit).rev() {
        if let Some((level, text)) = heading(j) {
            if level <= 3 {
                entry_ends.insert(j, next_entry_heading);
                next_entry_heading = j;
                if level == 3 {
                    entries_by_id.entry(text).or_default().push(j);
                }
            }
        }
    }
    // For each candidate, the next heading at or above each level before `limit`.
    let candidate_indices: HashSet<usize> = candidates.iter().map(|c| c.0).collect();
    let mut section_ends: HashMap<usize, [usize; 7]> = HashMap::new();
    let mut next_heading = [limit; 7];
    for i in (0..limit).rev() {
        if candidate_indices.contains(&i) {
            section_ends.insert(i, next_heading);
        }
        if let Some((level, _)) = heading(i) {
            for slot in next_heading.iter_mut().skip(level) {
                *slot = i;
            }
        }
    }
    candidates
        .iter()
        .enumerate()
        .map(|(n, (i, h, kind, id, anchor, alias_blocks))| {
            let level = h
                .and_then(|h| heading(h).map(|(level, _)| level))
                .unwrap_or(6);
            let section_end = section_ends[i][level];
            let end = candidates
                .get(n + 1)
                .map(|(i, h, _, _, a, aliases)| {
                    aliases.first().copied().or(*a).or(*h).unwrap_or(*i)
                })
                .unwrap_or(limit)
                .min(section_end);
            let entries = entries_by_id
                .get(id.as_str())
                .map_or(&[][..], Vec::as_slice);
            let metadata = (entries.len() == 1).then(|| {
                let a = entries[0];
                (a, entry_ends.get(&a).copied().unwrap_or(metadata_limit))
            });
            let aliases: Vec<String> = alias_blocks
                .iter()
                .filter_map(|&a| current_anchor_id(&blocks[a].raw_content))
                .collect();
            let diagnostic = if anchor.is_some_and(|a| {
                current_anchor_id(&blocks[a].raw_content).as_ref() != Some(id)
                    || anchor_counts.get(id.as_str()).copied() != Some(1)
                    || aliases.iter().any(|alias| {
                        anchor_counts.get(alias.as_str()).copied() != Some(1)
                            || id_counts.contains_key(alias.as_str())
                    })
            }) {
                Some("Mismatched or ambiguous statement anchor")
            } else if h.is_none() && anchor.is_none() {
                Some("Statement is missing its authored anchor or heading")
            } else if id_counts.get(id.as_str()).copied() != Some(1) {
                Some("Duplicate statement ID")
            } else if metadata.is_none() {
                Some("Missing or ambiguous footer metadata")
            } else if metadata_limit < blocks.len() {
                Some("Metadata must be the final section")
            } else {
                None
            };
            CurrentOntologyRecord {
                kind: *kind,
                id: id.clone(),
                heading: *h,
                anchor: *anchor,
                aliases,
                alias_start: alias_blocks.first().copied(),
                marker: *i,
                end,
                metadata,
                // Same rule as the editor's `currentRecordSpans`: a heading-owned
                // statement is ambiguous only when the next statement has no
                // anchor and starts inside its section.
                nested: anchor.is_none()
                    && h.is_some()
                    && candidates
                        .get(n + 1)
                        .is_some_and(|(j, _, _, _, a, _)| a.is_none() && *j < section_end),
                diagnostic: diagnostic.map(str::to_string),
            }
        })
        .collect()
}

/// Parse raw ontology markdown into existing block spans plus additive v4 facts.
#[must_use]
pub fn parse_ontology_document(component_id: &str, content: &str) -> OntologyMarkupDocument {
    let blocks = parse_blocks(content);
    let current = current_ontology_records(content);
    if component_id.starts_with(".g5/ontology/") || !current.is_empty() {
        let mut warnings: Vec<OntologyMarkupWarning> = current
            .iter()
            .filter_map(|r| {
                r.diagnostic.as_ref().map(|d| OntologyMarkupWarning {
                    line: None,
                    start_offset: Some(blocks[r.marker].start_offset),
                    message: format!("{}: {}", r.id, d),
                })
            })
            .collect();
        let marker = regex::Regex::new(r"^\[([IDQ]) · ([A-Za-z0-9][A-Za-z0-9._-]*)\]$").unwrap();
        let malformed_marker = regex::Regex::new(r"^\[[IDQ]\s*·").unwrap();
        let mut in_metadata = false;
        for block in &blocks {
            let raw = block.raw_content.trim();
            if parse_atx_heading(raw) == Some((2, "Metadata")) {
                in_metadata = true;
            }
            let malformed = malformed_marker.is_match(raw) && !marker.is_match(raw);
            let orphan = in_metadata
                && parse_atx_heading(raw).is_some_and(|(level, id)| {
                    level == 3 && current.iter().filter(|r| r.id == id).count() != 1
                });
            if malformed || orphan {
                warnings.push(OntologyMarkupWarning {
                    line: None,
                    start_offset: Some(block.start_offset),
                    message: if malformed {
                        "Malformed statement marker; repair in Files or with an agent."
                    } else {
                        "Orphan footer metadata; repair in Files or with an agent."
                    }
                    .into(),
                });
            }
        }
        let id_blocks = current
            .iter()
            .map(|r| {
                let marker = &blocks[r.marker];
                let end_offset = blocks
                    .get(r.end)
                    .map(|b| b.start_offset)
                    .unwrap_or(content.len());
                IdBlockFact {
                    kind: r.kind,
                    raw_id: r.id.clone(),
                    numeric_id: None,
                    title: String::new(),
                    label_start_offset: marker.start_offset,
                    label_end_offset: marker.end_offset,
                    start_offset: marker.start_offset,
                    end_offset,
                    start_line: content[..marker.start_offset]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1,
                    raw_text: content[marker.start_offset..end_offset].to_string(),
                    block_indexes: (r.marker..r.end).collect(),
                }
            })
            .collect();
        return OntologyMarkupDocument {
            blocks,
            id_blocks,
            refs: vec![],
            bindings: vec![],
            workflow_steps: vec![],
            workflow_branches: vec![],
            extends: vec![],
            native_artifacts: vec![],
            derived_reference_indexes: vec![],
            era: OntologyEra::V4Candidate,
            node_class: OntologyNodeClass::Concept,
            warnings,
        };
    }

    let lines: Vec<&str> = content.split('\n').collect();
    let offsets = compute_line_offsets(&lines, content);
    let mut excluded = vec![false; lines.len()];
    mark_frontmatter_exclusions(&lines, &mut excluded);
    let fence_zones = collect_fence_zones(&lines, &offsets, content, &mut excluded);

    let mut headings: Vec<(usize, usize, String)> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if excluded[idx] {
            continue;
        }
        if let Some((level, title)) = parse_atx_heading(line.trim()) {
            headings.push((idx, level, title.to_string()));
        }
    }

    let mut label_lines = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if excluded[idx] {
            continue;
        }
        if let Some(label) = parse_id_label(line) {
            label_lines.push((idx, label));
        }
    }

    let mut id_blocks = Vec::new();
    for (label_idx, (line_idx, label)) in label_lines.iter().enumerate() {
        let section_level = label.heading_level.unwrap_or_else(|| {
            headings
                .iter()
                .rev()
                .find(|(heading_line, _, _)| heading_line < line_idx)
                .map(|(_, level, _)| *level)
                .unwrap_or(0)
        });
        let next_label_line = label_lines
            .get(label_idx + 1)
            .map(|(idx, _)| *idx)
            .unwrap_or(lines.len());
        let next_section_line = headings
            .iter()
            .find(|(heading_line, level, _)| {
                heading_line > line_idx && (section_level == 0 || *level <= section_level)
            })
            .map(|(idx, _, _)| *idx)
            .unwrap_or(lines.len());
        let end_line_exclusive = next_label_line.min(next_section_line).min(lines.len());
        let start_offset = offsets[*line_idx];
        let end_offset = if end_line_exclusive < lines.len() {
            offsets[end_line_exclusive]
        } else {
            content.len()
        };
        let block_indexes = blocks
            .iter()
            .filter(|block| block.start_offset >= start_offset && block.start_offset < end_offset)
            .map(|block| block.block_index)
            .collect();
        id_blocks.push(IdBlockFact {
            kind: label.kind,
            raw_id: label.raw_id.clone(),
            numeric_id: label.numeric_id,
            title: label.title.clone(),
            label_start_offset: offsets[*line_idx] + label.label_start_col,
            label_end_offset: offsets[*line_idx] + label.label_end_col,
            start_offset,
            end_offset,
            start_line: line_idx + 1,
            raw_text: content[start_offset..end_offset].to_string(),
            block_indexes,
        });
    }

    let mut refs = Vec::new();
    let mut bindings = Vec::new();
    let mut workflow_steps = Vec::new();
    let mut workflow_branches = Vec::new();
    let mut extends = Vec::new();
    let mut derived_reference_indexes = Vec::new();
    let mut warnings = Vec::new();
    let mut section_stack: Vec<(usize, String)> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_offset = offsets.get(idx).copied().unwrap_or(content.len());
        let line_number = idx + 1;
        if !excluded[idx] {
            if let Some((level, title)) = parse_atx_heading(line.trim()) {
                section_stack.retain(|(existing_level, _)| *existing_level < level);
                section_stack.push((level, title.to_string()));
            }
        }
        let in_references = !excluded[idx] && in_level_two_section(&section_stack, "References");
        let in_steps = !excluded[idx] && in_level_two_section(&section_stack, "Steps");
        let in_branches = !excluded[idx] && in_level_two_section(&section_stack, "Branches");
        let owner_id = current_owner_id(&id_blocks, line_offset);

        if !excluded[idx] {
            let placement = if in_references {
                ReferencePlacement::DerivedIndex
            } else {
                ReferencePlacement::Inline
            };
            let refs_before_line = refs.len();
            let mut cursor = 0;
            while let Some(rel) = line[cursor..].find("@ref") {
                let ref_start = cursor + rel;
                let after_ref = ref_start + "@ref".len();
                if !line[after_ref..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
                {
                    cursor = after_ref;
                    continue;
                }
                let target_start =
                    after_ref + line[after_ref..].len() - line[after_ref..].trim_start().len();
                let target_end = token_end(line, target_start);
                if target_start == target_end {
                    warnings.push(OntologyMarkupWarning {
                        line: Some(line_number),
                        start_offset: Some(line_offset + ref_start),
                        message: "Malformed @ref with no target".to_string(),
                    });
                    cursor = after_ref;
                    continue;
                }
                if let Some((component, block)) = parse_ref_target(&line[target_start..target_end])
                {
                    let specificity = if block.is_some() {
                        ReferenceSpecificity::Block
                    } else {
                        ReferenceSpecificity::WholeNode
                    };
                    refs.push(ReferenceFact {
                        target_component: component.clone(),
                        target_block: block,
                        specificity,
                        placement,
                        owner_id: owner_id.clone(),
                        start_offset: line_offset + ref_start,
                        end_offset: line_offset + target_end,
                        line: line_number,
                        raw_text: line[ref_start..target_end].to_string(),
                        rationale: rationale_tail(line, target_end),
                    });
                    if in_references {
                        let context = ShorthandRefParseContext {
                            component: &component,
                            placement,
                            owner_id: owner_id.clone(),
                            line_start_offset: line_offset,
                            line_number,
                        };
                        let search_end = line[target_end..]
                            .find("@ref")
                            .map_or(line.len(), |rel| target_end + rel);
                        parse_shorthand_refs(line, target_end, search_end, &context, &mut refs);
                    }
                }
                cursor = target_end;
            }
            if in_references && refs.len() > refs_before_line {
                derived_reference_indexes.push(DerivedReferenceIndexFact {
                    start_offset: line_offset,
                    end_offset: line_end_offset(&lines, &offsets, idx).min(content.len()),
                    start_line: line_number,
                    ref_indices: (refs_before_line..refs.len()).collect(),
                    raw_text: (*line).to_string(),
                });
            }

            if !in_references && owner_id.is_some() {
                if let Some(binding_pos) = line.find("**Binding:**") {
                    let binding_start = line_offset + binding_pos;
                    let candidates = refs
                        .iter()
                        .enumerate()
                        .filter(|(_, reference)| {
                            reference.placement == ReferencePlacement::Inline
                                && reference.owner_id == owner_id
                                && reference.end_offset <= binding_start
                        })
                        .collect::<Vec<_>>();
                    let nearest_line = candidates.iter().map(|(_, reference)| reference.line).max();
                    let attached_ref_indices = candidates
                        .into_iter()
                        .filter(|(_, reference)| Some(reference.line) == nearest_line)
                        .map(|(idx, _)| idx)
                        .collect::<Vec<_>>();
                    if attached_ref_indices.is_empty() {
                        warnings.push(OntologyMarkupWarning {
                        line: Some(line_number),
                        start_offset: Some(line_offset + binding_pos),
                        message: "Binding has no preceding authoritative inline @ref in the same I/D/Q block"
                            .to_string(),
                    });
                    }
                    bindings.push(BindingFact {
                        owner_id: owner_id.clone(),
                        attached: !attached_ref_indices.is_empty(),
                        attached_ref_indices,
                        start_offset: line_offset + binding_pos,
                        end_offset: line_end_offset(&lines, &offsets, idx).min(content.len()),
                        line: line_number,
                        raw_text: line[binding_pos..].to_string(),
                    });
                }
            }

            if owner_id.is_some() && line.to_ascii_lowercase().contains("extends") {
                extends.push(ExtendsFact {
                    owner_id: owner_id.clone(),
                    target: parse_extends_target(line),
                    start_offset: line_offset,
                    end_offset: line_end_offset(&lines, &offsets, idx).min(content.len()),
                    line: line_number,
                    raw_text: (*line).to_string(),
                });
            }

            if in_steps && line.trim().starts_with('|') {
                let cells = split_table_row(line);
                if cells.len() >= 4
                    && !is_table_separator(line.trim())
                    && cells[0].parse::<u32>().is_ok()
                {
                    let (implemented_by, explicit_impl) = extract_cell_targets(&cells[2]);
                    let (touches, explicit_touches) = extract_cell_targets(&cells[3]);
                    workflow_steps.push(WorkflowStepFact {
                        owner_id: owner_id.clone(),
                        row_number: cells[0].parse::<u32>().ok(),
                        step: cells[1].clone(),
                        implemented_by,
                        touches,
                        raw_cells: cells,
                        explicit_ref_syntax: explicit_impl || explicit_touches,
                        start_offset: line_offset,
                        end_offset: line_end_offset(&lines, &offsets, idx).min(content.len()),
                        line: line_number,
                        raw_text: (*line).to_string(),
                    });
                }
            }

            if in_branches && line.trim().starts_with('|') {
                let cells = split_table_row(line);
                if cells.len() >= 5
                    && !is_table_separator(line.trim())
                    && !cells[0].eq_ignore_ascii_case("branch")
                {
                    workflow_branches.push(WorkflowBranchFact {
                        owner_id: owner_id.clone(),
                        branch: cells[0].clone(),
                        diverges_after: cells[1].clone(),
                        trigger: cells[2].clone(),
                        path: cells[3].clone(),
                        rejoin_terminal: cells[4].clone(),
                        raw_cells: cells,
                        start_offset: line_offset,
                        end_offset: line_end_offset(&lines, &offsets, idx).min(content.len()),
                        line: line_number,
                        raw_text: (*line).to_string(),
                    });
                }
            }
        }
    }

    let node_class = classify_node(component_id);
    let has_any_ref = !refs.is_empty();
    let native_artifacts = fence_zones
        .iter()
        .filter(|zone| is_supported_native_language(&zone.language))
        .filter_map(|zone| {
            let normativity = detect_normativity_near(content, zone.start_offset, zone.end_offset);
            let v4_context = node_class != OntologyNodeClass::LegacyOrUnknown
                || has_any_ref
                || normativity.is_some();
            v4_context.then(|| NativeArtifactFact {
                owner_id: current_owner_id(&id_blocks, zone.start_offset),
                language: zone.language.clone(),
                normativity,
                start_offset: zone.start_offset,
                end_offset: zone.end_offset,
                start_line: zone.start_line + 1,
                raw_text: content[zone.start_offset..zone.end_offset].to_string(),
            })
        })
        .collect::<Vec<_>>();

    for artifact in &native_artifacts {
        if find_owner_index(&id_blocks, artifact.start_offset).is_none() {
            warnings.push(OntologyMarkupWarning {
                line: Some(artifact.start_line),
                start_offset: Some(artifact.start_offset),
                message: format!(
                    "Native artifact fence `{}` has no owning I/D/Q block",
                    artifact.language
                ),
            });
        }
    }

    let has_v4_facts = !refs.is_empty()
        || !bindings.is_empty()
        || !workflow_steps.is_empty()
        || !workflow_branches.is_empty()
        || !extends.is_empty()
        || native_artifacts
            .iter()
            .any(|artifact| artifact.normativity.is_some())
        || matches!(
            node_class,
            OntologyNodeClass::Concept | OntologyNodeClass::SoftwareArchitecture
        );
    let era = if has_v4_facts {
        OntologyEra::V4Candidate
    } else {
        OntologyEra::V2
    };

    OntologyMarkupDocument {
        blocks,
        id_blocks,
        refs,
        bindings,
        workflow_steps,
        workflow_branches,
        extends,
        native_artifacts,
        derived_reference_indexes,
        era,
        node_class,
        warnings,
    }
}

// ─── Ontology v4 Phase 1 Linting ───────────────────────────────────────────

fn lint_location(
    component_id: &str,
    block_id: Option<String>,
    line: Option<usize>,
    start_offset: Option<usize>,
    end_offset: Option<usize>,
) -> OntologyLintLocation {
    OntologyLintLocation {
        component_id: component_id.to_string(),
        block_id,
        line,
        start_offset,
        end_offset,
    }
}

fn lint_diagnostic(
    rule_id: OntologyLintRuleId,
    message: impl Into<String>,
    location: OntologyLintLocation,
    evidence: Vec<String>,
    related_locations: Vec<OntologyLintLocation>,
) -> OntologyLintDiagnostic {
    OntologyLintDiagnostic {
        rule_id,
        severity: OntologyLintSeverity::Blocking,
        message: message.into(),
        location,
        evidence,
        related_locations,
    }
}

/// Advisory counterpart of [`lint_diagnostic`]: never promoted to a validation
/// issue by any lint mode (`strict` promotes only `Blocking` diagnostics) and
/// never flips a lint report's `valid` flag.
fn advisory_lint_diagnostic(
    rule_id: OntologyLintRuleId,
    message: impl Into<String>,
    location: OntologyLintLocation,
    evidence: Vec<String>,
    related_locations: Vec<OntologyLintLocation>,
) -> OntologyLintDiagnostic {
    OntologyLintDiagnostic {
        rule_id,
        severity: OntologyLintSeverity::Advisory,
        message: message.into(),
        location,
        evidence,
        related_locations,
    }
}

fn has_file_spec_frontmatter(content: &str) -> bool {
    let mut lines = content.lines();
    if lines.next().is_none_or(|line| line.trim() != "---") {
        return false;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            return false;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.trim() != "type" {
            continue;
        }
        let value = value
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        return value.eq_ignore_ascii_case("file");
    }
    false
}

fn reference_target_key(component: &str, block: Option<&str>) -> String {
    match block {
        Some(block) => format!("{component}#{block}"),
        None => component.to_string(),
    }
}

fn reference_fact_target_key(reference: &ReferenceFact) -> String {
    reference_target_key(
        &reference.target_component,
        reference.target_block.as_deref(),
    )
}

fn reference_target_location(component: &str, block: Option<&str>) -> OntologyLintLocation {
    lint_location(component, block.map(str::to_string), None, None, None)
}

fn parse_target_key(target: &str) -> (String, Option<String>) {
    let cleaned = trim_ref_syntax(target);
    if let Some((component, block)) = cleaned.split_once('#') {
        (component.to_string(), Some(block.to_string()))
    } else {
        (cleaned.to_string(), None)
    }
}

fn build_component_block_index(
    docs: &[ParsedLintDocument<'_>],
) -> HashMap<String, HashSet<String>> {
    docs.iter()
        .map(|doc| {
            (
                doc.component_id.to_string(),
                doc.document
                    .id_blocks
                    .iter()
                    .map(|block| block.raw_id.clone())
                    .collect::<HashSet<_>>(),
            )
        })
        .collect()
}

fn target_exists(
    index: &HashMap<String, HashSet<String>>,
    component: &str,
    block: Option<&str>,
) -> bool {
    let Some(blocks) = index.get(component) else {
        return false;
    };
    match block {
        Some(block) => blocks.contains(block),
        None => true,
    }
}

fn binding_has_contradiction_language(binding: &BindingFact) -> bool {
    let lower = binding.raw_text.to_ascii_lowercase();
    [
        "contradict",
        "contradicts",
        "contradiction",
        "overrides",
        "override",
        "instead of",
        "ignores",
        "does not follow",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn validate_reference_targets(
    docs: &[ParsedLintDocument<'_>],
    index: &HashMap<String, HashSet<String>>,
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        if doc.document.era == OntologyEra::V2 {
            continue;
        }

        for reference in &doc.document.refs {
            if reference.placement == ReferencePlacement::DerivedIndex {
                continue;
            }
            if !target_exists(
                index,
                &reference.target_component,
                reference.target_block.as_deref(),
            ) {
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::ValidateReferences,
                    format!(
                        "reference target does not exist: {}",
                        reference_fact_target_key(reference)
                    ),
                    lint_location(
                        doc.component_id,
                        reference.owner_id.clone(),
                        Some(reference.line),
                        Some(reference.start_offset),
                        Some(reference.end_offset),
                    ),
                    vec![reference.raw_text.clone()],
                    vec![reference_target_location(
                        &reference.target_component,
                        reference.target_block.as_deref(),
                    )],
                ));
            }
        }

        for step in &doc.document.workflow_steps {
            for target in step.implemented_by.iter().chain(step.touches.iter()) {
                let (component, block) = parse_target_key(target);
                if !target_exists(index, &component, block.as_deref()) {
                    diagnostics.push(lint_diagnostic(
                        OntologyLintRuleId::ValidateReferences,
                        format!("workflow target does not exist: {target}"),
                        lint_location(
                            doc.component_id,
                            step.owner_id.clone(),
                            Some(step.line),
                            Some(step.start_offset),
                            Some(step.end_offset),
                        ),
                        vec![step.raw_text.clone()],
                        vec![reference_target_location(&component, block.as_deref())],
                    ));
                }
            }
        }

        for binding in &doc.document.bindings {
            if binding.attached && binding_has_contradiction_language(binding) {
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::ValidateReferences,
                    "binding contains explicit contradiction/override language",
                    lint_location(
                        doc.component_id,
                        binding.owner_id.clone(),
                        Some(binding.line),
                        Some(binding.start_offset),
                        Some(binding.end_offset),
                    ),
                    vec![binding.raw_text.clone()],
                    Vec::new(),
                ));
            }
        }
    }
}

fn is_concept_target(component_id: &str) -> bool {
    classify_node(component_id) == OntologyNodeClass::Concept
}

fn reference_has_context(
    document: &OntologyMarkupDocument,
    _content: &str,
    reference_index: usize,
) -> bool {
    let reference = &document.refs[reference_index];
    if reference
        .rationale
        .as_deref()
        .is_some_and(|rationale| !rationale.trim().is_empty())
    {
        return true;
    }
    if document.bindings.iter().any(|binding| {
        binding.attached_ref_indices.contains(&reference_index)
            && binding.owner_id == reference.owner_id
    }) {
        return true;
    }
    false
}

fn validate_ref_quality(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    let mut annotation_sets: BTreeMap<Vec<String>, Vec<OntologyLintLocation>> = BTreeMap::new();

    for doc in docs {
        if doc.document.node_class != OntologyNodeClass::SoftwareArchitecture
            || doc.document.era == OntologyEra::V2
            || !doc.is_file_spec
        {
            continue;
        }
        let doc_in_scope = lint_doc_in_scope(doc, options);

        let concept_ref_indices = doc
            .document
            .refs
            .iter()
            .enumerate()
            .filter(|(_, reference)| is_concept_target(&reference.target_component))
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();
        let inline_concept_ref_indices = concept_ref_indices
            .iter()
            .copied()
            .filter(|idx| doc.document.refs[*idx].placement == ReferencePlacement::Inline)
            .collect::<Vec<_>>();
        let strong_inline_refs = inline_concept_ref_indices
            .iter()
            .copied()
            .filter(|idx| {
                let reference = &doc.document.refs[*idx];
                reference.specificity == ReferenceSpecificity::Block
                    && reference.target_block.as_deref() != Some("I1")
            })
            .collect::<Vec<_>>();

        if !concept_ref_indices.is_empty() && inline_concept_ref_indices.is_empty() {
            if doc_in_scope {
                let first = &doc.document.refs[concept_ref_indices[0]];
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::RefQualityLint,
                    "architecture leaf references concepts only through the derived References index",
                    lint_location(
                        doc.component_id,
                        first.owner_id.clone(),
                        Some(first.line),
                        Some(first.start_offset),
                        Some(first.end_offset),
                    ),
                    vec![first.raw_text.clone()],
                    Vec::new(),
                ));
            }
            continue;
        }

        if doc_in_scope && !inline_concept_ref_indices.is_empty() && strong_inline_refs.is_empty() {
            let first = &doc.document.refs[inline_concept_ref_indices[0]];
            diagnostics.push(lint_diagnostic(
                OntologyLintRuleId::RefQualityLint,
                "architecture leaf has only weak concept refs; use block-targeted refs beyond #I1",
                lint_location(
                    doc.component_id,
                    first.owner_id.clone(),
                    Some(first.line),
                    Some(first.start_offset),
                    Some(first.end_offset),
                ),
                vec![first.raw_text.clone()],
                Vec::new(),
            ));
        }

        let inline_targets = inline_concept_ref_indices
            .iter()
            .map(|idx| reference_fact_target_key(&doc.document.refs[*idx]))
            .collect::<HashSet<_>>();
        for idx in concept_ref_indices {
            let reference = &doc.document.refs[idx];
            if doc_in_scope
                && reference.placement == ReferencePlacement::DerivedIndex
                && !inline_targets.contains(&reference_fact_target_key(reference))
            {
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::RefQualityLint,
                    "concept ref appears only in the derived References index",
                    lint_location(
                        doc.component_id,
                        reference.owner_id.clone(),
                        Some(reference.line),
                        Some(reference.start_offset),
                        Some(reference.end_offset),
                    ),
                    vec![reference.raw_text.clone()],
                    Vec::new(),
                ));
            }
        }

        for idx in strong_inline_refs {
            let reference = &doc.document.refs[idx];
            if doc_in_scope && !reference_has_context(&doc.document, doc.content, idx) {
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::RefQualityLint,
                    "block-targeted concept ref has no rationale or Binding context",
                    lint_location(
                        doc.component_id,
                        reference.owner_id.clone(),
                        Some(reference.line),
                        Some(reference.start_offset),
                        Some(reference.end_offset),
                    ),
                    vec![reference.raw_text.clone()],
                    Vec::new(),
                ));
            }
        }

        let annotation_set = inline_concept_ref_annotation_set(&doc.document);
        if !annotation_set.is_empty() {
            let location = doc
                .document
                .refs
                .iter()
                .find(|reference| {
                    reference.placement == ReferencePlacement::Inline
                        && is_concept_target(&reference.target_component)
                })
                .map(|reference| {
                    lint_location(
                        doc.component_id,
                        reference.owner_id.clone(),
                        Some(reference.line),
                        Some(reference.start_offset),
                        Some(reference.end_offset),
                    )
                })
                .unwrap_or_else(|| lint_location(doc.component_id, None, None, None, None));
            annotation_sets
                .entry(annotation_set)
                .or_default()
                .push(location);
        }
    }

    for (annotation_set, locations) in annotation_sets {
        if locations.len() < options.dedup_min_group_size {
            continue;
        }
        if options.target_component_ids.is_none() {
            let Some(seed) = locations.first().cloned() else {
                continue;
            };
            diagnostics.push(lint_diagnostic(
                OntologyLintRuleId::RefQualityLint,
                format!(
                    "identical concept-ref annotation set appears in {} architecture leaves",
                    locations.len()
                ),
                seed,
                vec![
                    format!("group_size={}", locations.len()),
                    format!("annotations={}", annotation_set.join(" | ")),
                ],
                locations.into_iter().skip(1).collect(),
            ));
            continue;
        }

        for seed in locations
            .iter()
            .filter(|location| lint_component_in_scope(&location.component_id, options))
        {
            let related = locations
                .iter()
                .filter(|location| *location != seed)
                .cloned()
                .collect::<Vec<_>>();
            diagnostics.push(lint_diagnostic(
                OntologyLintRuleId::RefQualityLint,
                format!(
                    "identical concept-ref annotation set appears in {} architecture leaves",
                    locations.len()
                ),
                seed.clone(),
                vec![
                    format!("group_size={}", locations.len()),
                    format!("annotations={}", annotation_set.join(" | ")),
                ],
                related,
            ));
        }
    }
}

fn attached_binding_context(document: &OntologyMarkupDocument, ref_index: usize) -> Vec<String> {
    document
        .bindings
        .iter()
        .filter(|binding| binding.attached_ref_indices.contains(&ref_index))
        .map(|binding| binding.raw_text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect()
}

fn inline_concept_ref_annotation_set(document: &OntologyMarkupDocument) -> Vec<String> {
    let mut annotations = Vec::new();
    for (idx, reference) in document.refs.iter().enumerate() {
        if reference.placement != ReferencePlacement::Inline
            || !is_concept_target(&reference.target_component)
        {
            continue;
        }
        let context = reference
            .rationale
            .as_deref()
            .map(str::trim)
            .filter(|rationale| !rationale.is_empty())
            .map(str::to_string)
            .or_else(|| {
                let bindings = attached_binding_context(document, idx);
                if bindings.is_empty() {
                    None
                } else {
                    Some(bindings.join("\n"))
                }
            })
            .unwrap_or_default();
        annotations.push(format!(
            "{} :: {}",
            reference_fact_target_key(reference),
            context
        ));
    }
    annotations.sort();
    annotations
}

fn strip_fence_bodies(text: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn strip_id_label_prefix(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(rest) = trimmed.strip_prefix('[') else {
        return text;
    };
    let Some(close) = rest.find(']') else {
        return text;
    };
    let label = &rest[..close];
    let valid_label = matches!(label.chars().next(), Some('I' | 'D' | 'Q'))
        && label
            .chars()
            .skip(1)
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | ' ' | '_' | '-'));
    if valid_label {
        &rest[close + 1..]
    } else {
        text
    }
}

fn normalize_prose_tokens(text: &str) -> BTreeSet<String> {
    let without_label = strip_id_label_prefix(text);
    let without_fences = strip_fence_bodies(without_label);
    let mut cleaned = String::with_capacity(without_fences.len());
    let mut cursor = 0;
    while let Some(rel) = without_fences[cursor..].find("@ref") {
        cleaned.push_str(&without_fences[cursor..cursor + rel]);
        let after = cursor + rel + "@ref".len();
        let token_start =
            after + without_fences[after..].len() - without_fences[after..].trim_start().len();
        cursor = token_end(&without_fences, token_start).min(without_fences.len());
    }
    cleaned.push_str(&without_fences[cursor..]);

    cleaned
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'))
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .map(str::to_ascii_lowercase)
        .filter(|token| {
            !matches!(
                token.as_str(),
                "the"
                    | "and"
                    | "for"
                    | "with"
                    | "that"
                    | "this"
                    | "into"
                    | "from"
                    | "only"
                    | "must"
                    | "should"
                    | "where"
                    | "when"
                    | "what"
                    | "which"
                    | "per"
                    | "ref"
                    | "binding"
            )
        })
        .collect()
}

fn token_similarity(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn token_size_can_reach_similarity(
    a: &BTreeSet<String>,
    b: &BTreeSet<String>,
    threshold: f32,
) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let min_len = a.len().min(b.len()) as f32;
    let max_len = a.len().max(b.len()) as f32;
    max_len > 0.0 && (min_len / max_len) >= threshold
}

fn build_token_candidate_index(fingerprints: &[ProseFingerprint]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, fingerprint) in fingerprints.iter().enumerate() {
        for token in &fingerprint.tokens {
            index.entry(token.clone()).or_default().push(idx);
        }
    }
    index
}

fn candidate_indices_for_tokens(
    tokens: &BTreeSet<String>,
    index: &HashMap<String, Vec<usize>>,
    max_token_doc_frequency: usize,
    min_candidate_shared_tokens: usize,
) -> Vec<usize> {
    let min_candidate_shared_tokens = min_candidate_shared_tokens.max(1);
    let mut shared_counts: HashMap<usize, usize> = HashMap::new();
    for token in tokens {
        let Some(indices) = index.get(token) else {
            continue;
        };
        if max_token_doc_frequency > 0 && indices.len() > max_token_doc_frequency {
            continue;
        }
        for idx in indices {
            *shared_counts.entry(*idx).or_default() += 1;
        }
    }
    let mut candidates = shared_counts
        .into_iter()
        .filter_map(|(idx, shared_count)| {
            (shared_count >= min_candidate_shared_tokens).then_some(idx)
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates
}

fn strong_inline_concept_ref_targets(
    document: &OntologyMarkupDocument,
    block_id: &str,
) -> HashSet<String> {
    document
        .refs
        .iter()
        .filter(|reference| {
            reference.owner_id.as_deref() == Some(block_id)
                && reference.placement == ReferencePlacement::Inline
                && reference.specificity == ReferenceSpecificity::Block
                && reference.target_block.as_deref() != Some("I1")
                && is_concept_target(&reference.target_component)
        })
        .map(reference_fact_target_key)
        .collect()
}

fn block_has_inline_ref_to(
    document: &OntologyMarkupDocument,
    block_id: &str,
    target_component: &str,
    target_block: &str,
) -> bool {
    document.refs.iter().any(|reference| {
        reference.owner_id.as_deref() == Some(block_id)
            && reference.placement == ReferencePlacement::Inline
            && reference.specificity == ReferenceSpecificity::Block
            && reference.target_component == target_component
            && reference.target_block.as_deref() == Some(target_block)
    })
}

#[derive(Debug)]
struct ProseFingerprint {
    component_id: String,
    block_id: String,
    title: String,
    start_line: usize,
    start_offset: usize,
    end_offset: usize,
    tokens: BTreeSet<String>,
    strong_inline_concept_refs: HashSet<String>,
}

fn fingerprint_has_ref_covering_tokens(
    fingerprint: &ProseFingerprint,
    concept_fingerprints: &[ProseFingerprint],
    concept_fingerprint_by_key: &HashMap<String, usize>,
    repeated_tokens: &BTreeSet<String>,
    threshold: f32,
) -> bool {
    fingerprint.strong_inline_concept_refs.iter().any(|key| {
        concept_fingerprint_by_key
            .get(key)
            .and_then(|idx| concept_fingerprints.get(*idx))
            .is_some_and(|concept| token_similarity(repeated_tokens, &concept.tokens) >= threshold)
    })
}

fn validate_dedup_bar(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    let mut fingerprints = Vec::new();
    let mut concept_fingerprints = Vec::new();
    for doc in docs {
        match doc.document.node_class {
            OntologyNodeClass::SoftwareArchitecture if doc.is_file_spec => {
                for block in &doc.document.id_blocks {
                    let tokens = normalize_prose_tokens(&block.raw_text);
                    if tokens.len() < 8 {
                        continue;
                    }
                    fingerprints.push(ProseFingerprint {
                        component_id: doc.component_id.to_string(),
                        block_id: block.raw_id.clone(),
                        title: block.title.clone(),
                        start_line: block.start_line,
                        start_offset: block.start_offset,
                        end_offset: block.end_offset,
                        tokens,
                        strong_inline_concept_refs: strong_inline_concept_ref_targets(
                            &doc.document,
                            &block.raw_id,
                        ),
                    });
                }
            }
            OntologyNodeClass::Concept => {
                for block in &doc.document.id_blocks {
                    if block.raw_id == "I1" {
                        continue;
                    }
                    let tokens = normalize_prose_tokens(&block.raw_text);
                    if tokens.len() < 8 {
                        continue;
                    }
                    concept_fingerprints.push(ProseFingerprint {
                        component_id: doc.component_id.to_string(),
                        block_id: block.raw_id.clone(),
                        title: block.title.clone(),
                        start_line: block.start_line,
                        start_offset: block.start_offset,
                        end_offset: block.end_offset,
                        tokens,
                        strong_inline_concept_refs: HashSet::new(),
                    });
                }
            }
            _ => {
                continue;
            }
        }
    }

    let fingerprint_candidate_index = build_token_candidate_index(&fingerprints);
    let concept_candidate_index = build_token_candidate_index(&concept_fingerprints);
    let concept_fingerprint_by_key = concept_fingerprints
        .iter()
        .enumerate()
        .map(|(idx, concept)| {
            (
                format!("{}#{}", concept.component_id, concept.block_id),
                idx,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut emitted_groups: HashSet<Vec<String>> = HashSet::new();
    let seed_indices = if options.target_component_ids.is_some() {
        fingerprints
            .iter()
            .enumerate()
            .filter(|(_, fingerprint)| lint_component_in_scope(&fingerprint.component_id, options))
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>()
    } else {
        (0..fingerprints.len()).collect::<Vec<_>>()
    };
    for seed_idx in seed_indices {
        let mut group = vec![seed_idx];
        for idx in candidate_indices_for_tokens(
            &fingerprints[seed_idx].tokens,
            &fingerprint_candidate_index,
            options.dedup_max_token_doc_frequency,
            options.dedup_min_candidate_shared_tokens,
        ) {
            if idx == seed_idx {
                continue;
            }
            if !token_size_can_reach_similarity(
                &fingerprints[seed_idx].tokens,
                &fingerprints[idx].tokens,
                options.dedup_similarity_threshold,
            ) {
                continue;
            }
            if token_similarity(&fingerprints[seed_idx].tokens, &fingerprints[idx].tokens)
                >= options.dedup_similarity_threshold
            {
                group.push(idx);
            }
        }
        group.sort_unstable();
        group.dedup();
        if group.len() < options.dedup_min_group_size {
            continue;
        }
        if group.iter().all(|idx| {
            fingerprint_has_ref_covering_tokens(
                &fingerprints[*idx],
                &concept_fingerprints,
                &concept_fingerprint_by_key,
                &fingerprints[seed_idx].tokens,
                options.dedup_similarity_threshold,
            )
        }) {
            continue;
        }
        let key = group
            .iter()
            .map(|idx| {
                format!(
                    "{}#{}",
                    fingerprints[*idx].component_id, fingerprints[*idx].block_id
                )
            })
            .collect::<Vec<_>>();
        if !emitted_groups.insert(key.clone()) {
            continue;
        }
        let seed = &fingerprints[seed_idx];
        let related = group
            .iter()
            .filter(|idx| **idx != seed_idx)
            .map(|idx| {
                let item = &fingerprints[*idx];
                lint_location(
                    &item.component_id,
                    Some(item.block_id.clone()),
                    Some(item.start_line),
                    Some(item.start_offset),
                    Some(item.end_offset),
                )
            })
            .collect::<Vec<_>>();
        diagnostics.push(lint_diagnostic(
            OntologyLintRuleId::DedupBarLint,
            format!(
                "architecture prose appears duplicated across {} leaves without consistent block-targeted concept refs",
                group.len()
            ),
            lint_location(
                &seed.component_id,
                Some(seed.block_id.clone()),
                Some(seed.start_line),
                Some(seed.start_offset),
                Some(seed.end_offset),
            ),
            vec![
                format!("group_size={}", group.len()),
                format!("threshold={:.2}", options.dedup_similarity_threshold),
                format!("seed_title={}", seed.title),
            ],
            related,
        ));
    }

    for doc in docs {
        if doc.document.node_class != OntologyNodeClass::SoftwareArchitecture || !doc.is_file_spec {
            continue;
        }
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        for block in &doc.document.id_blocks {
            let leaf_tokens = normalize_prose_tokens(&block.raw_text);
            if leaf_tokens.len() < 8 {
                continue;
            }
            for concept_idx in candidate_indices_for_tokens(
                &leaf_tokens,
                &concept_candidate_index,
                options.dedup_max_token_doc_frequency,
                options.dedup_min_candidate_shared_tokens,
            ) {
                let concept = &concept_fingerprints[concept_idx];
                if !token_size_can_reach_similarity(
                    &leaf_tokens,
                    &concept.tokens,
                    options.dedup_similarity_threshold,
                ) {
                    continue;
                }
                if token_similarity(&leaf_tokens, &concept.tokens)
                    < options.dedup_similarity_threshold
                {
                    continue;
                }
                if block_has_inline_ref_to(
                    &doc.document,
                    &block.raw_id,
                    &concept.component_id,
                    &concept.block_id,
                ) {
                    continue;
                }
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::DedupBarLint,
                    "architecture prose restates a concept block without an inline ref to that block",
                    lint_location(
                        doc.component_id,
                        Some(block.raw_id.clone()),
                        Some(block.start_line),
                        Some(block.start_offset),
                        Some(block.end_offset),
                    ),
                    vec![
                        format!("matched_concept={}#{}", concept.component_id, concept.block_id),
                        format!("threshold={:.2}", options.dedup_similarity_threshold),
                        format!("concept_title={}", concept.title),
                    ],
                    vec![lint_location(
                        &concept.component_id,
                        Some(concept.block_id.clone()),
                        Some(concept.start_line),
                        Some(concept.start_offset),
                        Some(concept.end_offset),
                    )],
                ));
            }
        }
    }
}

fn validate_anti_scaffolding(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    const MARKERS: &[&str] = &[
        "legacy source detail",
        "legacy intent inventory",
        "mechanical conversion note",
        "hand-polish priority",
    ];
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        if doc.document.era == OntologyEra::V2 {
            continue;
        }
        let lines: Vec<&str> = doc.content.split('\n').collect();
        let offsets = compute_line_offsets(&lines, doc.content);
        let mut excluded = vec![false; lines.len()];
        mark_frontmatter_exclusions(&lines, &mut excluded);
        collect_fence_zones(&lines, &offsets, doc.content, &mut excluded);
        for (idx, line) in lines.iter().enumerate() {
            if excluded[idx] {
                continue;
            }
            let Some((_, title)) = parse_atx_heading(line.trim()) else {
                continue;
            };
            let lower = title.to_ascii_lowercase();
            if MARKERS.iter().any(|marker| lower.contains(marker)) {
                diagnostics.push(lint_diagnostic(
                    OntologyLintRuleId::AntiScaffoldingLint,
                    "mechanical migration/scaffolding heading is not allowed in v4 ontology",
                    lint_location(
                        doc.component_id,
                        None,
                        Some(idx + 1),
                        Some(offsets[idx]),
                        Some(line_end_offset(&lines, &offsets, idx)),
                    ),
                    vec![title.to_string()],
                    Vec::new(),
                ));
            }
        }
    }
}

fn validate_root_richness(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        let Some(baseline) = options.root_baselines.get(doc.component_id) else {
            continue;
        };
        let retained_ratio = if baseline.original_byte_len == 0 {
            1.0
        } else {
            doc.content.len() as f32 / baseline.original_byte_len as f32
        };
        let lower = doc.content.to_ascii_lowercase();
        let missing_markers = baseline
            .required_markers
            .iter()
            .filter(|marker| !lower.contains(&marker.to_ascii_lowercase()))
            .cloned()
            .collect::<Vec<_>>();
        if retained_ratio < options.root_min_retention_ratio || !missing_markers.is_empty() {
            diagnostics.push(lint_diagnostic(
                OntologyLintRuleId::RootRichnessGate,
                "migrated root candidate lost required richness markers",
                lint_location(doc.component_id, None, None, None, None),
                vec![
                    format!("retained_ratio={retained_ratio:.2}"),
                    format!("required_ratio={:.2}", options.root_min_retention_ratio),
                    format!("missing_markers={}", missing_markers.join(", ")),
                ],
                Vec::new(),
            ));
        }
    }
}

/// Shared line scan for the advisory tag/label hygiene checks: split into
/// lines, compute byte offsets, and exclude frontmatter plus fenced code so
/// examples quoted inside fences are never scanned.
///
/// The hygiene checks intentionally do not skip `OntologyEra::V2` documents:
/// legacy garbage lives in older-era files by definition, the diagnostics are
/// `Advisory` (never promoted, never affecting `valid`), so the
/// "v2 files remain default-valid" calibration is preserved while the
/// occurrences still surface for normalize-on-next-edit.
fn advisory_scan_lines(content: &str) -> (Vec<&str>, Vec<usize>, Vec<bool>) {
    let lines: Vec<&str> = content.split('\n').collect();
    let offsets = compute_line_offsets(&lines, content);
    let mut excluded = vec![false; lines.len()];
    mark_frontmatter_exclusions(&lines, &mut excluded);
    collect_fence_zones(&lines, &offsets, content, &mut excluded);
    (lines, offsets, excluded)
}

fn validate_legacy_uncertainty_tag_spelling(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        let (lines, offsets, excluded) = advisory_scan_lines(doc.content);
        for (idx, line) in lines.iter().enumerate() {
            if excluded[idx] {
                continue;
            }
            // Line-based limitation: only a line-start opening tag is matched
            // (`^\s*<uncertain[ >]`), which is how real block wrappers appear.
            // Mid-line prose or backticked mentions of the legacy spelling are
            // outside the anchor and intentionally not flagged; fenced examples
            // are excluded by the shared scan.
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("<uncertain") else {
                continue;
            };
            if !rest.starts_with(' ') && !rest.starts_with('>') {
                continue;
            }
            diagnostics.push(advisory_lint_diagnostic(
                OntologyLintRuleId::LegacyUncertaintyTagSpelling,
                "retired `<uncertain>` wrapper syntax is not authorable; move the marker to the owning I/D/Q label as `(Tags: uncertain)` with content unchanged",
                lint_location(
                    doc.component_id,
                    None,
                    Some(idx + 1),
                    Some(offsets[idx]),
                    Some(line_end_offset(&lines, &offsets, idx)),
                ),
                vec![trimmed.to_string()],
                Vec::new(),
            ));
        }
    }
}

fn validate_removed_uncertainty_wrapper(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        let (lines, offsets, excluded) = advisory_scan_lines(doc.content);
        for (idx, line) in lines.iter().enumerate() {
            if excluded[idx] {
                continue;
            }
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("<uncertainty") else {
                continue;
            };
            if !rest.starts_with(' ') && !rest.starts_with('>') {
                continue;
            }
            let opening_tag = trimmed
                .find('>')
                .map(|close_bracket| &trimmed[..close_bracket + 1])
                .unwrap_or(trimmed);
            diagnostics.push(advisory_lint_diagnostic(
                OntologyLintRuleId::NonAuthorableUncertaintyLevel,
                "retired `<uncertainty ...>` wrapper syntax and uncertainty levels are not authorable; use `(Tags: uncertain)` on the owning I/D/Q label",
                lint_location(
                    doc.component_id,
                    None,
                    Some(idx + 1),
                    Some(offsets[idx]),
                    Some(line_end_offset(&lines, &offsets, idx)),
                ),
                vec![opening_tag.to_string()],
                Vec::new(),
            ));
        }
    }
}

/// Match the bracket-label shape `[<LETTERS><DIGITS>[<one lowercase>].` at the
/// start of a trimmed line and return the alphabetic kind prefix. Non-label
/// brackets do not match the shape: markdown links/checklists/citations lack
/// the letters-then-digits-then-dot run (`[text](url)`, `[ ]`, `[1]`), and a
/// label-shaped link or reference definition is rejected by the trailing
/// `](`/`][`/`]:` guard.
fn bracket_label_kind(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix('[')?;
    let letters_len = rest.chars().take_while(char::is_ascii_alphabetic).count();
    if letters_len == 0 {
        return None;
    }
    let after_letters = &rest[letters_len..];
    let digits_len = after_letters
        .chars()
        .take_while(char::is_ascii_digit)
        .count();
    if digits_len == 0 {
        return None;
    }
    let mut after_digits = &after_letters[digits_len..];
    // One optional trailing lowercase insertion suffix (the `I7a` form).
    let mut suffix_chars = after_digits.chars();
    if suffix_chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        after_digits = suffix_chars.as_str();
    }
    if !after_digits.starts_with('.') {
        return None;
    }
    if let Some(close) = trimmed.find(']') {
        if matches!(
            trimmed[close + 1..].chars().next(),
            Some('(') | Some('[') | Some(':')
        ) {
            return None;
        }
    }
    Some(&rest[..letters_len])
}

fn validate_unrecognized_block_label_kind(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        let (lines, offsets, excluded) = advisory_scan_lines(doc.content);
        for (idx, line) in lines.iter().enumerate() {
            if excluded[idx] {
                continue;
            }
            let trimmed = line.trim_start();
            let Some(kind) = bracket_label_kind(trimmed) else {
                continue;
            };
            if matches!(kind, "I" | "D" | "Q") {
                continue;
            }
            let label_text = trimmed
                .find(']')
                .map(|close| trimmed[..close + 1].to_string())
                .unwrap_or_else(|| trimmed.chars().take(40).collect());
            diagnostics.push(advisory_lint_diagnostic(
                OntologyLintRuleId::UnrecognizedBlockLabelKind,
                format!(
                    "unrecognized label kind `{kind}` is invisible to intent records and grounding; only `[I<n>]`, `[D<n>]`, and `[Q<n>]` labels are parsed"
                ),
                lint_location(
                    doc.component_id,
                    None,
                    Some(idx + 1),
                    Some(offsets[idx]),
                    Some(line_end_offset(&lines, &offsets, idx)),
                ),
                vec![format!("label_kind={kind}"), label_text],
                Vec::new(),
            ));
        }
    }
}

/// Match a line-start `[I|D|Q<digits><UPPERCASE>. …]` label and return the
/// full label id (e.g. `D6D`). Lowercase insertion suffixes and non-I/D/Q
/// kinds are out of scope; those shapes are covered by
/// [`validate_unrecognized_block_label_kind`].
fn bracket_label_uppercase_suffix(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix('[')?;
    let mut chars = rest.chars();
    let kind = chars.next()?;
    if !matches!(kind, 'I' | 'D' | 'Q') {
        return None;
    }
    let after_kind = chars.as_str();
    let digits_len = after_kind.chars().take_while(char::is_ascii_digit).count();
    if digits_len == 0 {
        return None;
    }
    let after_digits = &after_kind[digits_len..];
    let mut suffix_chars = after_digits.chars();
    let suffix = suffix_chars.next()?;
    if !suffix.is_ascii_uppercase() || !suffix_chars.as_str().starts_with('.') {
        return None;
    }
    if let Some(close) = trimmed.find(']') {
        if matches!(
            trimmed[close + 1..].chars().next(),
            Some('(') | Some('[') | Some(':')
        ) {
            return None;
        }
    }
    Some(format!("{kind}{}{suffix}", &after_kind[..digits_len]))
}

fn validate_uppercase_label_insertion_suffix(
    docs: &[ParsedLintDocument<'_>],
    options: &OntologyLintOptions,
    diagnostics: &mut Vec<OntologyLintDiagnostic>,
) {
    for doc in docs {
        if !lint_doc_in_scope(doc, options) {
            continue;
        }
        let (lines, offsets, excluded) = advisory_scan_lines(doc.content);
        for (idx, line) in lines.iter().enumerate() {
            if excluded[idx] {
                continue;
            }
            let trimmed = line.trim_start();
            let Some(label) = bracket_label_uppercase_suffix(trimmed) else {
                continue;
            };
            let grandfather_key = format!("{}#{label}", doc.component_id);
            if options
                .grandfathered_uppercase_labels
                .contains(&grandfather_key)
            {
                continue;
            }
            let label_text = trimmed
                .find(']')
                .map(|close| trimmed[..close + 1].to_string())
                .unwrap_or_else(|| trimmed.chars().take(40).collect());
            diagnostics.push(advisory_lint_diagnostic(
                OntologyLintRuleId::UppercaseLabelInsertionSuffix,
                format!(
                    "label `{label}` uses an uppercase insertion suffix; a new insertion takes the next free number or a lowercase suffix (`I7a`) — existing uppercase-suffixed ids are grandfathered live identities and are never renumbered or re-cased"
                ),
                lint_location(
                    doc.component_id,
                    Some(label.clone()),
                    Some(idx + 1),
                    Some(offsets[idx]),
                    Some(line_end_offset(&lines, &offsets, idx)),
                ),
                vec![format!("label={label}"), label_text],
                Vec::new(),
            ));
        }
    }
}

/// Return the inline authoritative ref target set used by preservation checks.
#[must_use]
pub fn ontology_inline_ref_target_set(document: &OntologyMarkupDocument) -> BTreeSet<String> {
    document
        .refs
        .iter()
        .filter(|reference| reference.placement == ReferencePlacement::Inline)
        .map(reference_fact_target_key)
        .collect()
}

/// Validate that a pure restructure preserved the authoritative inline ref set.
#[must_use]
pub fn validate_ref_set_preservation(
    before: &OntologyMarkupDocument,
    after: &OntologyMarkupDocument,
) -> OntologyLintReport {
    let before_set = ontology_inline_ref_target_set(before);
    let after_set = ontology_inline_ref_target_set(after);
    let mut diagnostics = Vec::new();
    if before_set != after_set {
        let removed = before_set
            .difference(&after_set)
            .cloned()
            .collect::<Vec<_>>();
        let added = after_set
            .difference(&before_set)
            .cloned()
            .collect::<Vec<_>>();
        diagnostics.push(lint_diagnostic(
            OntologyLintRuleId::RefSetPreservationCheck,
            "inline ref target set changed during a pure restructure",
            lint_location("<before-after>", None, None, None, None),
            vec![
                format!("removed={}", removed.join(", ")),
                format!("added={}", added.join(", ")),
            ],
            Vec::new(),
        ));
    }
    OntologyLintReport {
        valid: diagnostics
            .iter()
            .all(|diag| diag.severity != OntologyLintSeverity::Blocking),
        diagnostics,
    }
}

/// Run deterministic Phase 1 ontology-v4 lints over a selected corpus.
#[must_use]
pub fn validate_ontology_corpus(
    inputs: &[OntologyLintInput<'_>],
    options: OntologyLintOptions,
) -> OntologyLintReport {
    let docs = inputs
        .iter()
        .map(|input| ParsedLintDocument {
            component_id: input.component_id,
            content: input.content,
            document: parse_ontology_document(input.component_id, input.content),
            is_file_spec: has_file_spec_frontmatter(input.content),
        })
        .collect::<Vec<_>>();
    let index = build_component_block_index(&docs);
    let mut diagnostics = Vec::new();

    validate_reference_targets(&docs, &index, &options, &mut diagnostics);
    validate_ref_quality(&docs, &options, &mut diagnostics);
    validate_dedup_bar(&docs, &options, &mut diagnostics);
    validate_anti_scaffolding(&docs, &options, &mut diagnostics);
    validate_root_richness(&docs, &options, &mut diagnostics);
    validate_legacy_uncertainty_tag_spelling(&docs, &options, &mut diagnostics);
    validate_removed_uncertainty_wrapper(&docs, &options, &mut diagnostics);
    validate_unrecognized_block_label_kind(&docs, &options, &mut diagnostics);
    validate_uppercase_label_insertion_suffix(&docs, &options, &mut diagnostics);

    diagnostics.sort_by(|a, b| {
        a.rule_id
            .cmp(&b.rule_id)
            .then(a.location.component_id.cmp(&b.location.component_id))
            .then(a.location.block_id.cmp(&b.location.block_id))
            .then(a.location.line.cmp(&b.location.line))
            .then(a.message.cmp(&b.message))
    });
    diagnostics.dedup_by(|a, b| {
        a.rule_id == b.rule_id
            && a.location == b.location
            && a.message == b.message
            && a.evidence == b.evidence
    });

    OntologyLintReport {
        valid: diagnostics
            .iter()
            .all(|diag| diag.severity != OntologyLintSeverity::Blocking),
        diagnostics,
    }
}

// ─── Tag Extraction ─────────────────────────────────────────────────────────

/// Serialized `g5-question` payload cap (D69). A payload over the cap is not
/// read: the card itself renders it read-only, and an unread payload cannot
/// establish authority.
const MAX_QUESTION_PAYLOAD_BYTES: usize = 32768;

/// The two payload facts protection depends on.
#[derive(Deserialize)]
struct QuestionProtectionPayload {
    /// Required and gated to exactly 1 below, mirroring the client's
    /// `raw.schema !== 1` rejection. Without it, a payload from an unsupported
    /// schema (or none) could be frozen as a human decision the product parser
    /// would refuse to read — authority for data no shipped surface accepts.
    #[serde(default)]
    schema: Option<u64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    answer: Option<QuestionProtectionAnswer>,
}

#[derive(Deserialize)]
struct QuestionProtectionAnswer {
    /// Left as a `Value` so BOTH provenance forms D10 recognizes round-trip
    /// through one field: the mapping carrying `kind`, and the legacy scalar
    /// `answered_by: user`.
    #[serde(default)]
    answered_by: Option<serde_yaml::Value>,
}

/// Body of the first `g5-question` fence in a block, if it has one.
fn question_fence_body(raw_content: &str) -> Option<String> {
    let mut body: Option<Vec<&str>> = None;
    for line in raw_content.lines() {
        let trimmed = line.trim_start();
        match body.as_mut() {
            None => {
                if trimmed.starts_with("```")
                    && trimmed.trim_start_matches('`').trim() == "g5-question"
                {
                    body = Some(Vec::new());
                }
            }
            Some(lines) => {
                if trimmed.starts_with("```") {
                    return Some(lines.join("\n"));
                }
                lines.push(line);
            }
        }
    }
    // An unterminated fence is not a payload.
    None
}

/// Whether a block's `g5-question` payload records a USER answer.
///
/// The label is an INDEX and is allowed to lag the payload — a document
/// answered before this grammar carries no `answered` token at all, and the
/// card only backfills it on the next write. Protection therefore cannot key on
/// the label alone, or every pre-grammar human decision sits unprotected while
/// looking, to the label, exactly like an unanswered question.
///
/// Parsed with the strict typed YAML contract rather than scanned, because a
/// hand-rolled scan disagrees with the schema in BOTH directions: it misses
/// quoted, flow-style, and commented values (leaving a real decision
/// unprotected) and accepts duplicate-key or malformed payloads (freezing text
/// no one approved). `serde_yaml` rejects duplicate keys, which is the same
/// corruption the card's own strict-write parser refuses.
///
/// Every failure — over the cap, unparseable, `agent`, null, or missing
/// provenance — yields false, i.e. UNPROTECTED. Validity is a precondition of
/// authority (D69), so an unreadable payload must not freeze content on a
/// guess.
#[doc(hidden)]
pub fn payload_records_user_answer(raw_content: &str) -> bool {
    let Some(body) = question_fence_body(raw_content) else {
        return false;
    };
    if body.len() > MAX_QUESTION_PAYLOAD_BYTES {
        return false;
    }
    let Ok(payload) = serde_yaml::from_str::<QuestionProtectionPayload>(&body) else {
        return false;
    };
    if payload.schema != Some(1) {
        return false;
    }
    if payload.status.as_deref() != Some("answered") {
        return false;
    }
    let Some(answered_by) = payload.answer.and_then(|answer| answer.answered_by) else {
        return false;
    };
    match answered_by {
        // Legacy scalar, still fully authoritative (D10).
        serde_yaml::Value::String(kind) => kind == "user",
        serde_yaml::Value::Mapping(mapping) => {
            mapping
                .get(serde_yaml::Value::String("kind".to_string()))
                .and_then(serde_yaml::Value::as_str)
                == Some("user")
        }
        _ => false,
    }
}

// ─── Protected Span Diff ────────────────────────────────────────────────────

// ─── Full Validation ────────────────────────────────────────────────────────

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ontology_document_fences_are_exclusion_zones_and_artifacts() {
        let content = "\
# Company Data

[D5. DBML shape]
The DBML block is normative for the logical entity shape.

```dbml
[D1. Fake label]
@ref fake#D1
<human origin=\"invalid_value\">example</human>
```

[D6. Follow-on DDL]
The SQL sketch follows @ref data_model/company#D5.
";

        let document = parse_ontology_document("data_model/company", content);

        assert_eq!(
            document
                .id_blocks
                .iter()
                .map(|block| block.raw_id.as_str())
                .collect::<Vec<_>>(),
            vec!["D5", "D6"]
        );
        assert_eq!(document.native_artifacts.len(), 1);
        assert_eq!(document.native_artifacts[0].owner_id.as_deref(), Some("D5"));
        assert_eq!(document.native_artifacts[0].language, "dbml");
        assert_eq!(
            document.native_artifacts[0].normativity.as_deref(),
            Some("normative")
        );
        assert_eq!(document.refs.len(), 1);
        assert_eq!(document.refs[0].target_component, "data_model/company");
        assert_eq!(document.refs[0].target_block.as_deref(), Some("D5"));
    }

    #[test]
    fn test_parse_empty_content() {
        let blocks = parse_blocks("");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_parse_heading_and_paragraph() {
        let content = "## Boundary\n\nSome content about boundaries.";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].raw_content, "## Boundary");
        assert_eq!(blocks[1].raw_content, "Some content about boundaries.");
        assert_eq!(blocks[0].block_index, 0);
        assert_eq!(blocks[1].block_index, 1);
    }

    #[test]
    fn test_parse_ontology_document_indexes_heading_prefixed_id_labels() {
        let content = "\
# Performance

### [D6. Heading target]

Visible target details.

### Next heading

Outside the D6 block.
";
        let document = parse_ontology_document("performance_requirement", content);
        let block = document
            .id_blocks
            .iter()
            .find(|block| block.raw_id == "D6")
            .expect("heading-prefixed label should define a target block");

        assert_eq!(block.kind, IdBlockKind::Design);
        assert_eq!(block.title, "Heading target");
        assert_eq!(block.start_line, 3);
        assert_eq!(
            &content[block.label_start_offset..block.label_end_offset],
            "[D6. Heading target]"
        );
        assert!(block.raw_text.contains("Visible target details."));
        assert!(!block.raw_text.contains("Outside the D6 block."));
    }

    #[test]
    fn test_parse_human_tagged_block() {
        let content = "[D1. Protected text] (Tags: human)\n\nMust not be changed.";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|block| {
            block
                .semantic_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.human && !metadata.uncertain)
        }));
    }

    #[test]
    fn h1_title_does_not_extend_record_metadata_across_a_section_heading() {
        let content = "# Document title\n\n[D1. Protected] (Tags: human)\n\nProtected body.\n\n## Next section\n\nOutside body.";
        let blocks = parse_blocks(content);

        let protected_body = blocks
            .iter()
            .find(|block| block.raw_content == "Protected body.")
            .expect("protected body block");
        assert!(protected_body
            .semantic_metadata
            .as_ref()
            .is_some_and(|metadata| metadata.human));

        let outside_body = blocks
            .iter()
            .find(|block| block.raw_content == "Outside body.")
            .expect("outside body block");
        assert!(outside_body.semantic_metadata.is_none());
    }

    #[test]
    fn test_parse_uncertainty_tagged_block() {
        let content = "[D1. This is uncertain] (Tags: uncertain)\n\nNeeds review.";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|block| {
            block
                .semantic_metadata
                .as_ref()
                .is_some_and(|metadata| !metadata.human && metadata.uncertain)
        }));
    }

    fn authored(human: bool, uncertain: bool) -> SemanticRecordMetadata {
        SemanticRecordMetadata {
            human,
            uncertain,
            answered: false,
            severity: None,
        }
    }

    #[test]
    fn semantic_record_metadata_suffix_accepts_the_authored_canonical_forms() {
        for (suffix, expected) in [
            ("(Tags: human)", authored(true, false)),
            ("(Tags: uncertain)", authored(false, true)),
            ("(Tags: human, uncertain)", authored(true, true)),
        ] {
            // Authored tags parse identically on a Q and on an I/D record.
            for is_question in [false, true] {
                assert_eq!(
                    parse_semantic_record_metadata_suffix(suffix, is_question),
                    Ok(expected.clone())
                );
            }
        }
    }

    #[test]
    fn semantic_record_metadata_suffix_accepts_question_state_on_q_records() {
        for (suffix, expected) in [
            (
                "(Tags: answered)",
                SemanticRecordMetadata {
                    human: false,
                    uncertain: false,
                    answered: true,
                    severity: None,
                },
            ),
            (
                "(Tags: critical)",
                SemanticRecordMetadata {
                    human: false,
                    uncertain: false,
                    answered: false,
                    severity: Some(SemanticRecordSeverity::Critical),
                },
            ),
            (
                "(Tags: answered, high)",
                SemanticRecordMetadata {
                    human: false,
                    uncertain: false,
                    answered: true,
                    severity: Some(SemanticRecordSeverity::High),
                },
            ),
            (
                "(Tags: human, uncertain, answered, low)",
                SemanticRecordMetadata {
                    human: true,
                    uncertain: true,
                    answered: true,
                    severity: Some(SemanticRecordSeverity::Low),
                },
            ),
        ] {
            assert_eq!(
                parse_semantic_record_metadata_suffix(suffix, true),
                Ok(expected)
            );
        }
    }

    #[test]
    fn answered_establishes_human_authorship_exactly_as_human_does() {
        let answered = parse_semantic_record_metadata_suffix("(Tags: answered)", true).unwrap();
        assert!(answered.is_human());
        assert!(!answered.human, "the authored flag itself stays false");
        assert!(!authored(false, true).is_human());
    }

    #[test]
    fn question_state_tags_are_rejected_on_non_question_records() {
        // Question state on an I/D record would claim metadata the record does
        // not carry, so it fails closed rather than being ignored.
        for suffix in [
            "(Tags: answered)",
            "(Tags: high)",
            "(Tags: human, answered)",
        ] {
            assert!(
                parse_semantic_record_metadata_suffix(suffix, false).is_err(),
                "expected Q-only tag to be rejected on an I/D record: {suffix}"
            );
        }
    }

    #[test]
    fn semantic_record_metadata_suffix_rejects_old_and_malformed_forms() {
        for suffix in [
            r#"(Tags: human origin="direct_edit")"#,
            r#"(Tags: uncertainty-high source="agent")"#,
            "(Tags: uncertainty-high)",
            "(Tags: human-provided)",
            "(Tags: uncertain, human)",
            "(Tags: human, human)",
            "(Tags: unknown-tag)",
            "(Tags:)",
            // Out-of-canonical-order question state.
            "(Tags: answered, human)",
            "(Tags: high, answered)",
            // Two severities cannot both be true of one question.
            "(Tags: high, critical)",
            "(Tags: answered, answered)",
            // Loose spacing stays rejected.
            "(Tags:human)",
            "(Tags: human,uncertain)",
        ] {
            assert!(
                parse_semantic_record_metadata_suffix(suffix, true).is_err(),
                "expected invalid suffix to fail closed: {suffix}"
            );
        }
    }

    #[test]
    fn test_parse_code_fence() {
        let content = "```rust\nfn main() {}\n```";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].raw_content.contains("fn main()"));
    }

    #[test]
    fn test_parse_table() {
        let content = "| Col1 | Col2 |\n| --- | --- |\n| a | b |";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].raw_content.contains("Col1"));
    }

    #[test]
    fn test_byte_offsets() {
        let content = "## Boundary\n\nSome content.";
        let blocks = parse_blocks(content);

        for block in &blocks {
            assert_eq!(
                &content[block.start_offset..block.end_offset],
                block.raw_content,
                "Byte offsets should match raw_content for block: {:?}",
                block.raw_content
            );
        }
    }

    #[test]
    fn test_frontmatter_parsing() {
        let content = "---\ncomponent_id: test\nversion: 0.1.0\n---\n\n## Boundary\n\nContent.";
        let blocks = parse_blocks(content);

        assert!(blocks[0].raw_content.starts_with("---"));
        assert!(blocks[0].raw_content.ends_with("---"));
    }

    #[test]
    fn test_bullet_list_parsing() {
        let content = "- Item 1\n- Item 2\n- Item 3";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].raw_content.contains("Item 1"));
        assert!(blocks[0].raw_content.contains("Item 3"));
    }

    #[test]
    fn test_numbered_list_parsing() {
        let content = "1. First\n2. Second\n3. Third";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].raw_content.contains("First"));
    }

    #[test]
    fn test_blockquote_parsing() {
        let content = "> This is a quote\n> Second line";
        let blocks = parse_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].raw_content.contains("This is a quote"));
    }

    // --- detect_protected_span_violations ---

    #[test]
    fn test_parse_ontology_document_v2_noop_summary() {
        let content = "\
---
component_id: g5/g5d/tools/spec_ops
---
# Spec Ops

[I1. Existing v2 behavior]

This is a normal existing ontology block with no v4 body facts.
";

        let document = parse_ontology_document("g5/g5d/tools/spec_ops", content);
        let summary = document.summary();

        assert_eq!(summary.era, OntologyEra::V2);
        assert_eq!(summary.node_class, OntologyNodeClass::LegacyOrUnknown);
        assert_eq!(summary.id_block_count, 1);
        assert_eq!(summary.ref_count, 0);
        assert_eq!(summary.binding_count, 0);
        assert_eq!(summary.native_artifact_count, 0);
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn test_parse_ontology_document_references_index_only_is_v4_candidate() {
        let content = "\
# Legacy With Index

## References

- `@ref data_model/company#I1`, `#D5`
";

        let document = parse_ontology_document("g5/g5d/tools/spec_ops", content);
        let summary = document.summary();

        assert_eq!(summary.era, OntologyEra::V4Candidate);
        assert_eq!(summary.node_class, OntologyNodeClass::LegacyOrUnknown);
        assert_eq!(summary.inline_ref_count, 0);
        assert_eq!(summary.derived_ref_count, 2);
        assert_eq!(summary.derived_reference_index_count, 1);
    }

    #[test]
    fn test_parse_ontology_document_inline_refs_binding_and_derived_index() {
        let content = "\
# Company Model

## References

- `@ref data_model/company#I1`, `#D5`, `#D6` — entity shape

## Implementation Surface

[D1. Code-level surface]

Fields realize @ref data_model/company#I2 and `@ref security_policy#D4`.
**Binding:** maps the concept fields into concrete columns without adding schema.
";

        let document = parse_ontology_document(
            "software_architecture/core_data_layer/models/company",
            content,
        );
        let summary = document.summary();

        assert_eq!(summary.era, OntologyEra::V4Candidate);
        assert_eq!(summary.node_class, OntologyNodeClass::SoftwareArchitecture);
        assert_eq!(summary.ref_count, 5);
        assert_eq!(summary.inline_ref_count, 2);
        assert_eq!(summary.derived_ref_count, 3);
        assert_eq!(summary.derived_reference_index_count, 1);
        assert_eq!(summary.binding_count, 1);
        assert!(document.bindings[0].attached);
        assert_eq!(document.bindings[0].attached_ref_indices.len(), 2);
        assert!(document.refs.iter().any(|reference| {
            reference.placement == ReferencePlacement::DerivedIndex
                && reference.target_component == "data_model/company"
                && reference.target_block.as_deref() == Some("D5")
        }));
        assert!(document.refs.iter().any(|reference| {
            reference.placement == ReferencePlacement::Inline
                && reference.owner_id.as_deref() == Some("D1")
                && reference.target_component == "security_policy"
                && reference.target_block.as_deref() == Some("D4")
        }));
    }

    #[test]
    fn test_parse_ontology_document_shorthand_refs_stop_at_next_full_ref() {
        let content = "\
# Company Model

## References

- `@ref data_model/company#I1`, `@ref security_policy#I2`, `#D4`
";

        let document = parse_ontology_document("software_architecture/models/company", content);
        let targets = document
            .refs
            .iter()
            .map(reference_fact_target_key)
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec![
                "data_model/company#I1".to_string(),
                "security_policy#I2".to_string(),
                "security_policy#D4".to_string(),
            ]
        );
        assert!(!targets.contains(&"data_model/company#I2".to_string()));
        assert!(!targets.contains(&"data_model/company#D4".to_string()));
    }

    #[test]
    fn test_parse_ref_target_strips_code_ticks_after_sentence_punctuation() {
        assert_eq!(
            parse_ref_target("`data_model/company#D5`."),
            Some(("data_model/company".to_string(), Some("D5".to_string())))
        );
    }

    #[test]
    fn test_parse_ref_target_rejects_placeholder_notation() {
        assert_eq!(parse_ref_target("<component>[#<block>]"), None);
        assert_eq!(parse_ref_target("<component>#<block>"), None);
    }

    #[test]
    fn test_ref_target_component_validator_boundaries() {
        assert!(is_valid_ref_component("software_architecture/g5-common"));
        assert!(is_valid_ref_component("data_model.company_v2"));

        for invalid in ["", ".", "./", "---", "bad<target>", "bad target"] {
            assert!(
                !is_valid_ref_component(invalid),
                "component should be invalid: {invalid:?}"
            );
        }
    }

    #[test]
    fn test_ref_target_block_validator_boundaries() {
        assert!(is_valid_ref_block("D1"));
        assert!(is_valid_ref_block("D13A"));
        assert!(is_valid_ref_block("I7a"));

        for invalid in ["", "_", "-", "D1/D2", "D1.D2", "D1 D2", "<D1>"] {
            assert!(
                !is_valid_ref_block(invalid),
                "block should be invalid: {invalid:?}"
            );
        }
    }

    #[test]
    fn test_parse_ontology_document_ignores_placeholder_ref_notation() {
        let content = "\
# Reference Syntax

[D1. Authoring notation]

Write placeholder notation as `@ref <component>[#<block>]` when documenting syntax.
";

        let document = parse_ontology_document("software_architecture/example", content);
        let summary = document.summary();

        assert_eq!(summary.inline_ref_count, 0);
        assert!(document.refs.is_empty());
        assert!(document.warnings.is_empty());
    }

    #[test]
    fn test_parse_ontology_document_chained_refs_are_not_rationale() {
        let content = "\
# Company Model

[D1. Code-level surface]

Uses @ref data_model/company#D5 and @ref security_policy#D4.
";

        let document = parse_ontology_document("software_architecture/models/company", content);
        let company_ref = document
            .refs
            .iter()
            .find(|reference| reference.target_component == "data_model/company")
            .expect("expected data model ref");

        assert_eq!(company_ref.rationale, None);
    }

    #[test]
    fn test_parse_ontology_document_binding_attaches_only_nearest_preceding_refs() {
        let content = "\
# Company Model

[D1. Code-level surface]

First applies @ref data_model/company#D5 because the model stores company rows.
Second applies @ref security_policy#D4 because this route enforces tenant checks.
**Binding:** maps the security concept to the local guard.

[D2. Same-line binding]

**Binding:** should not attach to @ref testing_policy#D2 after the marker.
";

        let document = parse_ontology_document("software_architecture/models/company", content);

        assert_eq!(document.bindings.len(), 2);
        assert_eq!(document.bindings[0].attached_ref_indices.len(), 1);
        let attached = &document.refs[document.bindings[0].attached_ref_indices[0]];
        assert_eq!(attached.target_component, "security_policy");
        assert_eq!(attached.target_block.as_deref(), Some("D4"));
        assert!(!document.bindings[1].attached);
        assert!(document
            .warnings
            .iter()
            .any(|warning| warning.line == Some(document.bindings[1].line)));
    }

    #[test]
    fn test_parse_ontology_document_extends_ref_marker_target() {
        let content = "\
# Specialized Policy

[D1. Pack specialization]

This policy extends @ref core_workflow/onboarding#D1 for local setup.
";

        let document = parse_ontology_document("security_policy/local", content);

        assert_eq!(document.extends.len(), 1);
        assert_eq!(
            document.extends[0].target.as_deref(),
            Some("core_workflow/onboarding#D1")
        );
    }

    #[test]
    fn test_parse_ontology_document_extends_plain_prose_has_no_target() {
        let content = "\
# Plain Extends Prose

[D1. Java inheritance prose]

Check whether the imported class appears only in `extends`/`implements` clauses.

[D2. Test reuse prose]

The migration test extends `g5-api/tests/api_issues_migration.rs`.
";

        let document = parse_ontology_document("software_architecture/local", content);

        assert_eq!(document.extends.len(), 2);
        assert!(document
            .extends
            .iter()
            .all(|extends| extends.target.is_none()));
    }

    #[test]
    fn test_parse_ontology_document_whole_node_refs_and_unattached_binding_warnings() {
        let content = "# Whole Node Ref\n\n\
[D1. Coarse dependency]\n\n\
This block depends on @ref data_model/company for the whole node.\n\n\
[D2. Unattached binding]\n\n\
**Binding:** has no authoritative inline reference in this block.\n\n\
[D3. Malformed ref]\n\n\
Malformed @ref \n";

        let document = parse_ontology_document("software_architecture/company_model", content);

        assert_eq!(document.refs.len(), 1);
        assert_eq!(
            document.refs[0].specificity,
            ReferenceSpecificity::WholeNode
        );
        assert_eq!(document.refs[0].target_component, "data_model/company");
        assert_eq!(document.refs[0].target_block, None);

        assert_eq!(document.bindings.len(), 1);
        assert!(!document.bindings[0].attached);
        assert_eq!(document.bindings[0].owner_id.as_deref(), Some("D2"));
        assert!(document
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Binding has no preceding")));
        assert!(document
            .warnings
            .iter()
            .any(|warning| warning.message.contains("Malformed @ref")));
    }

    #[test]
    fn test_parse_ontology_document_native_artifact_context_handles_non_ascii() {
        let repeated_context = "Architecture context — with unicode punctuation. ".repeat(20);
        let content = format!(
            "\
# Company Data

[D5. SQL shape]

{repeated_context}
This artifact is normative for the stored shape.

```sql
select 1;
```
"
        );

        let document = parse_ontology_document("data_model/company", &content);

        assert_eq!(document.native_artifacts.len(), 1);
        assert_eq!(
            document.native_artifacts[0].normativity.as_deref(),
            Some("normative")
        );
    }

    #[test]
    fn test_parse_ontology_document_adjacent_labels_bound_native_artifacts() {
        let content = "\
# Adjacent Artifacts

[D1. SQL artifact]

```sql
select 1;
```

[D2. DBML artifact]

```dbml
Table companies {
  id uuid
}
```
";

        let document = parse_ontology_document("data_model/company", content);

        assert_eq!(document.native_artifacts.len(), 2);
        assert_eq!(document.native_artifacts[0].owner_id.as_deref(), Some("D1"));
        assert_eq!(document.native_artifacts[1].owner_id.as_deref(), Some("D2"));
    }

    #[test]
    fn test_parse_ontology_document_workflow_rows_and_extends_mentions() {
        let content = "\
# Workflow

## Steps

### Happy path

[D1. Step sequence]

| # | Step | Implemented by (`@ref`) | Touches (concepts) |
|---|------|--------------------------|--------------------|
| 1 | Create the Contact | `software_architecture/domain_services/contact_service` | `data_model/contact`, `security_policy#D4` |

## Branches

### Existing contact

[D4. Branch model]

| Branch | Diverges after | Trigger | Path | Rejoin / terminal |
|--------|----------------|---------|------|-------------------|
| B1 Existing contact | step 1 | search returns match | skip step 2 | rejoins at step 3 |

[I2. Sharing]

This workflow later extends overlay behavior from an imported pack.
";

        let document = parse_ontology_document("core_workflow/create_contact_to_deal", content);

        assert_eq!(document.workflow_steps.len(), 1);
        assert_eq!(document.workflow_steps[0].owner_id.as_deref(), Some("D1"));
        assert_eq!(document.workflow_steps[0].row_number, Some(1));
        assert!(document.workflow_steps[0]
            .implemented_by
            .contains(&"software_architecture/domain_services/contact_service".to_string()));
        assert!(document.workflow_steps[0]
            .touches
            .contains(&"security_policy#D4".to_string()));
        assert_eq!(document.workflow_branches.len(), 1);
        assert_eq!(
            document.workflow_branches[0].owner_id.as_deref(),
            Some("D4")
        );
        assert_eq!(document.workflow_branches[0].branch, "B1 Existing contact");
        assert_eq!(document.extends.len(), 1);
        assert_eq!(document.extends[0].target, None);
    }

    #[test]
    fn test_parse_ontology_document_fact_byte_spans_round_trip() {
        let content = "\
# Byte Spans

[D1. Surface]
Uses @ref data_model/company#D5.
**Binding:** realizes the table.

```sql
select 1;
```
";

        let document = parse_ontology_document("data_model/company", content);
        for reference in &document.refs {
            assert_eq!(
                &content[reference.start_offset..reference.end_offset],
                reference.raw_text
            );
        }
        for binding in &document.bindings {
            assert_eq!(
                &content[binding.start_offset..binding.end_offset],
                binding.raw_text
            );
        }
        for artifact in &document.native_artifacts {
            assert_eq!(
                &content[artifact.start_offset..artifact.end_offset],
                artifact.raw_text
            );
        }
    }

    fn lint_report_for<'a>(inputs: &[(&'a str, &'a str)]) -> OntologyLintReport {
        lint_report_for_with_options(inputs, OntologyLintOptions::default())
    }

    fn lint_report_for_with_options<'a>(
        inputs: &[(&'a str, &'a str)],
        options: OntologyLintOptions,
    ) -> OntologyLintReport {
        let lint_inputs = inputs
            .iter()
            .map(|(component_id, content)| OntologyLintInput {
                component_id,
                content,
            })
            .collect::<Vec<_>>();
        validate_ontology_corpus(&lint_inputs, options)
    }

    fn rule_ids(report: &OntologyLintReport) -> Vec<OntologyLintRuleId> {
        report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id)
            .collect()
    }

    #[test]
    fn test_validate_ontology_corpus_v2_default_valid() {
        let v2 = "\
---
component_id: g5/g5d/tools/spec_ops
type: file
---
# Spec Ops

[D1. Existing implementation]

This current-format spec has no ontology-v4 body facts.
";

        let report = lint_report_for(&[("g5/g5d/tools/spec_ops", v2)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn test_validate_ontology_corpus_file_type_comes_from_frontmatter_only() {
        let body_type_file = "\
# Company Model

[D1. Body example]

This body mentions type: file but has no frontmatter, so file-only v4 lints do not apply.
Uses @ref data_model/company#D5.
";
        let quoted_frontmatter = "\
---
component_id: software_architecture/models/company
type: \"file\" # valid YAML comment
---
# Company Model

[D1. Body example]

Uses @ref data_model/company#D5.
";

        assert!(!has_file_spec_frontmatter(body_type_file));
        assert!(has_file_spec_frontmatter(quoted_frontmatter));
    }

    #[test]
    fn test_validate_ontology_corpus_gold_v4_passes() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

[D5. DBML shape]

The DBML shape is normative.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Code-level surface]

The SQLAlchemy model realizes @ref data_model/company#D5 for concrete columns.
**Binding:** maps every concept field into a typed ORM column without schema additions.
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn test_validate_ontology_corpus_target_scope_uses_index_without_unselected_diagnostics() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D1. Company concept]

Company records carry tenant ownership.
";
        let target = "\
---
component_id: data_model/order
type: file
---
# Order

[D1. Order concept]

Order records refer to @ref data_model/company#D1.
";
        let unselected_bad = "\
---
component_id: data_model/bad
type: file
---
# Bad

[D1. Bad ref]

This unselected file points at @ref data_model/missing#D1.
";

        let report = lint_report_for_with_options(
            &[
                ("data_model/company", concept),
                ("data_model/order", target),
                ("data_model/bad", unselected_bad),
            ],
            OntologyLintOptions::scoped_to_component("data_model/order"),
        );

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }

    #[test]
    fn test_validate_ontology_corpus_target_scope_dedup_seeds_selected_component() {
        let repeated = |component_id: &str, title: &str| {
            format!(
                "\
---
component_id: {component_id}
type: file
---
# {title}

[D1. Duplicated security prose]

Every request must enforce tenant scope, record owner filters, structured audit logging, and permission checks before returning data to callers.
"
            )
        };
        let a = repeated("software_architecture/models/a", "A");
        let b = repeated("software_architecture/models/b", "B");
        let c = repeated("software_architecture/models/c", "C");

        let report = lint_report_for_with_options(
            &[
                ("software_architecture/models/a", &a),
                ("software_architecture/models/b", &b),
                ("software_architecture/models/c", &c),
            ],
            OntologyLintOptions::scoped_to_component("software_architecture/models/b"),
        );

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::DedupBarLint
                && diagnostic.location.component_id == "software_architecture/models/b"
                && diagnostic
                    .message
                    .contains("architecture prose appears duplicated")
        }));
        assert!(
            report
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.location.component_id
                    == "software_architecture/models/b")
        );
    }

    #[test]
    fn test_validate_ontology_corpus_ignores_missing_derived_reference_targets() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company overview]

Company owns the business entity.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

## References

- `@ref data_model/company#D404` — missing block exists only in the index

[D1. Weak implementation]

The model applies @ref data_model/company#I1 because this weak overview ref is intentionally broad.
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);
        let rules = rule_ids(&report);

        assert!(!report.valid);
        assert!(rules.contains(&OntologyLintRuleId::RefQualityLint));
        assert!(!rules.contains(&OntologyLintRuleId::ValidateReferences));
    }

    #[test]
    fn test_validate_ontology_corpus_ignores_extends_plain_prose_targets() {
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Plain prose]

Check whether the imported class appears only in `extends`/`implements` clauses.
The migration test extends `g5-api/tests/api_issues_migration.rs`.
";

        let report = lint_report_for(&[("software_architecture/models/company", architecture)]);
        let rules = rule_ids(&report);

        assert!(!rules.contains(&OntologyLintRuleId::ValidateReferences));
    }

    #[test]
    fn test_validate_ontology_corpus_blocks_missing_inline_targets_and_weak_refs() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company overview]

Company owns the business entity.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Weak implementation]

The model depends on @ref data_model/company for its whole-node shape.
It also claims @ref data_model/company#D404
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);
        let rules = rule_ids(&report);

        assert!(!report.valid);
        assert!(rules.contains(&OntologyLintRuleId::ValidateReferences));
        assert!(rules.contains(&OntologyLintRuleId::RefQualityLint));
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::ValidateReferences
                && diagnostic.related_locations.iter().any(|location| {
                    location.component_id == "data_model/company"
                        && location.block_id.as_deref() == Some("D404")
                })
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_requires_real_rationale_or_binding_for_strong_refs() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D5. Tenant ownership invariant]

Company records carry tenant ownership and audit fields.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Bare strong ref]

Uses @ref data_model/company#D5.
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::RefQualityLint
                && diagnostic
                    .message
                    .contains("no rationale or Binding context")
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_blocks_repeated_ref_annotations() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D5. Tenant ownership invariant]

Company records carry tenant ownership, structured audit logging, and permission checks before returning data.
";
        let leaf = |component_id: &str, title: &str| {
            format!(
                "\
---
component_id: {component_id}
type: file
---
# {title}

[D1. Local model contract]

This leaf applies @ref data_model/company#D5 because every converted leaf uses the same boilerplate annotation.
"
            )
        };
        let a = leaf("software_architecture/models/a", "A");
        let b = leaf("software_architecture/models/b", "B");
        let c = leaf("software_architecture/models/c", "C");

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/a", &a),
            ("software_architecture/models/b", &b),
            ("software_architecture/models/c", &c),
        ]);

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::RefQualityLint
                && diagnostic
                    .message
                    .contains("identical concept-ref annotation set")
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_blocks_dedup_and_scaffolding() {
        let repeated = |component_id: &str, title: &str| {
            format!(
                "\
---
component_id: {component_id}
type: file
---
# {title}

[D1. Duplicated security prose]

Every request must enforce tenant scope, record owner filters, structured audit logging, and permission checks before returning data to callers.
"
            )
        };
        let scaffold = "\
---
component_id: software_architecture/models/scaffold
type: file
---
# Scaffold

## Legacy intent inventory

[D1. Converted content]

Mechanical migration content must not ship.
";
        let a = repeated("software_architecture/models/a", "A");
        let b = repeated("software_architecture/models/b", "B");
        let c = repeated("software_architecture/models/c", "C");

        let report = lint_report_for(&[
            ("software_architecture/models/a", &a),
            ("software_architecture/models/b", &b),
            ("software_architecture/models/c", &c),
            ("software_architecture/models/scaffold", scaffold),
        ]);
        let rules = rule_ids(&report);

        assert!(!report.valid);
        assert!(rules.contains(&OntologyLintRuleId::DedupBarLint));
        assert!(rules.contains(&OntologyLintRuleId::AntiScaffoldingLint));
    }

    #[test]
    fn test_validate_ontology_corpus_strips_labels_for_prose_similarity() {
        let left = normalize_prose_tokens(
            "[D5. Tenant ownership invariant]\n\nTenant scoped owner filters audit events permission checks.",
        );
        let right = normalize_prose_tokens(
            "[D1. Local restatement]\n\nTenant scoped owner filters audit events permission checks.",
        );

        assert_eq!(left, right);
        assert!(!left.contains("ownership"));
        assert!(!left.contains("invariant"));
        assert!(!right.contains("local"));
        assert!(!right.contains("restatement"));
    }

    #[test]
    fn test_validate_ontology_corpus_dedup_ref_must_cover_repeated_fact() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D5. Company row shape]

Company rows store legal names and billing identifiers for accounts.
";
        let repeated = |component_id: &str, title: &str| {
            format!(
                "\
---
component_id: {component_id}
type: file
---
# {title}

[D1. Repeated security prose]

Every request must enforce tenant scope, record owner filters, structured audit logging, and permission checks before returning data to callers. This file also uses @ref data_model/company#D5 because it stores company rows.
"
            )
        };
        let a = repeated("software_architecture/models/a", "A");
        let b = repeated("software_architecture/models/b", "B");
        let c = repeated("software_architecture/models/c", "C");

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/a", &a),
            ("software_architecture/models/b", &b),
            ("software_architecture/models/c", &c),
        ]);

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::DedupBarLint
                && diagnostic.message.contains("duplicated across 3 leaves")
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_blocks_concept_restatement_without_ref() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D5. Tenant ownership invariant]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Local restatement]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers.
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::DedupBarLint
                && diagnostic
                    .message
                    .contains("restates a concept block without an inline ref")
                && diagnostic
                    .evidence
                    .iter()
                    .any(|evidence| evidence.contains("data_model/company#D5"))
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_compares_module_concept_blocks_for_restatement() {
        let concept = "\
---
component_id: data_model
type: module
---
# Data Model

[D5. Tenant ownership invariant]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Local restatement]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers.
";

        let report = lint_report_for(&[
            ("data_model", concept),
            ("software_architecture/models/company", architecture),
        ]);

        assert!(!report.valid);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == OntologyLintRuleId::DedupBarLint
                && diagnostic
                    .evidence
                    .iter()
                    .any(|evidence| evidence.contains("data_model#D5"))
        }));
    }

    #[test]
    fn test_validate_ontology_corpus_allows_concept_restatement_with_exact_ref() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[D5. Tenant ownership invariant]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers.
";
        let architecture = "\
---
component_id: software_architecture/models/company
type: file
---
# Company Model

[D1. Local restatement]

Every company record must enforce tenant scoped owner filters, structured audit logging, and permission checks before returning data to callers. This local rule applies @ref data_model/company#D5 because the model owns company rows.
";

        let report = lint_report_for(&[
            ("data_model/company", concept),
            ("software_architecture/models/company", architecture),
        ]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn test_validate_ref_set_preservation_reports_changed_refs() {
        let before = parse_ontology_document(
            "software_architecture/models/company",
            "[D1. Surface]\nUses @ref data_model/company#D5.\n",
        );
        let after = parse_ontology_document(
            "software_architecture/models/company",
            "[D1. Surface]\nUses @ref data_model/company#D6.\n",
        );

        let report = validate_ref_set_preservation(&before, &after);

        assert!(!report.valid);
        assert_eq!(
            report.diagnostics[0].rule_id,
            OntologyLintRuleId::RefSetPreservationCheck
        );
    }

    #[test]
    fn test_validate_ontology_corpus_root_richness_gate() {
        let root = "\
---
component_id: root
type: root
---
# Root

[I1. Scope]

Short root.
";
        let mut options = OntologyLintOptions::default();
        options.root_baselines.insert(
            "root".to_string(),
            RootRichnessBaseline {
                original_byte_len: 1000,
                required_markers: vec!["diagram".to_string(), "adr".to_string()],
            },
        );
        let inputs = [OntologyLintInput {
            component_id: "root",
            content: root,
        }];

        let report = validate_ontology_corpus(&inputs, options);

        assert!(!report.valid);
        assert_eq!(
            report.diagnostics[0].rule_id,
            OntologyLintRuleId::RootRichnessGate
        );
    }

    #[test]
    fn test_validate_ontology_corpus_advisory_legacy_uncertain_spelling() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

<uncertain level=\"orange\">
[D5. Legacy-wrapped decision]

This block is wrapped in the legacy spelling the parser does not interpret.
</uncertain>
";

        let report = lint_report_for(&[("data_model/company", concept)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let diag = &report.diagnostics[0];
        assert_eq!(
            diag.rule_id,
            OntologyLintRuleId::LegacyUncertaintyTagSpelling
        );
        assert_eq!(diag.severity, OntologyLintSeverity::Advisory);
        assert_eq!(diag.location.component_id, "data_model/company");
        assert_eq!(diag.location.line, Some(11));
        assert_eq!(
            diag.evidence,
            vec!["<uncertain level=\"orange\">".to_string()]
        );
    }

    #[test]
    fn test_validate_ontology_corpus_advises_for_every_removed_uncertainty_wrapper() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

<uncertainty level=\"orange\">
[D5. Former orange decision]

All uncertainty levels were removed from canonical metadata.
</uncertainty>

<uncertainty level=\"normal\">
[D6. Former normal decision]

The wrapper must migrate without inventing a replacement level.
</uncertainty>

<uncertainty level=\"yellow\">
[D7. Former unknown-level decision]

Unknown legacy values are still surfaced for migration.
</uncertainty>
";

        let report = lint_report_for(&[("data_model/company", concept)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics.len(), 3, "{:?}", report.diagnostics);
        for diag in &report.diagnostics {
            assert_eq!(
                diag.rule_id,
                OntologyLintRuleId::NonAuthorableUncertaintyLevel
            );
            assert_eq!(diag.severity, OntologyLintSeverity::Advisory);
        }
        assert_eq!(report.diagnostics[0].location.line, Some(11));
        assert_eq!(
            report.diagnostics[0].evidence,
            vec!["<uncertainty level=\"orange\">".to_string()]
        );
        assert_eq!(report.diagnostics[1].location.line, Some(17));
        assert_eq!(
            report.diagnostics[1].evidence,
            vec!["<uncertainty level=\"normal\">".to_string()]
        );
        assert_eq!(report.diagnostics[2].location.line, Some(23));
        assert_eq!(
            report.diagnostics[2].evidence,
            vec!["<uncertainty level=\"yellow\">".to_string()]
        );
    }

    #[test]
    fn test_validate_ontology_corpus_advisory_unrecognized_label_kind() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

[E3. Edge case note]

An E-kind label is invisible to intent records and grounding.

[B4. Bullet note]

A B-kind label is equally invisible.

[Q2. Open question]

Legal question label; not flagged.
";

        let report = lint_report_for(&[("data_model/company", concept)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics.len(), 2, "{:?}", report.diagnostics);
        let first = &report.diagnostics[0];
        assert_eq!(
            first.rule_id,
            OntologyLintRuleId::UnrecognizedBlockLabelKind
        );
        assert_eq!(first.severity, OntologyLintSeverity::Advisory);
        assert_eq!(first.location.line, Some(11));
        assert_eq!(
            first.evidence,
            vec![
                "label_kind=E".to_string(),
                "[E3. Edge case note]".to_string()
            ]
        );
        let second = &report.diagnostics[1];
        assert_eq!(
            second.rule_id,
            OntologyLintRuleId::UnrecognizedBlockLabelKind
        );
        assert_eq!(second.location.line, Some(15));
        assert_eq!(
            second.evidence,
            vec!["label_kind=B".to_string(), "[B4. Bullet note]".to_string()]
        );
    }

    #[test]
    fn test_validate_ontology_corpus_advisory_uppercase_label_insertion_suffix() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

[D6D. Migration-era uppercase insertion]

This label is parser-live but is flagged when not grandfathered.

[D7a. Lowercase insertion]

A lowercase insertion suffix is the authorable form and is not flagged.
";

        let report = lint_report_for(&[("data_model/company", concept)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        let diag = &report.diagnostics[0];
        assert_eq!(
            diag.rule_id,
            OntologyLintRuleId::UppercaseLabelInsertionSuffix
        );
        assert_eq!(diag.severity, OntologyLintSeverity::Advisory);
        assert_eq!(diag.location.component_id, "data_model/company");
        assert_eq!(diag.location.block_id, Some("D6D".to_string()));
        assert_eq!(diag.location.line, Some(11));
        assert_eq!(
            diag.evidence,
            vec![
                "label=D6D".to_string(),
                "[D6D. Migration-era uppercase insertion]".to_string()
            ]
        );
    }

    #[test]
    fn test_validate_ontology_corpus_uppercase_label_suffix_grandfather_baseline() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity.

[D6D. Grandfathered uppercase insertion]

A baselined label is a live identity and is not flagged.
";

        let mut options = OntologyLintOptions::default();
        options
            .grandfathered_uppercase_labels
            .insert("data_model/company#D6D".to_string());
        let report = lint_report_for_with_options(&[("data_model/company", concept)], options);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }

    #[test]
    fn test_validate_ontology_corpus_advisory_checks_clean_fixture_no_false_positives() {
        let concept = "\
---
component_id: data_model/company
type: file
---
# Company

[I1. Company concept]

Company owns the business entity. The legacy `<uncertain>` spelling is
mentioned here only in prose backticks.
`<uncertain>` at line start is still backticked prose, not a tag.
[text](url) is a line-start markdown link and
[E3. label-shaped link text](url) is guarded by its trailing paren.

- [ ] checklist item with citation brackets [1]

[D1. Title]

A legal design label with a table-free body.

[I7a. x]

A legal insertion-suffix label.

[D2. Canonical uncertain decision] (Tags: uncertain)

The canonical record suffix is never flagged.

```markdown
<uncertain level=\"orange\">
[E9. fenced example]
<uncertainty level=\"normal\">
```
";

        let report = lint_report_for(&[("data_model/company", concept)]);

        assert!(report.valid, "{:?}", report.diagnostics);
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }
}

#[cfg(test)]
mod current_format_tests {
    use super::*;
    #[test]
    fn current_ontology_shared_fixtures() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/current-ontology.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let text = case["source"].as_str().unwrap();
            let records = current_ontology_records(text);
            assert_eq!(
                serde_json::json!(records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()),
                case["ids"],
                "{}",
                case["name"]
            );
            assert_eq!(
                records.iter().filter(|r| r.diagnostic.is_some()).count(),
                case["diagnostics"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            assert_eq!(
                records.iter().filter(|r| r.nested).count(),
                case["nested"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            let aliases: serde_json::Map<String, serde_json::Value> = records
                .iter()
                .filter(|r| !r.aliases.is_empty())
                .map(|r| (r.id.clone(), serde_json::json!(r.aliases)))
                .collect();
            assert_eq!(
                serde_json::Value::Object(aliases),
                case.get("aliases")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
                "{}",
                case["name"]
            );
            let document = parse_ontology_document(".g5/ontology/test.g5.md", text);
            assert_eq!(document.id_blocks.len(), records.len());
            assert_eq!(
                document.warnings.len(),
                case["warnings"].as_u64().unwrap() as usize,
                "{}",
                case["name"]
            );
            assert!(
                document.refs.is_empty(),
                "ordinary links never declare dependency edges"
            );
        }
    }
}
