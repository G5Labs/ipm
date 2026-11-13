use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::borrow::Cow;
use std::ops::Range;

use g5_contracts::types::ontology_markup::{
    is_semantic_record_metadata_suffix_candidate, parse_semantic_record_metadata_suffix,
    payload_records_user_answer, SemanticMetadataSuffixError, SemanticRecordMetadata,
    SemanticRecordSeverity,
};

use super::anchor_types::{ExtractError, ExtractedContent, ParseResult, ParsedBlock};

/// Extracts I/D/Q-labeled blocks from markdown content using `pulldown-cmark`
/// for structural analysis (code block exclusion) and line-level label scanning.
pub struct MarkdownAnchorExtractor;

/// Normalize `\r\n` and bare `\r` to `\n` so byte offset tracking and content
/// extraction work consistently regardless of platform line endings.
fn normalize_line_endings(content: &str) -> Cow<'_, str> {
    if content.contains('\r') {
        Cow::Owned(content.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(content)
    }
}

impl MarkdownAnchorExtractor {
    /// Extract all I/D/Q-labeled blocks from markdown content.
    pub fn extract(content: &str) -> ParseResult {
        let content = normalize_line_endings(content);
        let content: &str = &content;
        let current = g5_contracts::types::ontology_markup::current_ontology_records(content);
        if !current.is_empty() {
            let spans = g5_contracts::types::ontology_markup::parse_blocks(content);
            return ParseResult {
                warnings: current
                    .iter()
                    .filter_map(|r| r.diagnostic.clone())
                    .collect(),
                blocks: current
                    .iter()
                    .map(|r| ParsedBlock {
                        identifier: r.id.clone(),
                        title: String::new(),
                        is_human: false,
                        is_uncertain: r.diagnostic.is_some(),
                        priority: None,
                        certainty: None,
                        line_number: content[..spans[r.marker].start_offset]
                            .bytes()
                            .filter(|b| *b == b'\n')
                            .count()
                            + 1,
                        byte_range: spans[r.marker].start_offset
                            ..spans
                                .get(r.end)
                                .map(|b| b.start_offset)
                                .unwrap_or(content.len()),
                    })
                    .collect(),
            };
        }
        let excluded_ranges = collect_excluded_ranges(content);
        let mut blocks: Vec<ParsedBlock> = Vec::new();
        let mut heading_positions: Vec<(usize, u8)> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut byte_offset = 0;

        for (line_idx, line) in content.lines().enumerate() {
            let line_start = byte_offset;
            // Advance byte offset past the line + newline
            byte_offset += line.len();
            if byte_offset < content.len() {
                byte_offset += 1; // LF — content is normalized, no CRLF
            }

            let excluded = excluded_ranges
                .iter()
                .find(|excluded| excluded.range.contains(&line_start));
            let parsed_label = try_parse_label(line);
            let skip_line = excluded.is_some();
            let label_seen = parsed_label.is_some();
            let label_metadata = if !skip_line && label_seen {
                match label_line_semantic_metadata(line) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        warnings.push(format!(
                            "Invalid semantic metadata suffix at line {}: {}",
                            line_idx + 1,
                            error
                        ));
                        None
                    }
                }
            } else {
                None
            };
            // AUTHORED `human` only. `answered` on the label is an INDEX into
            // the payload, so trusting it here would persist a false
            // human-authorship bit for a label-only, open, agent-authored, or
            // corrupt question. Answered-based authorship is derived from the
            // payload below, once block ranges cover the fence.
            let label_has_human_metadata = label_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.human);
            let label_has_uncertain_metadata = label_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.uncertain);
            let line_heading_level = (!label_seen).then(|| heading_level(line)).flatten();

            if !skip_line {
                if let Some((ident, title)) = parsed_label {
                    blocks.push(ParsedBlock {
                        identifier: ident,
                        title,
                        is_human: label_has_human_metadata,
                        is_uncertain: label_has_uncertain_metadata,
                        priority: None,
                        certainty: None,
                        line_number: line_idx + 1,
                        byte_range: line_start..byte_offset,
                    });
                } else if let Some(level) = line_heading_level {
                    heading_positions.push((line_start, level));
                }
            }
        }

        // Fix byte ranges: extend each block to the start of the next block,
        // the next same/higher-level enclosing markdown heading, or EOF —
        // whichever comes first. Deeper headings remain part of the record.
        fix_block_ranges(content, &mut blocks, &heading_positions);

        // Ranges are final here, so each Q block now spans the fence it labels.
        // Authorship for an answered question is derived from the PAYLOAD, not
        // its label: D3 makes the payload authoritative, and a `(Tags: answered)`
        // the payload does not back must not register as human-authored.
        for block in &mut blocks {
            if block.is_human || !block.identifier.starts_with(['Q', 'q']) {
                continue;
            }
            if let Some(body) = content.get(block.byte_range.clone()) {
                block.is_human = payload_records_user_answer(body);
            }
        }

        // The label can now be checked against the payload it indexes.
        warnings.extend(detect_question_tag_drift(content, &blocks));

        // Detect duplicates
        detect_duplicates(&blocks, &mut warnings);

        ParseResult { blocks, warnings }
    }

    /// Extract the content of a specific I/D/Q block by identifier.
    pub fn extract_block(
        content: &str,
        identifier: &str,
    ) -> Result<ExtractedContent, ExtractError> {
        Self::extract_block_with_metadata(content, identifier).map(|(content, _)| content)
    }

    /// Reduce a stored legacy markdown block body to the canonical extracted
    /// shape: LF-normalized and right-trimmed. A first line is dropped only
    /// when it is itself a valid canonical I/D/Q label line; manual or
    /// label-less historical bodies are already content and retain their
    /// first line. Retired wrapper syntax is not interpreted here.
    /// This is exactly the shaping [`Self::extract_block_with_metadata`]
    /// applies to a freshly extracted block, exposed for the merge
    /// normalization and ledger migration that must fingerprint historical
    /// stored bodies (`block_extraction` spec D5).
    pub fn canonicalize_stored_block_body(body: &str) -> String {
        let normalized = normalize_line_endings(body);
        let first_line = normalized.lines().next().unwrap_or("");
        if try_parse_label(first_line).is_some() && label_line_semantic_metadata(first_line).is_ok()
        {
            strip_label_line(&normalized)
        } else {
            normalized.trim_end().to_string()
        }
    }

    /// Extract a specific I/D/Q block's content together with its parsed
    /// block metadata (normalized identifier, bracket title, authorship,
    /// line number, and a byte range into the parser's LF-normalized source)
    /// from the same parse — so callers never re-parse content to recover
    /// label/title metadata (parsing spec D2; consumed by the
    /// `block_extraction` canonical service).
    pub fn extract_block_with_metadata(
        content: &str,
        identifier: &str,
    ) -> Result<(ExtractedContent, ParsedBlock), ExtractError> {
        let content = normalize_line_endings(content);
        let content: &str = &content;
        let result = Self::extract(content);

        let matches: Vec<&ParsedBlock> = result
            .blocks
            .iter()
            .filter(|b| {
                if !g5_contracts::types::ontology_markup::current_ontology_records(content)
                    .is_empty()
                {
                    b.identifier == identifier
                } else {
                    b.identifier.eq_ignore_ascii_case(identifier)
                }
            })
            .collect();

        match matches.len() {
            0 => Err(ExtractError::NotFound),
            1 => {
                let block = matches[0];
                let block_content = &content[block.byte_range.clone()];
                let stripped = if block_content
                    .lines()
                    .next()
                    .is_some_and(|l| l.contains(" · "))
                {
                    block_content
                        .split_once('\n')
                        .map(|(_, body)| body.trim().to_string())
                        .unwrap_or_default()
                } else {
                    strip_label_line(block_content)
                };
                Ok((
                    ExtractedContent {
                        content: stripped,
                        boundary_uncertain: block.is_uncertain,
                    },
                    block.clone(),
                ))
            }
            _ => Err(ExtractError::AmbiguousDuplicate(
                matches.iter().map(|b| b.line_number).collect(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExcludedRange {
    range: Range<usize>,
    kind: ExcludedRangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExcludedRangeKind {
    CodeBlock,
    Html,
}

/// Use pulldown-cmark to find byte ranges of code blocks and non-G5 HTML
/// blocks that should be excluded from label scanning.
fn collect_excluded_ranges(content: &str) -> Vec<ExcludedRange> {
    let opts = Options::all();
    let parser = Parser::new_ext(content, opts).into_offset_iter();

    let mut excluded: Vec<ExcludedRange> = Vec::new();
    let mut code_block_start: Option<usize> = None;
    for (event, range) in parser {
        match &event {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_start = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    excluded.push(ExcludedRange {
                        range: start..range.end,
                        kind: ExcludedRangeKind::CodeBlock,
                    });
                }
            }
            Event::Html(_) => {
                excluded.push(ExcludedRange {
                    range,
                    kind: ExcludedRangeKind::Html,
                });
            }
            _ => {}
        }
    }

    excluded
}

/// Try to parse an I/D/Q label from text.
/// Pattern: `[<letter><digits><optional ascii suffix>. <optional title>]`.
/// Returns `(identifier, title)` if matched.
fn try_parse_label(text: &str) -> Option<(String, String)> {
    let text = text.trim_start();
    let rest = text.strip_prefix('[')?;

    let letter = rest.chars().next()?;
    if !matches!(letter, 'I' | 'i' | 'D' | 'd' | 'Q' | 'q') {
        return None;
    }

    let after_letter = &rest[letter.len_utf8()..];
    // Read digits
    let digit_end = after_letter
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_letter.len());
    if digit_end == 0 {
        return None; // no digits
    }
    let number = &after_letter[..digit_end];
    let after_digits = &after_letter[digit_end..];
    let suffix_end = after_digits
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_digits.len());
    let suffix = &after_digits[..suffix_end];
    let after_suffix = &after_digits[suffix_end..];

    // Expect '.'
    let after_dot = after_suffix.strip_prefix('.')?;

    // Find closing ']'
    let close = after_dot.find(']')?;
    let title = after_dot[..close].trim().to_string();

    let identifier = format!(
        "{}{}{}",
        letter.to_uppercase().next().unwrap(),
        number,
        suffix
    );
    Some((identifier, title))
}

fn label_line_semantic_metadata(
    line: &str,
) -> Result<Option<SemanticRecordMetadata>, SemanticMetadataSuffixError> {
    let Some(label_end) = label_end(line) else {
        return Ok(None);
    };
    let suffix = line[label_end..].trim();
    if !is_semantic_record_metadata_suffix_candidate(suffix) {
        return Ok(None);
    }
    // `answered` and severity are valid on Q records only, so the parser needs
    // to know which kind of label this suffix is attached to.
    let is_question = try_parse_label(line.trim_start())
        .is_some_and(|(identifier, _)| identifier.starts_with(['Q', 'q']));
    parse_semantic_record_metadata_suffix(suffix, is_question).map(Some)
}

/// What a `g5-question` payload asserts about its own state, read from the
/// fence a `[Q<N>.]` block contains.
struct QuestionPayloadFacts {
    severity: Option<SemanticRecordSeverity>,
    answered: bool,
}

/// Read the payload facts the label is supposed to mirror.
///
/// Deliberately a top-level line scan rather than a YAML parse: this feeds an
/// ADVISORY warning, so it must never be the reason a document fails to
/// register. Only unindented `severity:`/`status:` keys inside the first
/// `g5-question` fence are read — a nested `severity:` under `options:` is not
/// the question's severity — and anything it cannot read confidently yields
/// `None`, which reports nothing.
fn question_payload_facts(block_body: &str) -> Option<QuestionPayloadFacts> {
    let mut in_fence = false;
    let mut found = false;
    let mut facts = QuestionPayloadFacts {
        severity: None,
        answered: false,
    };
    for line in block_body.lines() {
        let trimmed = line.trim_start();
        if !in_fence {
            if trimmed.starts_with("```") && trimmed.trim_start_matches('`').trim() == "g5-question"
            {
                in_fence = true;
                found = true;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            break;
        }
        // Top-level keys only.
        if line.starts_with([' ', '\t', '-']) {
            continue;
        }
        if let Some(rest) = line.strip_prefix("severity:") {
            facts.severity = SemanticRecordSeverity::from_token(rest.trim());
        } else if let Some(rest) = line.strip_prefix("status:") {
            facts.answered = rest.trim() == "answered";
        }
    }
    found.then_some(facts)
}

/// Warn when a `[Q<N>.]` label's question tags disagree with the payload it
/// labels.
///
/// The label is an INDEX into the payload, so a disagreement is always a defect
/// in the label — either state the payload never asserted, or a missing entry
/// that leaves label-reading consumers (tag filters, registration, `is_human`)
/// blind to a severity the document does carry. Without this, both failures are
/// silent: a bare label parses fine and a contradicting token parses fine.
///
/// One asymmetry is deliberate. A payload that IS answered while the label
/// lacks `answered` is NOT reported: every document written before this grammar
/// looks like that, the card backfills it on the next write, and warning would
/// fire on legacy documents that are behaving exactly as designed. The reverse
/// — a label claiming `answered` over an unanswered payload — is reported,
/// because that is a claim of human authority the payload does not support.
fn detect_question_tag_drift(content: &str, blocks: &[ParsedBlock]) -> Vec<String> {
    let mut warnings = Vec::new();
    for block in blocks {
        if !block.identifier.starts_with(['Q', 'q']) {
            continue;
        }
        let Some(body) = content.get(block.byte_range.clone()) else {
            continue;
        };
        let first_line = body.lines().next().unwrap_or_default();
        let metadata = match label_line_semantic_metadata(first_line) {
            Ok(Some(metadata)) => metadata,
            // No suffix at all still needs the missing-severity report.
            Ok(None) => SemanticRecordMetadata {
                human: false,
                uncertain: false,
                answered: false,
                severity: None,
            },
            // Already reported as an invalid suffix; do not report twice.
            Err(_) => continue,
        };
        let Some(payload) = question_payload_facts(body) else {
            continue;
        };
        let line = block.line_number;
        let id = &block.identifier;
        match (payload.severity, metadata.severity) {
            (Some(payload_severity), None) => warnings.push(format!(
                "Question tag drift at line {line}: [{id}] payload sets \
                 `severity: {}` but the label carries no severity token; write \
                 `(Tags: {})` so label readers see it without parsing the fence",
                payload_severity.token(),
                payload_severity.token()
            )),
            (Some(payload_severity), Some(label_severity))
                if payload_severity != label_severity =>
            {
                warnings.push(format!(
                    "Question tag drift at line {line}: [{id}] label says \
                     `{}` but the payload sets `severity: {}`; the payload is \
                     authoritative, so fix the label",
                    label_severity.token(),
                    payload_severity.token()
                ));
            }
            (None, Some(label_severity)) => warnings.push(format!(
                "Question tag drift at line {line}: [{id}] label carries \
                 `{}` but the payload sets no `severity:`",
                label_severity.token()
            )),
            _ => {}
        }
        if metadata.answered && !payload.answered {
            warnings.push(format!(
                "Question tag drift at line {line}: [{id}] label claims \
                 `answered` but the payload is not answered — `answered` is \
                 written by the question card when a user answers, and asserts \
                 a human decision the payload does not record"
            ));
        }
    }
    warnings
}

/// Return the level of a markdown ATX heading (1-6 '#' chars followed by space or EOL).
fn heading_level(line: &str) -> Option<u8> {
    let trimmed = line.trim_start();
    let hash_count = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hash_count == 0 || hash_count > 6 {
        return None;
    }
    let after_hashes = &trimmed[hash_count..];
    (after_hashes.is_empty() || after_hashes.starts_with(' ') || after_hashes.starts_with('\t'))
        .then_some(hash_count as u8)
}

/// Fix block byte ranges so each block extends from its start to the
/// beginning of the next block, the next heading at the enclosing level or
/// higher, or EOF — whichever comes first.
/// If no heading encloses the block, any following heading is a boundary.
fn fix_block_ranges(content: &str, blocks: &mut [ParsedBlock], heading_positions: &[(usize, u8)]) {
    for i in 0..blocks.len() {
        let block_start = blocks[i].byte_range.start;
        let next_block_start = if i + 1 < blocks.len() {
            blocks[i + 1].byte_range.start
        } else {
            content.len()
        };

        let enclosing_heading_level = heading_positions
            .iter()
            .rev()
            // H1 is the SemLift document title, not a record-owning section.
            .find(|(position, level)| *position < block_start && *level >= 2)
            .map(|(_, level)| *level);
        let next_heading = heading_positions
            .iter()
            .find(|(position, level)| {
                *position > block_start
                    && enclosing_heading_level.is_none_or(|enclosing| *level <= enclosing)
            })
            .map(|(position, _)| *position)
            .unwrap_or(content.len());

        blocks[i].byte_range = block_start..next_block_start.min(next_heading);
    }
}

/// Strip the first line (label line) from block content, returning the rest.
fn strip_label_line(content: &str) -> String {
    let Some(label_end) = label_end(content.lines().next().unwrap_or("")) else {
        return match content.find('\n') {
            Some(idx) => content[idx + 1..].trim_end().to_string(),
            None => String::new(),
        };
    };

    let (first_line, rest) = match content.find('\n') {
        Some(idx) => (&content[..idx], Some(&content[idx + 1..])),
        None => (content, None),
    };

    let mut stripped = String::new();
    let same_line_tail = first_line[label_end..].trim_start();
    let tail_is_valid_metadata = matches!(label_line_semantic_metadata(first_line), Ok(Some(_)));
    if !same_line_tail.is_empty() && !tail_is_valid_metadata {
        stripped.push_str(same_line_tail);
        if rest.is_some_and(|r| !r.is_empty()) {
            stripped.push('\n');
        }
    }
    if let Some(rest) = rest {
        stripped.push_str(rest);
    }
    stripped.trim_end().to_string()
}

fn label_end(first_line: &str) -> Option<usize> {
    let offset = first_line.len() - first_line.trim_start().len();
    let rest = &first_line[offset..];
    try_parse_label(rest)?;
    rest.find(']').map(|idx| offset + idx + 1)
}

/// Detect duplicate identifiers and emit warnings.
/// Keeps only the first occurrence.
fn detect_duplicates(blocks: &[ParsedBlock], warnings: &mut Vec<String>) {
    let mut seen = std::collections::HashMap::new();
    for block in blocks {
        let key = block.identifier.to_uppercase();
        seen.entry(key)
            .or_insert_with(Vec::new)
            .push(block.line_number);
    }
    for (id, lines) in &seen {
        if lines.len() > 1 {
            warnings.push(format!(
                "Duplicate label [{}] found at lines {:?} — only first occurrence kept",
                id, lines
            ));
        }
    }
}

/// 1-based inclusive line span of the I/D/Q block labeled `label` in `text`.
/// The canonical parser defines block boundaries; the first occurrence wins.
///
/// Shared by the Intent Log and the Source Control Diff Graph Human-intent
/// lens so both git-blame fallbacks attribute the same committed line range.
pub fn block_line_range_by_label(label: &str, text: &str) -> Option<(usize, usize)> {
    let normalized = normalize_line_endings(text);
    let parsed = MarkdownAnchorExtractor::extract(&normalized);
    let block = parsed
        .blocks
        .iter()
        .find(|block| block.identifier.eq_ignore_ascii_case(label))?;
    let slice = normalized.get(block.byte_range.clone())?;
    let line_count = slice.lines().count().max(1);
    Some((block.line_number, block.line_number + line_count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEMANTIC_RECORD_EXTENT_PARITY: &str =
        include_str!("../../tests/fixtures/semantic-record-extent-parity.txt");

    #[test]
    fn repeated_empty_display_wikilinks_have_bounded_events() {
        let repetitions = 32;
        let input = "[[link|]]".repeat(repetitions);
        let expected_events = repetitions * 3 + 2;
        // RUSTSEC-2026-0414: cap iteration so a vulnerable parser fails
        // promptly instead of traversing an exponentially duplicated tail.
        let events: Vec<_> = Parser::new_ext(&input, Options::all())
            .take(expected_events + 1)
            .collect();
        assert_eq!(events.len(), expected_events);
        assert_eq!(events[0], Event::Start(Tag::Paragraph));
        assert_eq!(events.last(), Some(&Event::End(TagEnd::Paragraph)));
        for link in events[1..events.len() - 1].chunks_exact(3) {
            assert!(matches!(
                &link[0],
                Event::Start(Tag::Link { dest_url, .. }) if dest_url.as_ref() == "link"
            ));
            assert_eq!(link[1], Event::Text("".into()));
            assert_eq!(link[2], Event::End(TagEnd::Link));
        }
    }

    #[test]
    fn simple_intent_blocks() {
        let input = "[I1. Title One]\ncontent one\n[I2. Title Two]\ncontent two\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[0].title, "Title One");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[1].title, "Title Two");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn decision_blocks() {
        let input = "[D1. My Decision]\nThe chosen approach.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
        assert_eq!(result.blocks[0].title, "My Decision");
    }

    #[test]
    fn question_blocks() {
        let input = "[Q1. Open question]\nWhat should we do?\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "Q1");
        assert_eq!(result.blocks[0].title, "Open question");
    }

    #[test]
    fn mixed_blocks() {
        let input = "[I1. Intent]\nfoo\n[D2. Decision]\nbar\n[I3. Another Intent]\nbaz\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "D2");
        assert_eq!(result.blocks[2].identifier, "I3");
    }

    #[test]
    fn semantic_record_extents_match_shared_client_fixture() {
        let result = MarkdownAnchorExtractor::extract(SEMANTIC_RECORD_EXTENT_PARITY);
        let actual = result
            .blocks
            .iter()
            .map(|block| {
                (
                    block.identifier.as_str(),
                    SEMANTIC_RECORD_EXTENT_PARITY[block.byte_range.clone()].trim_end(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            actual,
            vec![
                (
                    "I1",
                    "[I1. Multi-paragraph intent]\n\nFirst paragraph.\n\nSecond paragraph.\n\n---\n\nAfter the divider."
                ),
                (
                    "D2",
                    "[D2. Canonical uncertain record] (Tags: uncertain)\n\nOwned body.\n\n### Nested inventory\n\n| Name | State |\n| --- | --- |\n| sample | open |"
                ),
                (
                    "D3",
                    "[D3. Record before another label]\n\nDesign body."
                ),
                ("Q4", "[Q4. Label at EOF]\n\nFinal question body."),
            ]
        );
    }

    #[test]
    fn inside_code_fence_ignored() {
        let input = "```\n[I5. Example]\n```\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn duplicate_numbers_warning() {
        let input = "[I3. A]\nfirst\n[I3. B]\nsecond\n";
        let result = MarkdownAnchorExtractor::extract(input);
        // Both are parsed but warning emitted
        assert_eq!(result.blocks.len(), 2);
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("Duplicate"));
    }

    #[test]
    fn no_blocks() {
        let input = "# Just a heading\nSome text\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn block_boundary_at_next_label() {
        let input = "[I1. A]\ncontent A\n[I2. B]\ncontent B\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        // I1's range should end where I2 starts
        assert_eq!(
            result.blocks[0].byte_range.end,
            result.blocks[1].byte_range.start
        );
    }

    #[test]
    fn block_at_eof() {
        let input = "[I1. A]\ncontent\nmore";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].byte_range.end, input.len());
    }

    #[test]
    fn multiline_content() {
        let input = "[I1. Title]\nline1\nline2\nline3\n[I2. Next]\nfoo\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
    }

    #[test]
    fn extract_specific_block() {
        let input = "[I1. A]\ncontent A\n[I2. B]\ncontent B\n[I3. C]\ncontent C\n";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();
        assert_eq!(extracted.content, "content B");
        assert!(!extracted.boundary_uncertain);
    }

    #[test]
    fn extract_nonexistent_block() {
        let input = "[I1. A]\ncontent A\n[I3. C]\ncontent C\n";
        let result = MarkdownAnchorExtractor::extract_block(input, "I2");
        assert!(matches!(result, Err(ExtractError::NotFound)));
    }

    #[test]
    fn empty_input() {
        let result = MarkdownAnchorExtractor::extract("");
        assert_eq!(result.blocks.len(), 0);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn case_insensitive_label_letter() {
        let input = "[i1. Lowercase]\ncontent\n[d2. Also Lower]\nmore\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        // Identifiers are normalized to uppercase
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "D2");
    }

    #[test]
    fn label_without_title() {
        let input = "[I1.]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[0].title, "");
    }

    // ── TC-PRS-001: Real-world ontology file ────────────────

    #[test]
    fn real_world_ontology_file_with_frontmatter_and_mixed_content() {
        let input = r#"---
component_id: g5/g5-api/auth
---

# Auth Module

## Boundary

[I1. Authentication Scope]
This module handles JWT-based authentication.

## Behavior

[I2. Token Validation]
Tokens are validated using HMAC-SHA256.

[D1. Refresh Token Strategy]
Refresh tokens use rotating key pairs.

### Implementation Notes

[I3. Key Rotation Interval]
Keys rotate every 24 hours.

<!-- [I99. This should be ignored] -->

```rust
// Example: [I50. This I/D label is inside a code fence]
fn example() {}
```

[Q1. Should we support OAuth2?]
Open question for future iteration.

## Edge Cases

[D2. Rate Limit Handling]
Rate limits use sliding window counters.
"#;
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(
            result.blocks.len(),
            6,
            "Expected 6 blocks, got {:?}",
            result
                .blocks
                .iter()
                .map(|b| &b.identifier)
                .collect::<Vec<_>>()
        );
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[0].title, "Authentication Scope");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[2].identifier, "D1");
        assert_eq!(result.blocks[3].identifier, "I3");
        assert_eq!(result.blocks[4].identifier, "Q1");
        assert_eq!(result.blocks[5].identifier, "D2");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn html_comment_labels_are_ignored() {
        let input = "<!-- [I99. Comment Label] -->\n[I1. Real Label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
    }

    #[test]
    fn multiline_html_comment_labels_ignored() {
        let input = "<!--\n[I5. Hidden]\nSome hidden content\n-->\n[D1. Visible]\nreal content\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
    }

    #[test]
    fn suffixed_intent_identifier_parses() {
        let input = "[D44a. Direct edit decision]\nbody\n[Q2b. Follow-up]\nquestion\n";
        let result = MarkdownAnchorExtractor::extract(input);
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|block| block.identifier.as_str())
            .collect();
        assert_eq!(ids, vec!["D44a", "Q2b"]);
    }

    /// `[Q1.]` block with the given label suffix and payload lines.
    fn question_doc(label_suffix: &str, payload: &str) -> String {
        format!(
            "[Q1. Pick a backend]{label_suffix}\n\nContext prose.\n\n\
             ```g5-question\nschema: 1\nid: pick-backend\n{payload}\
             prompt: Which backend?\noptions:\n  - label: Postgres\n    \
             description: reuse it\n    severity: ignored-nested-key\n```\n"
        )
    }

    fn drift_warnings(doc: &str) -> Vec<String> {
        MarkdownAnchorExtractor::extract(doc)
            .warnings
            .into_iter()
            .filter(|w| w.contains("Question tag drift"))
            .collect()
    }

    #[test]
    fn reports_a_label_missing_the_payload_severity() {
        // The case the enforcement exists for: an agent authored the payload
        // but not the token, so every label-reading consumer is blind to a
        // severity the document does carry.
        let warnings = drift_warnings(&question_doc("", "status: open\nseverity: high\n"));
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(warnings[0].contains("`severity: high`"), "{}", warnings[0]);
        assert!(warnings[0].contains("(Tags: high)"), "{}", warnings[0]);
    }

    #[test]
    fn reports_a_label_severity_that_contradicts_the_payload() {
        let warnings = drift_warnings(&question_doc(
            " (Tags: low)",
            "status: open\nseverity: critical\n",
        ));
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(warnings[0].contains("label says `low`"), "{}", warnings[0]);
    }

    #[test]
    fn reports_a_label_severity_the_payload_does_not_set() {
        let warnings = drift_warnings(&question_doc(" (Tags: high)", "status: open\n"));
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(
            warnings[0].contains("sets no `severity:`"),
            "{}",
            warnings[0]
        );
    }

    #[test]
    fn reports_an_answered_label_over_an_unanswered_payload() {
        // A claim of human authority the payload does not support.
        let warnings = drift_warnings(&question_doc(" (Tags: answered)", "status: open\n"));
        assert_eq!(warnings.len(), 1, "got {warnings:?}");
        assert!(warnings[0].contains("claims `answered`"), "{}", warnings[0]);
    }

    #[test]
    fn stays_silent_when_the_label_mirrors_the_payload() {
        // Correct authoring, and the nested `severity:` under `options:` must
        // not be mistaken for the question's own.
        assert!(drift_warnings(&question_doc(
            " (Tags: high)",
            "status: open\nseverity: high\n"
        ))
        .is_empty());
        // No severity anywhere, and authored tags alone.
        assert!(drift_warnings(&question_doc(" (Tags: human)", "status: open\n")).is_empty());
        // A pre-grammar ANSWERED document is not nagged: the card backfills the
        // tag on its next write, and this is exactly how legacy docs look.
        assert!(drift_warnings(&question_doc("", "status: answered\n")).is_empty());
    }

    #[test]
    fn ignores_blocks_that_are_not_questions() {
        let doc = "[D1. A decision] (Tags: human)\n\nBody with no fence.\n";
        assert!(drift_warnings(doc).is_empty());
    }

    #[test]
    fn an_answered_question_registers_as_human_authored() {
        // Authorship comes from the PAYLOAD: a user answer makes the record
        // human, so its intent record keeps the authorship protection. The
        // label token alone must not, or a hand-written or drifted
        // `(Tags: answered)` would persist a false human-authorship bit.
        let user_answer = question_doc(
            " (Tags: answered)",
            "status: answered\nanswer:\n  answered_by:\n    kind: user\n",
        );
        let blocks = MarkdownAnchorExtractor::extract(&user_answer).blocks;
        assert!(blocks[0].is_human, "a user answer is human-authored");

        for (label, payload, why) in [
            (
                " (Tags: answered)",
                "status: open\n",
                "label-only over an open payload",
            ),
            (
                "",
                "status: answered\nanswer:\n  answered_by:\n    kind: agent\n",
                "agent provenance",
            ),
            (
                "",
                "status: answered\n",
                "answered with no provenance at all",
            ),
            (
                "",
                "status: answered\nanswer:\n  answered_by:\n    kind: user\n    kind: agent\n",
                "duplicate keys — corrupt, so not authoritative",
            ),
        ] {
            let doc = question_doc(label, payload);
            let blocks = MarkdownAnchorExtractor::extract(&doc).blocks;
            assert!(!blocks[0].is_human, "{why} must not register as human");
        }
    }

    #[test]
    fn question_state_tags_do_not_parse_on_an_id_label() {
        // Q-only vocabulary on a D record is not canonical, so the suffix is
        // reported as invalid rather than silently registering the record as
        // human-authored on the strength of a tag it may not carry.
        let input = "[D1. A decision] (Tags: answered)\nBody.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert!(
            result.warnings.iter().any(|warning| {
                let lowered = warning.to_lowercase();
                lowered.contains("tag") || lowered.contains("metadata")
            }),
            "expected a semantic-metadata warning, got {:?}",
            result.warnings
        );
        assert!(
            result.blocks.iter().all(|block| !block.is_human),
            "an invalid suffix must not confer human authorship"
        );
    }

    #[test]
    fn canonical_semantic_suffixes_are_agent_readable() {
        let input = "[D44a. Direct edit] (Tags: human)\nHuman body.\n[Q7c. Needs review] (Tags: uncertain)\nQuestion body.\n[I1. Both] (Tags: human, uncertain)\nBoth body.\n";
        let result = MarkdownAnchorExtractor::extract(input);

        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].title, "Direct edit");
        assert!(result.blocks[0].is_human);
        assert!(!result.blocks[0].is_uncertain);
        assert!(!result.blocks[1].is_human);
        assert!(result.blocks[1].is_uncertain);
        assert!(result.blocks[2].is_human);
        assert!(result.blocks[2].is_uncertain);
        assert!(result.warnings.is_empty());
        let human = MarkdownAnchorExtractor::extract_block(input, "D44a").unwrap();
        assert_eq!(human.content, "Human body.");
        assert!(!human.boundary_uncertain);
        let uncertain = MarkdownAnchorExtractor::extract_block(input, "Q7c").unwrap();
        assert_eq!(uncertain.content, "Question body.");
        assert!(uncertain.boundary_uncertain);
        let combined = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert_eq!(combined.content, "Both body.");
        assert!(combined.boundary_uncertain);
    }

    #[test]
    fn removed_wrapper_syntax_does_not_create_records_or_human_authority() {
        let input = "<human origin=\"direct_edit\">[D44a. Removed]</human>\n[I1. Visible]\nbody\n";
        let result = MarkdownAnchorExtractor::extract(input);

        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert!(!result.blocks[0].is_human);
    }

    #[test]
    fn removed_metadata_forms_fail_closed_and_remain_in_agent_content() {
        for suffix in [
            "(Tags: human origin=\"direct_edit\")",
            "(Tags: uncertainty-high source=\"agent\")",
            "(Tags: human-provided)",
            "(Tags: uncertain, human)",
            "(tags: human)",
        ] {
            let input = format!("[D8. Invalid] {suffix}\nBody.\n");
            let result = MarkdownAnchorExtractor::extract(&input);
            assert_eq!(result.blocks.len(), 1);
            assert!(!result.blocks[0].is_human);
            assert_eq!(result.warnings.len(), 1);
            assert!(result.warnings[0].contains("Invalid semantic metadata suffix"));
            let extracted = MarkdownAnchorExtractor::extract_block(&input, "D8").unwrap();
            assert!(extracted.content.starts_with(suffix));
        }
    }

    #[test]
    fn prose_human_mentions_do_not_leak_human_metadata() {
        let input = "The `<human>` tag marks protected blocks.\n[I1. Plain]\nbody\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert!(!result.blocks[0].is_human);
    }

    #[test]
    fn inline_human_label_inside_code_fence_is_ignored() {
        let input = r#"```
<human origin="direct_edit">[D44a. Hidden]</human>
```
[D1. Visible]
body
"#;
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
        assert!(!result.blocks[0].is_human);
    }

    #[test]
    fn labels_inside_non_g5_html_blocks_are_ignored() {
        let input = r#"<div>
[D44a. Hidden]
</div>

[D1. Visible]
body
"#;
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
    }

    // ── TC-PRS-002: Duplicate identifiers ───────────────────

    #[test]
    fn duplicate_identifiers_emit_warning_with_line_numbers() {
        let input = "[I3. First]\ncontent A\n[I3. Second]\ncontent B\n[I1. Unique]\ncontent C\n";
        let result = MarkdownAnchorExtractor::extract(input);
        // Both are parsed
        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].identifier, "I3");
        assert_eq!(result.blocks[0].title, "First");
        assert_eq!(result.blocks[1].identifier, "I3");
        assert_eq!(result.blocks[1].title, "Second");
        // Warning emitted
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("I3"));
    }

    #[test]
    fn extract_block_with_duplicate_returns_ambiguous_error() {
        let input = "[I3. First]\ncontent A\n[I3. Second]\ncontent B\n";
        let result = MarkdownAnchorExtractor::extract_block(input, "I3");
        assert!(matches!(result, Err(ExtractError::AmbiguousDuplicate(_))));
        if let Err(ExtractError::AmbiguousDuplicate(lines)) = result {
            assert_eq!(lines.len(), 2);
        }
    }

    // ── TC-PRS-003: Adjacent blocks with no content ─────────

    #[test]
    fn adjacent_blocks_with_no_content() {
        let input = "[I1. First Intent]\n[I2. Second Intent]\n[D1. First Decision]\nSome actual content here.\n[D2. Second Decision]\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[2].identifier, "D1");
        assert_eq!(result.blocks[3].identifier, "D2");

        // I1 content should be empty (just label line, next block immediately follows)
        let i1_content = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(
            i1_content.content.trim().is_empty(),
            "I1 should have empty content, got: {:?}",
            i1_content.content
        );

        // D1 should have content
        let d1_content = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(d1_content.content.contains("Some actual content here."));

        // Byte ranges don't overlap
        for i in 0..result.blocks.len() - 1 {
            assert!(
                result.blocks[i].byte_range.end <= result.blocks[i + 1].byte_range.start
                    || result.blocks[i].byte_range.end == result.blocks[i + 1].byte_range.start,
                "Block {} and {} overlap: {:?} vs {:?}",
                result.blocks[i].identifier,
                result.blocks[i + 1].identifier,
                result.blocks[i].byte_range,
                result.blocks[i + 1].byte_range,
            );
        }
    }

    // ── TC-PRS-004: Varied label formatting ─────────────────

    #[test]
    fn label_no_space_after_period() {
        let input = "[D2.Complex label without space after period]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D2");
        assert_eq!(
            result.blocks[0].title,
            "Complex label without space after period"
        );
    }

    #[test]
    fn label_extra_spaces() {
        let input = "[I3.  Extra spaces in label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I3");
        assert_eq!(result.blocks[0].title, "Extra spaces in label");
    }

    #[test]
    fn high_number_labels() {
        let input = "[Q10. Very high number]\ncontent A\n[I100. Three-digit number]\ncontent B\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "Q10");
        assert_eq!(result.blocks[1].identifier, "I100");
    }

    #[test]
    fn all_varied_formats_in_one_file() {
        let input = "[I1. Simple label]\nContent A.\n\n[D2.Complex label without space after period]\nContent B.\n\n[I3.  Extra spaces in label]\nContent C.\n\n[Q10. Very high number]\nContent D.\n\n[I100. Three-digit number]\nContent E.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 5);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "D2");
        assert_eq!(result.blocks[2].identifier, "I3");
        assert_eq!(result.blocks[3].identifier, "Q10");
        assert_eq!(result.blocks[4].identifier, "I100");
    }

    // ── TC-PRS-005: Nested code fences ──────────────────────

    #[test]
    fn nested_code_fences_with_label_inside() {
        let input = "[I1. Before code fence]\nSome content.\n\n````markdown\n[I2. This is inside a nested code fence]\n```\ninner fence\n```\n````\n\n[I3. After code fence]\nMore content.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|b| b.identifier.as_str())
            .collect();
        assert!(ids.contains(&"I1"), "Should contain I1");
        assert!(ids.contains(&"I3"), "Should contain I3");
        assert!(
            !ids.contains(&"I2"),
            "I2 inside code fence should NOT be extracted"
        );
    }

    #[test]
    fn language_annotated_code_fence() {
        let input = "```python\n[I1. Label inside python fence]\ndef foo(): pass\n```\n[D1. Real label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
    }

    // ── TC-PRS-006: Extract specific block content ──────────

    #[test]
    fn extract_block_preserves_multiline_content() {
        let input = "[I1. First]\nline 1 of I1\nline 2 of I1\n[I2. Second]\nmulti\nline\ncontent\n[I3. Third]\nthird content\n";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();
        assert!(extracted.content.contains("multi"));
        assert!(extracted.content.contains("line"));
        assert!(extracted.content.contains("content"));
        assert!(!extracted.content.contains("line 1 of I1"));
        assert!(!extracted.content.contains("third content"));
    }

    #[test]
    fn extract_block_at_end_of_file() {
        let input = "[I1. First]\ncontent1\n[D1. Last]\nlast line 1\nlast line 2";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(extracted.content.contains("last line 1"));
        assert!(extracted.content.contains("last line 2"));
    }

    #[test]
    fn extract_block_case_insensitive() {
        let input = "[I1. Title]\ncontent\n";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "i1").unwrap();
        assert!(extracted.content.contains("content"));
    }

    // ── Block line numbers ──────────────────────────────────

    #[test]
    fn line_numbers_are_one_indexed() {
        let input = "# Header\n\n[I1. Label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].line_number, 3);
    }

    #[test]
    fn line_numbers_correct_with_frontmatter() {
        let input = "---\ntitle: test\n---\n\n[I1. Label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        // YAML frontmatter lines are counted
        assert_eq!(result.blocks[0].line_number, 5);
    }

    // ── Byte range accuracy ─────────────────────────────────

    #[test]
    fn byte_ranges_cover_full_content_without_gaps() {
        let input = "[I1. A]\nfoo\n[I2. B]\nbar\n[D1. C]\nbaz\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);

        // First block starts at 0
        assert_eq!(result.blocks[0].byte_range.start, 0);
        // Last block extends to EOF
        assert_eq!(result.blocks[2].byte_range.end, input.len());
        // No gaps between blocks
        assert_eq!(
            result.blocks[0].byte_range.end,
            result.blocks[1].byte_range.start
        );
        assert_eq!(
            result.blocks[1].byte_range.end,
            result.blocks[2].byte_range.start
        );
    }

    #[test]
    fn byte_range_content_matches_original() {
        let input = "[I1. Title]\ncontent line 1\ncontent line 2\n[I2. Next]\nfoo\n";
        let result = MarkdownAnchorExtractor::extract(input);
        let block_text = &input[result.blocks[0].byte_range.clone()];
        assert!(block_text.starts_with("[I1. Title]"));
        assert!(block_text.contains("content line 1"));
        assert!(block_text.contains("content line 2"));
        assert!(!block_text.contains("[I2. Next]"));
    }

    // ── CRLF handling ───────────────────────────────────────

    #[test]
    fn crlf_line_endings_extract_correct_blocks() {
        let input = "[I1. Title One]\r\ncontent one\r\n[I2. Title Two]\r\ncontent two\r\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[0].title, "Title One");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[1].title, "Title Two");
    }

    #[test]
    fn crlf_byte_ranges_no_gaps() {
        let input = "[I1. A]\r\nfoo\r\n[I2. B]\r\nbar\r\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        // Byte ranges are relative to the normalized (LF-only) content
        assert_eq!(result.blocks[0].byte_range.start, 0);
        assert_eq!(
            result.blocks[0].byte_range.end,
            result.blocks[1].byte_range.start
        );
    }

    #[test]
    fn crlf_extract_block_content_no_stray_cr() {
        let input = "[I1. Title]\r\nline1\r\nline2\r\n[I2. Next]\r\nfoo\r\n";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(
            !extracted.content.contains('\r'),
            "Extracted content should not contain \\r"
        );
        assert!(extracted.content.contains("line1"));
        assert!(extracted.content.contains("line2"));
    }

    #[test]
    fn mixed_line_endings_handled() {
        // Mix of LF and CRLF in same file
        let input = "[I1. A]\ncontent one\r\n[I2. B]\r\ncontent two\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "I2");
    }

    #[test]
    fn crlf_code_fence_exclusion() {
        let input = "```\r\n[I5. Inside fence]\r\n```\r\n[I1. Outside]\r\ncontent\r\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
    }

    #[test]
    fn bare_cr_line_endings() {
        // Bare \r (old Mac-style) — rare but str::lines() handles it
        let input = "[I1. Title]\rcontent\r[I2. Next]\rfoo\r";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "I2");
    }

    // ── Label pattern edge cases ────────────────────────────

    #[test]
    fn label_missing_closing_bracket_not_parsed() {
        let input = "[I1. Missing bracket\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn label_missing_period_not_parsed() {
        let input = "[I1 No Period]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn label_with_no_number_not_parsed() {
        let input = "[I. No Number]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn label_wrong_prefix_not_parsed() {
        let input = "[X1. Wrong Prefix]\n[A2. Also Wrong]\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn label_with_leading_whitespace() {
        let input = "  [I1. Indented Label]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
    }

    #[test]
    fn label_with_text_after_bracket() {
        // Text after the closing bracket should not affect parsing
        let input = "[I1. Title] some trailing text\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
    }

    #[test]
    fn label_with_special_chars_in_title() {
        let input = "[I1. Title with (parens), 'quotes', & symbols!]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert!(result.blocks[0].title.contains("parens"));
    }

    #[test]
    fn label_with_brackets_in_title() {
        // Closing bracket inside title ends the label early
        let input = "[I1. Title with [nested] brackets]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        // try_parse_label finds first ']' — title would be "Title with [nested"
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");
    }

    // ── Content edge cases ──────────────────────────────────

    #[test]
    fn only_whitespace_content() {
        let input = "   \n\n  \n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn file_with_only_headings_and_prose() {
        let input = "# Top Level\n\nSome text.\n\n## Sub Section\n\nMore text.\n\n### Deep\n\nDeepest text.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 0);
    }

    #[test]
    fn file_ending_without_newline() {
        let input = "[I1. A]\ncontent without trailing newline";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].byte_range.end, input.len());

        let extracted = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert_eq!(extracted.content, "content without trailing newline");
    }

    #[test]
    fn unicode_content_in_blocks() {
        let input = "[I1. 日本語タイトル]\nコンテンツ here.\n[D1. Ünïcödé]\nMöre cöntënt.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[0].title, "日本語タイトル");
        assert_eq!(result.blocks[1].identifier, "D1");
    }

    #[test]
    fn many_blocks_in_sequence() {
        let mut input = String::new();
        for i in 1..=50 {
            input.push_str(&format!(
                "[I{}. Block number {}]\nContent for block {}.\n",
                i, i, i
            ));
        }
        let result = MarkdownAnchorExtractor::extract(&input);
        assert_eq!(result.blocks.len(), 50);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[49].identifier, "I50");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn blocks_separated_by_blank_lines_and_headings() {
        let input = "# Section 1\n\n[I1. First]\nContent A.\n\n## Subsection\n\n[D1. Decision]\nContent B.\n\n# Section 2\n\n[I2. Second]\nContent C.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "D1");
        assert_eq!(result.blocks[2].identifier, "I2");
    }

    // ── Code fence variations ───────────────────────────────

    #[test]
    fn tilde_code_fence_excluded() {
        let input = "~~~\n[I5. Inside tilde fence]\n~~~\n[I1. Outside]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|b| b.identifier.as_str())
            .collect();
        assert!(ids.contains(&"I1"));
        assert!(
            !ids.contains(&"I5"),
            "I5 inside tilde code fence should be ignored"
        );
    }

    #[test]
    fn multiple_code_fences_with_labels_inside() {
        let input = "[I1. Before]\ncontent\n```\n[I2. Hidden A]\n```\n[I3. Between]\nmore\n```\n[I4. Hidden B]\n```\n[I5. After]\nfinal\n";
        let result = MarkdownAnchorExtractor::extract(input);
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|b| b.identifier.as_str())
            .collect();
        assert_eq!(ids, vec!["I1", "I3", "I5"]);
    }

    #[test]
    fn code_fence_at_start_of_file() {
        let input = "```\n[I1. Inside fence]\n```\n[D1. After fence]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "D1");
    }

    // ── Complex real-world patterns ─────────────────────────

    #[test]
    fn interleaved_headings_blocks_and_fences() {
        let input = r#"# Module Design

## Overview

[I1. Authentication Module]
Handles user authentication and authorization.

## Requirements

### Core Requirements

[I2. JWT Token Support]
Must support JWT-based auth with configurable expiry.

[I3. API Key Support]
Also support API key auth for service-to-service calls.

### Non-functional

[D1. Token Expiry Default]
Default token expiry is 1 hour.

## Implementation

```rust
// This is just an example snippet
// [I99. Not a real label]
pub struct AuthConfig {
    pub token_expiry_secs: u64,
}
```

## Open Questions

[Q1. Multi-Tenant Isolation]
How do we isolate auth tokens across tenants?
"#;
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 5);
        assert_eq!(result.blocks[0].identifier, "I1");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[2].identifier, "I3");
        assert_eq!(result.blocks[3].identifier, "D1");
        assert_eq!(result.blocks[4].identifier, "Q1");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn extract_block_from_complex_file() {
        let input = "# Header\n\n[I1. First]\nA paragraph about I1.\nAnother line.\n\n[D1. Important Decision]\nWe decided to use approach X.\nRationale: lower latency.\n\n[I2. Second]\nSome other content.\n";
        let extracted = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(extracted.content.contains("We decided to use approach X."));
        assert!(extracted.content.contains("Rationale: lower latency."));
        assert!(!extracted.content.contains("A paragraph about I1."));
        assert!(!extracted.content.contains("Some other content."));
    }

    #[test]
    fn inline_code_with_label_like_content() {
        // Inline code (single backticks) does NOT create an exclusion zone
        // in pulldown-cmark for our purposes — only fenced code blocks do.
        // But the label pattern requires line-start `[`, so inline code content
        // within a paragraph won't match anyway.
        let input = "Some text with `[I5. inline code]` in a paragraph.\n[I1. Real]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        // The inline `[I5. inline code]` might match depending on implementation.
        // The critical assertion is that I1 is found.
        assert!(result.blocks.iter().any(|b| b.identifier == "I1"));
    }

    #[test]
    fn label_on_very_first_line() {
        let input = "[I1. Very First]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].line_number, 1);
        assert_eq!(result.blocks[0].byte_range.start, 0);
    }

    #[test]
    fn single_label_no_content() {
        let input = "[I1. Only Label]";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);
        assert_eq!(result.blocks[0].identifier, "I1");

        let extracted = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(extracted.content.is_empty());
    }

    #[test]
    fn many_decision_blocks_only() {
        let input = "[D1. Dec A]\ncontent A\n[D2. Dec B]\ncontent B\n[D3. Dec C]\ncontent C\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);
        for (i, block) in result.blocks.iter().enumerate() {
            assert_eq!(block.identifier, format!("D{}", i + 1));
        }
    }

    #[test]
    fn mixed_i_d_q_all_types() {
        let input = "[I1. Intent]\nfoo\n[D1. Decision]\nbar\n[Q1. Question]\nbaz\n[I2. Intent2]\nqux\n[D2. Decision2]\nquux\n[Q2. Question2]\ncorge\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 6);
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|b| b.identifier.as_str())
            .collect();
        assert_eq!(ids, vec!["I1", "D1", "Q1", "I2", "D2", "Q2"]);
    }

    #[test]
    fn non_sequential_numbering() {
        let input = "[I5. Fifth]\ncontent\n[I2. Second]\ncontent\n[D10. Tenth]\ncontent\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);
        assert_eq!(result.blocks[0].identifier, "I5");
        assert_eq!(result.blocks[1].identifier, "I2");
        assert_eq!(result.blocks[2].identifier, "D10");
    }

    #[test]
    fn cross_type_duplicate_numbers_no_warning() {
        // I1 and D1 share the number 1 but have different prefixes — NOT a duplicate
        let input = "[I1. Intent]\nfoo\n[D1. Decision]\nbar\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);
        assert!(
            result.warnings.is_empty(),
            "I1 and D1 should NOT trigger duplicate warning"
        );
    }

    // ── Heading terminates block ────────────────────────────

    #[test]
    fn heading_terminates_block_content() {
        let input = "[I1. First]\nContent A.\n\n## Unlabeled Section\n\nNot part of I1.\n\n[I2. Second]\nContent B.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            !i1.content.contains("Unlabeled Section"),
            "Heading should terminate I1 block"
        );
        assert!(
            !i1.content.contains("Not part of I1."),
            "Content after heading should not be in I1"
        );

        let i2 = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();
        assert!(i2.content.contains("Content B."));
    }

    #[test]
    fn heading_at_end_terminates_last_block() {
        let input = "[I1. Only]\nContent.\n\n## Trailing Section\n\nTrailing text.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 1);

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(!i1.content.contains("Trailing Section"));
        assert!(!i1.content.contains("Trailing text."));
    }

    #[test]
    fn any_heading_level_terminates_block() {
        // All heading levels (# through ######) should terminate a block
        for level in 1..=6 {
            let hashes = "#".repeat(level);
            let input = format!(
                "[I1. Block]\nContent.\n\n{} Heading\n\nAfter heading.\n",
                hashes
            );
            let i1 = MarkdownAnchorExtractor::extract_block(&input, "I1").unwrap();
            assert!(
                i1.content.contains("Content."),
                "Level {} heading: content before heading should be present",
                level
            );
            assert!(
                !i1.content.contains("After heading."),
                "Level {} heading should terminate block",
                level
            );
        }
    }

    #[test]
    fn heading_inside_code_fence_does_not_terminate_block() {
        let input = "[I1. Block]\nContent A.\n\n```\n## Heading in code fence\n```\n\nContent B still in I1.\n[I2. Next]\nfoo\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            i1.content.contains("Content B still in I1."),
            "Heading inside code fence should not terminate block"
        );
    }

    #[test]
    fn block_byte_range_ends_at_heading() {
        let input = "[I1. A]\nfoo\n## Heading\nbar\n[I2. B]\nbaz\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 2);

        // I1's range should end at the heading, not at I2
        let i1_end = result.blocks[0].byte_range.end;
        let heading_start = input.find("## Heading").unwrap();
        assert_eq!(
            i1_end, heading_start,
            "I1 block should end at heading start"
        );
    }

    // ── Comprehensive block boundary tests ──────────────────
    // Rule: block content = label line → min(next I/D/Q label, next heading, EOF)

    #[test]
    fn label_immediately_before_heading_yields_empty_content() {
        // Label on one line, heading on the very next line — block content is empty
        let input = "[I1. Empty]\n## Heading\nstuff\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(
            i1.content.trim().is_empty(),
            "Block immediately followed by heading should have empty content, got: {:?}",
            i1.content
        );
    }

    #[test]
    fn label_wins_over_heading_when_label_comes_first() {
        // Next I/D/Q label comes before next heading — label terminates the block
        let input = "[I1. First]\nContent A.\n[I2. Second]\nContent B.\n## Section\nAfter.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            !i1.content.contains("Content B."),
            "I2 label should terminate I1 before the heading"
        );
    }

    #[test]
    fn heading_wins_over_label_when_heading_comes_first() {
        // Next heading comes before next I/D/Q label — heading terminates the block
        let input = "[I1. First]\nContent A.\n## Section\nMiddle.\n[I2. Second]\nContent B.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            !i1.content.contains("Section"),
            "Heading should terminate I1 before I2"
        );
        assert!(!i1.content.contains("Middle."));
    }

    #[test]
    fn eof_terminates_last_block_no_heading_no_label() {
        // No heading or label after the last block — EOF terminates it
        let input = "[D1. Only Block]\nLine 1.\nLine 2.\nLine 3.";
        let d1 = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(d1.content.contains("Line 1."));
        assert!(d1.content.contains("Line 2."));
        assert!(d1.content.contains("Line 3."));
    }

    #[test]
    fn multiple_headings_between_labels_first_heading_terminates() {
        // Two headings between I1 and I2 — only the first heading matters for I1's boundary
        let input =
            "[I1. First]\nContent A.\n## H1\nMiddle.\n### H2\nMore.\n[I2. Second]\nContent B.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            !i1.content.contains("H1"),
            "First heading should terminate I1"
        );
        assert!(!i1.content.contains("Middle."));
    }

    #[test]
    fn heading_on_same_level_as_label_section() {
        // Label is under ## Section, then another ## Section appears — terminates
        let input = "## Section A\n\n[I1. Intent]\nContent in section A.\n\n## Section B\n\nContent in section B.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content in section A."));
        assert!(!i1.content.contains("Section B"));
        assert!(!i1.content.contains("Content in section B."));
    }

    #[test]
    fn deeper_heading_remains_inside_enclosing_record() {
        // A record under ## owns nested ### subsections such as table inventories.
        let input = "## Top\n\n[I1. Intent]\nContent.\n\n### Sub-section\n\nSub content.\n\n## Next\n\nOutside.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            i1.content.contains("Sub-section"),
            "a deeper heading should remain in the enclosing record"
        );
        assert!(i1.content.contains("Sub content."));
        assert!(!i1.content.contains("Next"));
        assert!(!i1.content.contains("Outside."));
    }

    #[test]
    fn shallower_heading_still_terminates() {
        // A shallower heading (#) terminates a block that appeared after ###
        let input =
            "### Deep\n\n[D1. Decision]\nDetailed content.\n\n# Top Level\n\nTop content.\n";
        let d1 = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(d1.content.contains("Detailed content."));
        assert!(
            !d1.content.contains("Top Level"),
            "# heading should terminate block"
        );
    }

    #[test]
    fn label_on_heading_line_is_a_block_not_a_terminator() {
        // A heading line that ALSO contains an I/D label should be treated as a block start,
        // not as a terminator for the previous block. But try_parse_label checks for `[` prefix
        // after trimming, and a heading starts with `#`, so `### [I2. Title]` is parsed as
        // a heading (not a label). This test documents expected behavior.
        let input = "[I1. First]\nContent A.\n### [I2. Title On Heading]\nContent B.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        // `### [I2. Title On Heading]` starts with `#` — is_heading_line returns true.
        // try_parse_label trims whitespace then checks for `[` — but `### [I2...` has `#` first.
        // So this is a heading, NOT a label.
        let ids: Vec<&str> = result
            .blocks
            .iter()
            .map(|b| b.identifier.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["I1"],
            "Heading-prefixed label should be treated as heading, not I/D label"
        );

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            !i1.content.contains("Content B."),
            "Heading should terminate I1"
        );
    }

    #[test]
    fn consecutive_headings_then_label() {
        // Multiple headings in a row, then a label — block starts at the label with no interference
        let input = "# H1\n## H2\n### H3\n[I1. After headings]\nContent.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
    }

    #[test]
    fn label_heading_label_sandwich() {
        // I1 → heading → I2: both blocks have correct content
        let input = "[I1. Before]\nContent A.\n## Middle\nOrphan text.\n[I2. After]\nContent B.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        let i2 = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();

        assert!(i1.content.contains("Content A."));
        assert!(!i1.content.contains("Middle"));
        assert!(!i1.content.contains("Orphan text."));

        assert!(i2.content.contains("Content B."));
        assert!(
            !i2.content.contains("Orphan text."),
            "I2 starts after its label, not at the heading"
        );
    }

    #[test]
    fn heading_immediately_after_label_line_no_content() {
        // No blank line between label and heading — content is still empty
        let input = "[D1. Instant heading]\n# Big Heading\nStuff.\n";
        let d1 = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(
            d1.content.trim().is_empty(),
            "Block terminated by immediate heading should be empty, got: {:?}",
            d1.content
        );
    }

    #[test]
    fn seven_hashes_is_not_a_heading() {
        // 7+ `#` chars are NOT valid ATX headings (max is 6)
        let input = "[I1. Block]\nContent.\n####### Not a heading\nStill in block.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            i1.content.contains("Not a heading"),
            "7 hashes is not a heading, should not terminate block"
        );
        assert!(i1.content.contains("Still in block."));
    }

    #[test]
    fn hash_without_space_is_not_heading() {
        // `##word` (no space) is not a heading
        let input = "[I1. Block]\nContent.\n##NotAHeading\nStill in block.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            i1.content.contains("NotAHeading"),
            "## without space is not a heading, should not terminate block"
        );
        assert!(i1.content.contains("Still in block."));
    }

    #[test]
    fn empty_heading_terminates_block() {
        // A heading with no text after `# ` is still a valid heading
        let input = "[I1. Block]\nContent.\n## \nAfter empty heading.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            !i1.content.contains("After empty heading."),
            "Empty heading should still terminate block"
        );
    }

    #[test]
    fn bare_hash_heading_terminates_block() {
        // `#` alone on a line (ATX heading level 1, empty content) is a valid heading
        let input = "[I1. Block]\nContent.\n#\nAfter bare hash.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            !i1.content.contains("After bare hash."),
            "Bare # heading should terminate block"
        );
    }

    #[test]
    fn heading_in_html_comment_does_not_terminate() {
        let input =
            "[I1. Block]\nContent A.\n<!-- ## Heading in comment -->\nContent B still in I1.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            i1.content.contains("Content B still in I1."),
            "Heading inside HTML comment should not terminate block"
        );
    }

    #[test]
    fn heading_in_tilde_code_fence_does_not_terminate() {
        let input = "[I1. Block]\nContent A.\n~~~\n## Heading in tilde fence\n~~~\nContent B still in I1.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content A."));
        assert!(
            i1.content.contains("Content B still in I1."),
            "Heading inside tilde code fence should not terminate block"
        );
    }

    #[test]
    fn content_between_heading_and_next_label_is_orphaned() {
        // Verify that content between a terminating heading and the next label
        // is not attributed to any block
        let input = "[I1. A]\nContent A.\n## Divider\nOrphan content.\n[I2. B]\nContent B.\n";
        let result = MarkdownAnchorExtractor::extract(input);

        let i1_text = &input[result.blocks[0].byte_range.clone()];
        let i2_text = &input[result.blocks[1].byte_range.clone()];

        assert!(
            !i1_text.contains("Orphan content."),
            "Orphan text should not be in I1"
        );
        assert!(
            !i2_text.contains("Orphan content."),
            "Orphan text should not be in I2"
        );
    }

    #[test]
    fn three_way_race_label_heading_eof_label_wins() {
        // All three terminators present: next label is closest → label wins
        let input = "[I1. First]\nA.\n[I2. Second]\nB.\n## H\nC.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert_eq!(i1.content.trim(), "A.");
    }

    #[test]
    fn three_way_race_heading_wins() {
        // All three terminators present: heading is closest → heading wins
        let input = "[I1. First]\nA.\n## H\nC.\n[I2. Second]\nB.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert_eq!(i1.content.trim(), "A.");
    }

    #[test]
    fn three_way_race_eof_wins() {
        // Only terminator is EOF (no heading, no next label)
        let input = "[I1. Only]\nA.\nB.\nC.";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("A."));
        assert!(i1.content.contains("B."));
        assert!(i1.content.contains("C."));
    }

    #[test]
    fn heading_with_label_like_text_is_heading_not_label() {
        // `## [I2. Looks like a label]` — parsed as heading because line starts with `#`
        let input = "[I1. A]\nContent.\n## [I2. Looks like a label]\nMore.\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(
            result.blocks.len(),
            1,
            "## [I2...] should be a heading, not a label"
        );
        assert_eq!(result.blocks[0].identifier, "I1");

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(!i1.content.contains("More."));
    }

    #[test]
    fn block_headings_respect_their_enclosing_section_level() {
        // With no enclosing section, I1 stops at the next heading. I2 is under
        // ## H1, so its nested ### subsection remains owned until I3.
        let input = "\
[I1. A]\nContent A.\n## H1\nOrphan 1.\n\
[I2. B]\nContent B.\n### H2\nOrphan 2.\n\
[I3. C]\nContent C.\n# H3\nOrphan 3.\n";

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        let i2 = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();
        let i3 = MarkdownAnchorExtractor::extract_block(input, "I3").unwrap();

        assert_eq!(i1.content.trim(), "Content A.");
        assert_eq!(i2.content.trim(), "Content B.\n### H2\nOrphan 2.");
        assert_eq!(i3.content.trim(), "Content C.");
    }

    #[test]
    fn block_with_blank_lines_before_heading() {
        // Blank lines are part of the block content until the heading is reached
        let input = "[I1. Block]\nContent.\n\n\n\n## Heading\nAfter.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(!i1.content.contains("Heading"));
        assert!(!i1.content.contains("After."));
    }

    #[test]
    fn indented_heading_still_terminates() {
        // Leading whitespace before `#` — is_heading_line trims leading whitespace
        let input = "[I1. Block]\nContent.\n  ## Indented Heading\nAfter.\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            !i1.content.contains("After."),
            "Indented heading should still terminate block"
        );
    }

    #[test]
    fn crlf_heading_terminates_block() {
        let input = "[I1. Block]\r\nContent.\r\n## Heading\r\nAfter.\r\n";
        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("Content."));
        assert!(
            !i1.content.contains("After."),
            "Heading with CRLF should terminate block"
        );
    }

    #[test]
    fn byte_ranges_no_overlap_with_headings() {
        // Verify that block byte ranges with interleaved headings never overlap
        let input = "[I1. A]\nfoo\n## H1\nbar\n[I2. B]\nbaz\n### H2\nqux\n[I3. C]\nquux\n";
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 3);

        for i in 0..result.blocks.len() {
            for j in 0..result.blocks.len() {
                if i != j {
                    let a = &result.blocks[i].byte_range;
                    let b = &result.blocks[j].byte_range;
                    assert!(
                        a.end <= b.start || b.end <= a.start,
                        "Blocks {} and {} overlap: {:?} vs {:?}",
                        result.blocks[i].identifier,
                        result.blocks[j].identifier,
                        a,
                        b
                    );
                }
            }
        }
    }

    #[test]
    fn real_world_ontology_heading_terminates_block() {
        // Realistic ontology file structure: section heading after block should terminate it
        let input = r#"---
component_id: g5/g5-api/task
---

# Task Service

## Boundary

[I1. Task lifecycle management]
The task service manages the full lifecycle of tasks: creation, assignment,
execution tracking, and completion. All task mutations are scoped to a
workspace branch.

[I2. Task state machine]
Tasks follow a strict state machine: Pending → InProgress → Complete|Failed.
Invalid transitions are rejected at the service layer.

## Behavior

[D1. Optimistic locking for task updates]
Task updates use an `updated_at` timestamp for optimistic concurrency control.
If the timestamp doesn't match, the update is rejected with a conflict error.

**Why:** Prevents lost updates when multiple agents work on related tasks.

## Open Questions

[Q1. Task priority scheduling]
Should we implement priority-based task scheduling or FIFO?

## Implementation Notes

These notes are not part of any intent block and should be orphaned.
"#;
        let result = MarkdownAnchorExtractor::extract(input);
        assert_eq!(result.blocks.len(), 4);

        let i1 = MarkdownAnchorExtractor::extract_block(input, "I1").unwrap();
        assert!(i1.content.contains("full lifecycle"));
        assert!(
            !i1.content.contains("state machine"),
            "I2 label should terminate I1"
        );

        let i2 = MarkdownAnchorExtractor::extract_block(input, "I2").unwrap();
        assert!(i2.content.contains("strict state machine"));
        assert!(
            !i2.content.contains("Behavior"),
            "## Behavior heading should terminate I2"
        );
        assert!(!i2.content.contains("Optimistic locking"));

        let d1 = MarkdownAnchorExtractor::extract_block(input, "D1").unwrap();
        assert!(d1.content.contains("optimistic concurrency"));
        assert!(d1.content.contains("Prevents lost updates"));
        assert!(
            !d1.content.contains("Open Questions"),
            "## heading should terminate D1"
        );

        let q1 = MarkdownAnchorExtractor::extract_block(input, "Q1").unwrap();
        assert!(q1.content.contains("priority-based"));
        assert!(
            !q1.content.contains("Implementation Notes"),
            "## heading should terminate Q1"
        );
        assert!(!q1.content.contains("not part of any intent block"));
    }
}
