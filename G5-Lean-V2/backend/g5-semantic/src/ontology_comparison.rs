//! Server implementation of the ontology statement rule
//! (`g5.documents.ontology.comparison-matching`).
//!
//! The editor's `frontend/src/lib/ontologyComparison.ts` implements the same
//! rule in the browser. Both must pass `tests/fixtures/ontology-comparison`
//! unchanged (`g5.documents.ontology.comparison-owner`): statements pair by
//! stable ID within one file, only authored bodies are compared, and any
//! structural diagnostic refuses the version instead of guessing.

use std::collections::{BTreeMap, HashMap, HashSet};

use g5_contracts::types::{
    current_ontology_records, parse_blocks, payload_records_user_answer, BlockSpan,
    CurrentOntologyRecord, IdBlockKind,
};

/// Combined baseline and current input the rule accepts, as in the editor.
pub const MAX_COMPARISON_BYTES: usize = 10 * 1024 * 1024;
/// Statements across both versions, as in the editor.
pub const MAX_COMPARISON_STATEMENTS: usize = 5_000;

const PROVENANCE_FLAGS: [&str; 4] = ["human-provided", "human-verified", "agent", "uncertain"];

/// Why a version or comparison is refused. `diagnostic` matches the editor's
/// first diagnostic for the same input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonRefusal {
    pub diagnostic: String,
}

impl std::fmt::Display for ComparisonRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Statement comparison unavailable: {}. Repair in Files or with an agent.",
            self.diagnostic
        )
    }
}

fn refuse(diagnostic: impl Into<String>) -> ComparisonRefusal {
    ComparisonRefusal {
        diagnostic: diagnostic.into(),
    }
}

/// Where an authored link sits in its statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinkLocation {
    Body,
    Footer,
}

/// An inline Markdown link authored in a statement body or its footer entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatementLink {
    pub href: String,
    pub text: String,
    pub location: LinkLocation,
    /// The formal `Expands [parent](#id)` / `Expands: [parent](#id)` relation.
    pub expands: bool,
}

/// One statement of one file version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyStatement {
    pub id: String,
    /// `I`, `D` or `Q`, from the marker.
    pub kind: char,
    /// The compared text: everything after the marker up to the statement end.
    pub body: String,
    /// Authored source from the first alias, anchor or marker to the statement end.
    pub source: String,
    /// Former IDs named by alias anchors above the statement's own anchor.
    pub aliases: Vec<String>,
    /// The paired footer entry's content, without its `### id` heading.
    pub metadata: String,
    /// The shared Human rule (`g5.documents.ontology.human-statements`).
    pub human: bool,
    pub links: Vec<StatementLink>,
    /// 1-based inclusive line range of `source` in its version.
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StatementChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementChange {
    pub id: String,
    pub change: StatementChangeKind,
    pub before: Option<OntologyStatement>,
    pub after: Option<OntologyStatement>,
}

/// One file compared under the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileComparison {
    /// Changed statements in current order, each deletion placed before its next
    /// surviving baseline sibling, else after its previous one, else at the end.
    pub changes: Vec<StatementChange>,
    pub before: Vec<OntologyStatement>,
    pub after: Vec<OntologyStatement>,
    /// Text outside statement bodies: headings, prose, markers of statements on
    /// both sides (so Type changes show) and their footer entries.
    pub before_residual: String,
    pub after_residual: String,
}

impl FileComparison {
    /// Metadata, Type, heading or prose changes that flag no statement.
    pub fn other_text_changed(&self) -> bool {
        self.before_residual != self.after_residual
    }
}

struct ParsedVersion {
    statements: Vec<OntologyStatement>,
    /// Byte ranges per statement ID: the whole statement span and its footer
    /// entry, and the body after the marker.
    spans: HashMap<String, StatementSpans>,
    text: String,
}

#[derive(Debug, Clone)]
struct StatementSpans {
    whole: std::ops::Range<usize>,
    body: std::ops::Range<usize>,
    footer: Option<std::ops::Range<usize>>,
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes, rest[1..].trim().trim_end_matches('#').trim_end()))
}

fn is_fence(block: &BlockSpan) -> bool {
    let raw = block.raw_content.trim_start();
    raw.starts_with("```") || raw.starts_with("~~~")
}

fn block_start(blocks: &[BlockSpan], index: usize, text: &str) -> usize {
    blocks.get(index).map_or(text.len(), |b| b.start_offset)
}

/// Statements of one version under the rule, or the first refusal diagnostic.
pub fn parse_version(text: &str) -> Result<Vec<OntologyStatement>, ComparisonRefusal> {
    parse(&normalize(text)).map(|parsed| parsed.statements)
}

fn parse(text: &str) -> Result<ParsedVersion, ComparisonRefusal> {
    let blocks = parse_blocks(text);
    let records = current_ontology_records(text);
    first_diagnostic(&blocks, &records)?;
    let mut statements = Vec::with_capacity(records.len());
    let mut spans = HashMap::with_capacity(records.len());
    // Line numbers from one index: counting from the start per statement was
    // quadratic at the 5,000-statement limit.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_of = |offset: usize| line_starts.partition_point(|&start| start <= offset);
    for record in &records {
        let marker = &blocks[record.marker];
        // A statement's source starts at its first alias anchor, so a deletion takes its aliases.
        let start = block_start(
            &blocks,
            record
                .alias_start
                .or(record.anchor)
                .unwrap_or(record.marker),
            text,
        );
        let end = block_start(&blocks, record.end.max(record.marker + 1), text);
        let body_start = marker.end_offset.min(end);
        let (metadata, footer) = match record.metadata {
            Some((heading, entry_end)) => {
                let heading_end = blocks[heading].end_offset;
                let entry_end = block_start(&blocks, entry_end, text);
                (
                    text[heading_end.min(entry_end)..entry_end].to_string(),
                    Some(blocks[heading].start_offset..entry_end),
                )
            }
            None => (String::new(), None),
        };
        let kind = match record.kind {
            IdBlockKind::Intent => 'I',
            IdBlockKind::Design => 'D',
            IdBlockKind::Question => 'Q',
        };
        let source = text[start..end].to_string();
        let body = text[body_start..end].to_string();
        let human =
            footer_is_human(&metadata) || (kind == 'Q' && payload_records_user_answer(&source));
        let mut links = extract_links(&body, LinkLocation::Body);
        links.extend(extract_links(&metadata, LinkLocation::Footer));
        spans.insert(
            record.id.clone(),
            StatementSpans {
                whole: start..end,
                body: body_start..end,
                footer,
            },
        );
        let start_line = line_of(start);
        let end_line = line_of(start + source.trim_end().len().saturating_sub(1)).max(start_line);
        statements.push(OntologyStatement {
            id: record.id.clone(),
            kind,
            body,
            source,
            aliases: record.aliases.clone(),
            metadata,
            human,
            links,
            start_line,
            end_line,
        });
    }
    Ok(ParsedVersion {
        statements,
        spans,
        text: text.to_string(),
    })
}

/// The editor's diagnostics in the editor's order: malformed markers and
/// unpaired footer entries in block order, then each statement's diagnostic and
/// nested-boundary ambiguity in statement order.
fn first_diagnostic(
    blocks: &[BlockSpan],
    records: &[CurrentOntologyRecord],
) -> Result<(), ComparisonRefusal> {
    let marker = |raw: &str| {
        let raw = raw.trim();
        raw.strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .and_then(|inner| inner.split_once(" · "))
            .is_some_and(|(kind, id)| {
                matches!(kind, "I" | "D" | "Q")
                    && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            })
    };
    let malformed = |raw: &str| {
        let raw = raw.trim();
        let mut chars = raw.chars();
        chars.next() == Some('[')
            && matches!(chars.next(), Some('I' | 'D' | 'Q'))
            && chars.as_str().trim_start().starts_with('·')
            && !marker(raw)
    };
    let counts = records
        .iter()
        .fold(HashMap::<&str, usize>::new(), |mut acc, r| {
            *acc.entry(r.id.as_str()).or_default() += 1;
            acc
        });
    let footer = blocks
        .iter()
        .position(|b| atx_heading(&b.raw_content) == Some((2, "Metadata")));
    for (index, block) in blocks.iter().enumerate() {
        if is_fence(block) {
            continue;
        }
        if atx_heading(&block.raw_content).is_none() && malformed(&block.raw_content) {
            return Err(refuse(
                "Malformed statement marker; repair in Files or with an agent.",
            ));
        }
        if footer.is_some_and(|footer| index > footer) {
            if let Some((3, id)) = atx_heading(&block.raw_content) {
                if counts.get(id).copied() != Some(1) {
                    return Err(refuse(format!("Unpaired footer metadata: {id}")));
                }
            }
        }
    }
    for record in records {
        if let Some(diagnostic) = &record.diagnostic {
            return Err(refuse(diagnostic.clone()));
        }
        if record.nested {
            return Err(refuse("Nested statement boundaries are ambiguous"));
        }
    }
    Ok(())
}

/// A footer bullet made only of provenance flags establishes provenance. Items
/// take lazy continuation lines like the editor's list parser, so a flag
/// followed by more text on the next line is prose, not provenance; fenced
/// examples are not bullets.
fn footer_is_human(metadata: &str) -> bool {
    fn bullet(line: &str) -> Option<&str> {
        let trimmed = line.trim_start();
        ["- ", "* ", "+ "]
            .iter()
            .find_map(|marker| trimmed.strip_prefix(marker))
            .map(str::trim)
    }
    let starts_block = |line: &str| {
        let trimmed = line.trim();
        trimmed.is_empty()
            || bullet(line).is_some()
            || trimmed.starts_with('#')
            || trimmed.starts_with("> ")
            || trimmed == ">"
            || matches!(trimmed, "---" | "***" | "___")
            || trimmed.starts_with(":::")
            || is_fence_line(trimmed)
            || trimmed.split_once('.').is_some_and(|(n, rest)| {
                !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ')
            })
    };
    let lines: Vec<&str> = metadata.lines().collect();
    let mut flags = HashSet::new();
    let mut fence: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str())
                && trimmed
                    .trim_start_matches(open.chars().next().unwrap())
                    .is_empty()
            {
                fence = None;
            }
            i += 1;
            continue;
        }
        if is_fence_line(trimmed) {
            let ch = trimmed.chars().next().unwrap();
            fence = Some(trimmed.chars().take_while(|c| *c == ch).collect());
            i += 1;
            continue;
        }
        let Some(first) = bullet(lines[i]) else {
            i += 1;
            continue;
        };
        let mut item = first.to_string();
        i += 1;
        while i < lines.len() && !starts_block(lines[i]) {
            item.push('\n');
            item.push_str(lines[i].trim());
            i += 1;
        }
        let tokens: Vec<&str> = item.split(['·', ';', ',']).map(str::trim).collect();
        if tokens.iter().all(|token| PROVENANCE_FLAGS.contains(token)) {
            flags.extend(tokens.into_iter().map(str::to_string));
        }
    }
    flags.contains("human-provided") || flags.contains("human-verified")
}

fn is_fence_line(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

/// Inline `[text](destination)` links outside fenced and inline code. Images are
/// not links.
pub fn extract_links(text: &str, location: LinkLocation) -> Vec<StatementLink> {
    let mut links = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let fence_char = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        if let Some(ch) = fence_char {
            let run = trimmed.chars().take_while(|c| *c == ch).count();
            if run >= 3 {
                match fence {
                    None => fence = Some((ch, run)),
                    Some((open, len)) if open == ch && run >= len => fence = None,
                    _ => {}
                }
                continue;
            }
        }
        if fence.is_some() {
            continue;
        }
        scan_line(line, location, &mut links);
    }
    links
}

fn scan_line(line: &str, location: LinkLocation, out: &mut Vec<StatementLink>) {
    let bytes = line.as_bytes();
    // Bound pathological lines: no `[` after the last `]` can open a link, and
    // a backtick run that found no closer once never will later.
    let last_close = line.rfind(']');
    let mut unclosed_runs: HashSet<usize> = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
                if unclosed_runs.contains(&run) {
                    i += run;
                    continue;
                }
                let close = "`".repeat(run);
                match line[i + run..].find(&close) {
                    Some(end) => i += run + end + run,
                    None => {
                        unclosed_runs.insert(run);
                        i += run;
                    }
                }
            }
            b'[' if last_close.is_none_or(|last| i > last) => break,
            b'[' if i == 0 || bytes[i - 1] != b'!' => {
                let Some((text_end, dest, next)) = parse_link(line, i) else {
                    i += 1;
                    continue;
                };
                let prefix = line[..i].trim().trim_start_matches(['-', '*', '+']).trim();
                out.push(StatementLink {
                    href: dest,
                    text: line[i + 1..text_end].to_string(),
                    location,
                    expands: prefix == "Expands" || prefix == "Expands:",
                });
                i = next;
            }
            _ => i += 1,
        }
    }
}

/// Parse `[text](dest "title")` starting at `open`; returns the text end, the
/// destination and the index after the closing parenthesis.
fn parse_link(line: &str, open: usize) -> Option<(usize, String, usize)> {
    let bytes = line.as_bytes();
    let mut depth = 0usize;
    let mut text_end = None;
    for (offset, b) in bytes[open..].iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    text_end = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let text_end = text_end?;
    if bytes.get(text_end + 1) != Some(&b'(') {
        return None;
    }
    let start = text_end + 2;
    let rest = &line[start..];
    if let Some(angle) = rest.strip_prefix('<') {
        let close = angle.find('>')?;
        let after = &angle[close + 1..];
        let paren = after.find(')')?;
        return Some((
            text_end,
            angle[..close].to_string(),
            start + 1 + close + 1 + paren + 1,
        ));
    }
    let mut parens = 0usize;
    for (offset, ch) in rest.char_indices() {
        match ch {
            '(' => parens += 1,
            ')' if parens == 0 => {
                let inner = rest[..offset].trim();
                let dest = inner.split_whitespace().next().unwrap_or_default();
                if dest.is_empty() {
                    return None;
                }
                return Some((text_end, dest.to_string(), start + offset + 1));
            }
            ')' => parens -= 1,
            _ => {}
        }
    }
    None
}

/// Compare one file's baseline and current versions. `None` is a confirmed
/// absent version, never an unavailable one.
pub fn compare(
    before: Option<&str>,
    after: Option<&str>,
) -> Result<FileComparison, ComparisonRefusal> {
    if before.map_or(0, str::len) + after.map_or(0, str::len) > MAX_COMPARISON_BYTES {
        return Err(refuse("combined input exceeds 10 MiB"));
    }
    let before = before.map(normalize);
    let after = after.map(normalize);
    let old = before.as_deref().map(parse).transpose()?;
    let new = after.as_deref().map(parse).transpose()?;
    let old_statements = old.as_ref().map_or(&[][..], |v| &v.statements[..]);
    let new_statements = new.as_ref().map_or(&[][..], |v| &v.statements[..]);
    if old_statements.len() + new_statements.len() > MAX_COMPARISON_STATEMENTS {
        return Err(refuse("more than 5,000 statements"));
    }
    let current_ids: HashSet<&str> = new_statements.iter().map(|s| s.id.as_str()).collect();
    let previous: HashMap<&str, &OntologyStatement> =
        old_statements.iter().map(|s| (s.id.as_str(), s)).collect();

    // Deleted statements attach to their next surviving sibling, else the
    // previous one, else the end, as in the editor.
    let mut deleted_before: BTreeMap<&str, Vec<&OntologyStatement>> = BTreeMap::new();
    let mut deleted_after: BTreeMap<&str, Vec<&OntologyStatement>> = BTreeMap::new();
    let mut deleted_at_end = Vec::new();
    let mut next_surviving = vec![None; old_statements.len()];
    let mut following = None;
    for (index, statement) in old_statements.iter().enumerate().rev() {
        next_surviving[index] = following;
        if current_ids.contains(statement.id.as_str()) {
            following = Some(statement.id.as_str());
        }
    }
    let mut preceding = None;
    for (index, statement) in old_statements.iter().enumerate() {
        if current_ids.contains(statement.id.as_str()) {
            preceding = Some(statement.id.as_str());
            continue;
        }
        match (next_surviving[index], preceding) {
            (Some(next), _) => deleted_before.entry(next).or_default().push(statement),
            (None, Some(prev)) => deleted_after.entry(prev).or_default().push(statement),
            (None, None) => deleted_at_end.push(statement),
        }
    }
    let deletion = |statement: &OntologyStatement| StatementChange {
        id: statement.id.clone(),
        change: StatementChangeKind::Deleted,
        before: Some(statement.clone()),
        after: None,
    };
    let mut changes = Vec::new();
    for statement in new_statements {
        let id = statement.id.as_str();
        changes.extend(
            deleted_before
                .get(id)
                .into_iter()
                .flatten()
                .map(|s| deletion(s)),
        );
        match previous.get(id) {
            None => changes.push(StatementChange {
                id: statement.id.clone(),
                change: StatementChangeKind::Added,
                before: None,
                after: Some(statement.clone()),
            }),
            Some(prior) if prior.body != statement.body => changes.push(StatementChange {
                id: statement.id.clone(),
                change: StatementChangeKind::Modified,
                before: Some((*prior).clone()),
                after: Some(statement.clone()),
            }),
            Some(_) => {}
        }
        changes.extend(
            deleted_after
                .get(id)
                .into_iter()
                .flatten()
                .map(|s| deletion(s)),
        );
    }
    changes.extend(deleted_at_end.into_iter().map(deletion));

    let paired: HashSet<&str> = new_statements
        .iter()
        .map(|s| s.id.as_str())
        .filter(|id| previous.contains_key(id))
        .collect();
    Ok(FileComparison {
        before_residual: old
            .as_ref()
            .map(|v| residual(v, &paired))
            .unwrap_or_default(),
        after_residual: new
            .as_ref()
            .map(|v| residual(v, &paired))
            .unwrap_or_default(),
        before: old_statements.to_vec(),
        after: new_statements.to_vec(),
        changes,
    })
}

/// Text outside compared bodies. A statement on both sides keeps its anchor,
/// marker and footer entry (so Type and metadata changes surface here); an added
/// or deleted statement is removed entirely, footer entry included.
fn residual(version: &ParsedVersion, paired: &HashSet<&str>) -> String {
    let mut removed: Vec<std::ops::Range<usize>> = Vec::new();
    for statement in &version.statements {
        let spans = &version.spans[&statement.id];
        if paired.contains(statement.id.as_str()) {
            removed.push(spans.body.clone());
        } else {
            removed.push(spans.whole.clone());
            removed.extend(spans.footer.clone());
        }
    }
    removed.sort_by_key(|range| (range.start, range.end));
    let mut out = String::new();
    let mut cursor = 0;
    for range in removed {
        if range.start >= cursor {
            out.push_str(&version.text[cursor..range.start]);
            cursor = range.end;
        } else if range.end > cursor {
            cursor = range.end;
        }
    }
    out.push_str(&version.text[cursor..]);
    let mut collapsed = String::with_capacity(out.len());
    let mut blank = false;
    for line in out.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank && blank {
            continue;
        }
        blank = is_blank;
        collapsed.push_str(line.trim_end());
        collapsed.push('\n');
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn corpus() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ontology-comparison")
    }

    fn read(dir: &Path, name: &str) -> Option<String> {
        std::fs::read_to_string(dir.join(name)).ok()
    }

    fn version_json(text: Option<&str>) -> serde_json::Value {
        match text.map(parse_version).unwrap_or(Ok(Vec::new())) {
            Ok(statements) => serde_json::json!({
                "statements": statements.iter().map(|s| {
                    let mut statement = serde_json::json!({
                        "id": s.id, "kind": s.kind.to_string(), "human": s.human,
                    });
                    // Only alias cases carry the field, so the others stay unchanged.
                    if !s.aliases.is_empty() {
                        statement["aliases"] = serde_json::json!(s.aliases);
                    }
                    statement
                }).collect::<Vec<_>>()
            }),
            Err(refusal) => serde_json::json!({"error": refusal.diagnostic}),
        }
    }

    /// One rule, two implementations: the editor runs the same corpus.
    #[test]
    fn conformance_corpus_matches_the_editor_rule() {
        let mut names: Vec<_> = std::fs::read_dir(corpus())
            .expect("conformance corpus")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert!(names.len() > 10, "corpus has cases");
        for name in names {
            let dir = corpus().join(&name);
            let expected: serde_json::Value =
                serde_json::from_str(&read(&dir, "expected.json").unwrap()).unwrap();
            let before = read(&dir, "before.g5.md");
            let after = read(&dir, "after.g5.md");
            assert_eq!(
                version_json(before.as_deref()),
                expected["before"],
                "{name}: before"
            );
            assert_eq!(
                version_json(after.as_deref()),
                expected["after"],
                "{name}: after"
            );
            match compare(before.as_deref(), after.as_deref()) {
                Ok(result) => {
                    let mut changes: Vec<_> = result
                        .changes
                        .iter()
                        .map(|c| serde_json::json!({"id": c.id, "status": format!("{:?}", c.change)}))
                        .collect();
                    changes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
                    assert!(expected.get("error").is_none(), "{name}: expected refusal");
                    assert_eq!(
                        serde_json::json!(changes),
                        expected["changes"],
                        "{name}: changes"
                    );
                }
                Err(refusal) => {
                    assert_eq!(
                        Some(refusal.diagnostic.as_str()),
                        expected["error"].as_str(),
                        "{name}: refusal"
                    );
                }
            }
        }
    }

    fn page(body: &str, footer: &[(&str, &str)]) -> String {
        let mut text = format!("# Page\n\n{body}\n\n## Metadata\n\n");
        for (id, meta) in footer {
            text.push_str(&format!("### {id}\n\n{meta}\n\n"));
        }
        text
    }

    #[test]
    fn residual_surfaces_metadata_type_and_prose_but_not_bodies_or_whole_statements() {
        let before = page(
            "Intro.\n\n<a id=\"a\"></a>\n[I · a]\n\nBody A.\n\n<a id=\"b\"></a>\n[I · b]\n\nBody B.",
            &[("a", "- agent"), ("b", "- agent")],
        );
        let body_only = before.replace("Body A.", "Body A, edited.");
        let result = compare(Some(&before), Some(&body_only)).unwrap();
        assert!(
            !result.other_text_changed(),
            "a body edit is a statement change only"
        );
        assert_eq!(result.changes.len(), 1);

        let metadata = before.replacen("- agent", "- human-provided", 1);
        let result = compare(Some(&before), Some(&metadata)).unwrap();
        assert!(result.changes.is_empty());
        assert!(
            result.other_text_changed(),
            "metadata-only changes are other text"
        );
        assert!(result.after.iter().any(|s| s.id == "a" && s.human));

        let typed = before.replace("[I · b]", "[D · b]");
        let result = compare(Some(&before), Some(&typed)).unwrap();
        assert!(result.changes.is_empty());
        assert!(result.other_text_changed(), "Type changes are other text");

        let prose = before.replace("Intro.", "Intro, edited.");
        assert!(compare(Some(&before), Some(&prose))
            .unwrap()
            .other_text_changed());

        let removed = page(
            "Intro.\n\n<a id=\"a\"></a>\n[I · a]\n\nBody A.",
            &[("a", "- agent")],
        );
        let result = compare(Some(&before), Some(&removed)).unwrap();
        assert_eq!(result.changes[0].change, StatementChangeKind::Deleted);
        assert!(
            !result.other_text_changed(),
            "a deleted statement takes its footer entry with it"
        );
    }

    #[test]
    fn links_are_extracted_with_location_and_expands() {
        let text = page(
            concat!(
                "<a id=\"p\"></a>\n[I · p]\n\nParent cites [the child](#p.formal) and [code](../../src/lib.rs).\n\n",
                "<a id=\"p.formal\"></a>\n[D · p.formal]\n\nExpands [p](#p)\n\nSee `[not a link](x)` and ![image](img.png).\n\n```md\n[fenced](nope.md)\n```"
            ),
            &[
                ("p", "- human-provided\n- [Decision](../../chat_intent/x/decisions.md#human-decisions)"),
                ("p.formal", "- agent\n- Expands: [p](#p) (formal)"),
            ],
        );
        let statements = parse_version(&text).unwrap();
        let parent = &statements[0];
        assert_eq!(
            parent.links,
            vec![
                StatementLink {
                    href: "#p.formal".into(),
                    text: "the child".into(),
                    location: LinkLocation::Body,
                    expands: false
                },
                StatementLink {
                    href: "../../src/lib.rs".into(),
                    text: "code".into(),
                    location: LinkLocation::Body,
                    expands: false
                },
                StatementLink {
                    href: "../../chat_intent/x/decisions.md#human-decisions".into(),
                    text: "Decision".into(),
                    location: LinkLocation::Footer,
                    expands: false
                },
            ]
        );
        let child = &statements[1];
        assert_eq!(
            child.links,
            vec![
                StatementLink {
                    href: "#p".into(),
                    text: "p".into(),
                    location: LinkLocation::Body,
                    expands: true
                },
                StatementLink {
                    href: "#p".into(),
                    text: "p".into(),
                    location: LinkLocation::Footer,
                    expands: true
                },
            ]
        );
    }

    #[test]
    fn link_destinations_handle_titles_angles_and_parentheses() {
        let links = extract_links(
            "[a](x.md \"Title\") [b](<with space.md>) [c](wiki_(term).md) [d]() [e] (no.md)",
            LinkLocation::Body,
        );
        let hrefs: Vec<_> = links.iter().map(|l| l.href.as_str()).collect();
        assert_eq!(hrefs, ["x.md", "with space.md", "wiki_(term).md"]);
    }

    #[test]
    fn statements_carry_their_source_line_ranges() {
        let text = page(
            "<a id=\"a\"></a>\n[I · a]\n\nLine one.\nLine two.\n\n<a id=\"b\"></a>\n[D · b]\n\nB.",
            &[("a", "- agent"), ("b", "- agent")],
        );
        let statements = parse_version(&text).unwrap();
        assert_eq!((statements[0].start_line, statements[0].end_line), (3, 7));
        assert_eq!((statements[1].start_line, statements[1].end_line), (9, 12));
    }

    #[test]
    fn limits_refuse_instead_of_truncating() {
        let big = "x".repeat(MAX_COMPARISON_BYTES / 2 + 1);
        assert_eq!(
            compare(Some(&big), Some(&big)).unwrap_err().diagnostic,
            "combined input exceeds 10 MiB"
        );
        let mut many = String::from("# Many\n\n");
        let mut footer = String::from("## Metadata\n\n");
        for n in 0..=MAX_COMPARISON_STATEMENTS / 2 {
            many.push_str(&format!("<a id=\"s{n}\"></a>\n[I · s{n}]\n\nBody.\n\n"));
            footer.push_str(&format!("### s{n}\n\n- agent\n\n"));
        }
        let text = format!("{many}{footer}");
        assert_eq!(
            compare(Some(&text), Some(&text)).unwrap_err().diagnostic,
            "more than 5,000 statements"
        );
    }

    #[test]
    fn deletions_keep_the_editor_placement() {
        let before = page(
            "<a id=\"a\"></a>\n[I · a]\n\nA.\n\n<a id=\"gone\"></a>\n[I · gone]\n\nGone.\n\n<a id=\"b\"></a>\n[I · b]\n\nB.\n\n<a id=\"tail\"></a>\n[I · tail]\n\nTail.",
            &[("a", "- agent"), ("gone", "- agent"), ("b", "- agent"), ("tail", "- agent")],
        );
        let after = page(
            "<a id=\"a\"></a>\n[I · a]\n\nA.\n\n<a id=\"b\"></a>\n[I · b]\n\nB.\n\n<a id=\"new\"></a>\n[I · new]\n\nNew.",
            &[("a", "- agent"), ("b", "- agent"), ("new", "- agent")],
        );
        let order: Vec<_> = compare(Some(&before), Some(&after))
            .unwrap()
            .changes
            .iter()
            .map(|c| (c.id.clone(), c.change))
            .collect();
        assert_eq!(
            order,
            vec![
                ("gone".into(), StatementChangeKind::Deleted),
                ("tail".into(), StatementChangeKind::Deleted),
                ("new".into(), StatementChangeKind::Added),
            ]
        );
    }
}
