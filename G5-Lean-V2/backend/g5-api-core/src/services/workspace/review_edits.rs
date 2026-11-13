//! Source-preserving edit units owned by workspace review records.
//!
//! Offsets refer to immutable captured UTF-8 source, never a rendered token index.
//! Accepted replacements are composed separately from immutable proposal text.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSuggestionEdit {
    pub id: Uuid,
    pub start: usize,
    pub end: usize,
    pub before_text: String,
    pub after_text: String,
    pub accepted_after_text: Option<String>,
    pub status: String,
    pub status_by: Option<Uuid>,
    pub status_at: Option<DateTime<Utc>>,
    pub comment_id: Option<Uuid>,
    #[serde(default)]
    pub checkpoint_pending: bool,
    #[serde(default)]
    pub written_hash: Option<String>,
    #[serde(default)]
    pub current_start: usize,
    #[serde(default)]
    pub current_end: usize,
    #[serde(default)]
    pub source_conflict: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expected_before_lines: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expected_after_lines: String,
}

#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn tokens(source: &str) -> Vec<Token<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut kind = None;
    for (offset, ch) in source.char_indices() {
        let next_kind = if ch.is_alphanumeric() || ch == '_' {
            0
        } else if ch.is_whitespace() {
            1
        } else {
            2
        };
        if kind.is_some_and(|previous| previous != next_kind || next_kind == 2) {
            result.push(Token {
                text: &source[start..offset],
                start,
            });
            start = offset;
        }
        kind = Some(next_kind);
    }
    if start < source.len() {
        result.push(Token {
            text: &source[start..],
            start,
        });
    }
    result
}

/// Split an exact shortest edit path with linear frontier storage. Repeated
/// unchanged words remain correspondence even when there is no unique anchor.
fn bisect(left: &[Token<'_>], right: &[Token<'_>]) -> Option<(usize, usize)> {
    let n = left.len() as isize;
    let m = right.len() as isize;
    let max_d = (n + m + 1) / 2;
    let offset = max_d + 1;
    let mut forward = vec![-1isize; (2 * max_d + 3) as usize];
    let mut reverse = forward.clone();
    forward[(offset + 1) as usize] = 0;
    reverse[(offset + 1) as usize] = 0;
    let delta = n - m;
    let front = delta % 2 != 0;
    let (mut f_start, mut f_end, mut r_start, mut r_end) = (0, 0, 0, 0);
    for d in 0..max_d {
        for k in ((-d + f_start)..=(d - f_end)).step_by(2) {
            let at = (offset + k) as usize;
            let mut x = if k == -d || (k != d && forward[at - 1] < forward[at + 1]) {
                forward[at + 1]
            } else {
                forward[at - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && left[x as usize].text == right[y as usize].text {
                x += 1;
                y += 1;
            }
            forward[at] = x;
            if x > n {
                f_end += 2;
            } else if y > m {
                f_start += 2;
            } else if front {
                let opposite = offset + delta - k;
                if opposite >= 0
                    && (opposite as usize) < reverse.len()
                    && reverse[opposite as usize] >= 0
                    && x >= n - reverse[opposite as usize]
                {
                    return Some((x as usize, y as usize));
                }
            }
        }
        for k in ((-d + r_start)..=(d - r_end)).step_by(2) {
            let at = (offset + k) as usize;
            let mut x = if k == -d || (k != d && reverse[at - 1] < reverse[at + 1]) {
                reverse[at + 1]
            } else {
                reverse[at - 1] + 1
            };
            let mut y = x - k;
            while x < n
                && y < m
                && left[(n - x - 1) as usize].text == right[(m - y - 1) as usize].text
            {
                x += 1;
                y += 1;
            }
            reverse[at] = x;
            if x > n {
                r_end += 2;
            } else if y > m {
                r_start += 2;
            } else if !front {
                let opposite = offset + delta - k;
                if opposite >= 0
                    && (opposite as usize) < forward.len()
                    && forward[opposite as usize] >= 0
                {
                    let first_x = forward[opposite as usize];
                    if first_x >= n - x {
                        return Some((first_x as usize, (first_x - (delta - k)) as usize));
                    }
                }
            }
        }
    }
    None // No unchanged token lies on an edit path.
}

/// Exact unchanged token pairs. Large regions use monotonic anchors or a
/// linear-memory shortest-edit-path split instead of an unbounded LCS table.
fn matches(before: &[Token<'_>], after: &[Token<'_>]) -> Vec<(usize, usize)> {
    sequence_matches(before, after, true)
}

fn sequence_matches(
    before: &[Token<'_>],
    after: &[Token<'_>],
    weak_punctuation_suffix: bool,
) -> Vec<(usize, usize)> {
    let mut prefix = 0;
    while prefix < before.len().min(after.len()) && before[prefix].text == after[prefix].text {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < (before.len() - prefix).min(after.len() - prefix)
        && before[before.len() - 1 - suffix].text == after[after.len() - 1 - suffix].text
    {
        suffix += 1;
    }
    // A punctuation-only terminator is weak correspondence: a new paragraph
    // can end the same way. Pure trailing whitespace instead stays canonical;
    // borrowing a newline inside a newly added fence would split that block.
    if weak_punctuation_suffix
        && suffix > 0
        && !before[before.len() - suffix..]
            .iter()
            .any(|token| token.text.chars().any(char::is_alphanumeric))
        && before[before.len() - suffix..]
            .iter()
            .any(|token| token.text.chars().any(|ch| !ch.is_whitespace()))
    {
        suffix = 0;
    }
    let left = &before[prefix..before.len() - suffix];
    let right = &after[prefix..after.len() - suffix];
    let mut result: Vec<_> = (0..prefix).map(|i| (i, i)).collect();
    if (left.len() + 1).saturating_mul(right.len() + 1) <= 1_000_000 {
        let width = right.len() + 1;
        let mut lcs = vec![0u32; (left.len() + 1) * width];
        for i in (0..left.len()).rev() {
            for j in (0..right.len()).rev() {
                lcs[i * width + j] = if left[i].text == right[j].text {
                    1 + lcs[(i + 1) * width + j + 1]
                } else {
                    lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < left.len() && j < right.len() {
            if left[i].text == right[j].text {
                result.push((prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if lcs[(i + 1) * width + j] >= lcs[i * width + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    } else {
        let mut occurrences: HashMap<&str, (Vec<usize>, Vec<usize>)> = HashMap::new();
        for (i, token) in left.iter().enumerate() {
            occurrences.entry(token.text).or_default().0.push(i);
        }
        for (j, token) in right.iter().enumerate() {
            occurrences.entry(token.text).or_default().1.push(j);
        }
        let mut anchors: Vec<_> = occurrences
            .values()
            .filter(|(a, b)| a.len() == 1 && b.len() == 1)
            .map(|(a, b)| (a[0], b[0]))
            .collect();
        anchors.sort_unstable();
        // Longest increasing subsequence of right offsets, deterministic after
        // sorting by left offsets. Each anchor reduces both recursive regions.
        let mut tails: Vec<usize> = Vec::new();
        let mut predecessors = vec![None; anchors.len()];
        for (index, &(_, right_index)) in anchors.iter().enumerate() {
            let slot = tails.partition_point(|&tail| anchors[tail].1 < right_index);
            if slot > 0 {
                predecessors[index] = Some(tails[slot - 1]);
            }
            if slot == tails.len() {
                tails.push(index);
            } else {
                tails[slot] = index;
            }
        }
        let mut selected = Vec::new();
        let mut current = tails.last().copied();
        while let Some(index) = current {
            selected.push(anchors[index]);
            current = predecessors[index];
        }
        selected.reverse();
        if selected.is_empty() {
            if let Some((i, j)) = bisect(left, right)
                .filter(|&(i, j)| (i != 0 || j != 0) && (i != left.len() || j != right.len()))
            {
                result.extend(
                    sequence_matches(&left[..i], &right[..j], weak_punctuation_suffix)
                        .into_iter()
                        .map(|(a, b)| (prefix + a, prefix + b)),
                );
                result.extend(
                    sequence_matches(&left[i..], &right[j..], weak_punctuation_suffix)
                        .into_iter()
                        .map(|(a, b)| (prefix + i + a, prefix + j + b)),
                );
            }
        }
        let (mut left_start, mut right_start) = (0, 0);
        for (i, j) in selected {
            result.extend(
                sequence_matches(
                    &left[left_start..i],
                    &right[right_start..j],
                    weak_punctuation_suffix,
                )
                .into_iter()
                .map(|(a, b)| (prefix + left_start + a, prefix + right_start + b)),
            );
            result.push((prefix + i, prefix + j));
            left_start = i + 1;
            right_start = j + 1;
        }
        if left_start != 0 || right_start != 0 {
            result.extend(
                sequence_matches(
                    &left[left_start..],
                    &right[right_start..],
                    weak_punctuation_suffix,
                )
                .into_iter()
                .map(|(a, b)| (prefix + left_start + a, prefix + right_start + b)),
            );
        }
    }
    result.extend((0..suffix).map(|i| (before.len() - suffix + i, after.len() - suffix + i)));
    result
}

/// Complete top-level source blocks take precedence over word correspondence.
/// Strip only line terminators: trailing spaces can encode Markdown meaning.
fn source_blocks(source: &str) -> Vec<Token<'_>> {
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    for (event, range) in
        pulldown_cmark::Parser::new_ext(source, pulldown_cmark::Options::all()).into_offset_iter()
    {
        match event {
            pulldown_cmark::Event::Start(_) | pulldown_cmark::Event::Rule => {
                if depth == 0 {
                    let text = source[range.clone()].trim_end_matches(['\r', '\n']);
                    if !text.is_empty() {
                        blocks.push(Token {
                            text,
                            start: range.start,
                        });
                    }
                }
                if matches!(event, pulldown_cmark::Event::Start(_)) {
                    depth += 1;
                }
            }
            pulldown_cmark::Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    blocks
}

type UnchangedSpan = (usize, usize, usize, usize);

fn word_matches(before: &str, after: &str) -> Vec<UnchangedSpan> {
    let before_tokens = tokens(before);
    let after_tokens = tokens(after);
    matches(&before_tokens, &after_tokens)
        .into_iter()
        .map(|(a, b)| {
            let a = before_tokens[a];
            let b = after_tokens[b];
            (
                a.start,
                a.start + a.text.len(),
                b.start,
                b.start + b.text.len(),
            )
        })
        .collect()
}

/// A block gap owns its outer separators. Matching an interior fence/list
/// newline against canonical whitespace would split the structural insertion.
fn boundary_matches(before: &str, after: &str, next_block: bool) -> Vec<UnchangedSpan> {
    if !before.chars().all(char::is_whitespace) && !after.chars().all(char::is_whitespace) {
        return word_matches(before, after);
    }
    let prefix_len = |left: &str, right: &str| {
        left.chars()
            .zip(right.chars())
            .take_while(|(a, b)| a == b && a.is_whitespace())
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>()
    };
    let suffix_len = |left: &str, right: &str| {
        left.chars()
            .rev()
            .zip(right.chars().rev())
            .take_while(|(a, b)| a == b && a.is_whitespace())
            .map(|(a, _)| a.len_utf8())
            .sum::<usize>()
    };
    let (prefix, suffix) = if next_block {
        let prefix = prefix_len(before, after);
        (prefix, suffix_len(&before[prefix..], &after[prefix..]))
    } else {
        let suffix = suffix_len(before, after);
        (
            prefix_len(
                &before[..before.len() - suffix],
                &after[..after.len() - suffix],
            ),
            suffix,
        )
    };
    let mut result = Vec::new();
    if prefix > 0 {
        result.push((0, prefix, 0, prefix));
    }
    if suffix > 0 {
        result.push((
            before.len() - suffix,
            before.len(),
            after.len() - suffix,
            after.len(),
        ));
    }
    result
}

fn source_matches(before: &str, after: &str) -> Vec<UnchangedSpan> {
    let before_blocks = source_blocks(before);
    let after_blocks = source_blocks(after);
    // Definitions and other parser-invisible source still use exact word
    // correspondence; no bytes are dropped from the captured source frame.
    if before_blocks.is_empty() || after_blocks.is_empty() {
        return word_matches(before, after);
    }
    let anchors = sequence_matches(&before_blocks, &after_blocks, false);
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (a, b) in anchors
        .into_iter()
        .chain(std::iter::once((before_blocks.len(), after_blocks.len())))
    {
        // Within a rewritten run, pair existing blocks in source order first.
        // Excess proposed blocks remain in the following insertion gap.
        pairs.extend((0..(a - i).min(b - j)).map(|offset| (i + offset, j + offset)));
        if a < before_blocks.len() {
            pairs.push((a, b));
        }
        i = a + 1;
        j = b + 1;
    }
    let mut result = Vec::new();
    let (mut before_cursor, mut after_cursor) = (0, 0);
    for (a, b) in pairs {
        let a = before_blocks[a];
        let b = after_blocks[b];
        result.extend(
            boundary_matches(
                &before[before_cursor..a.start],
                &after[after_cursor..b.start],
                true,
            )
            .into_iter()
            .map(|(x, y, u, v)| {
                (
                    before_cursor + x,
                    before_cursor + y,
                    after_cursor + u,
                    after_cursor + v,
                )
            }),
        );
        // Zero-width correspondence marks a decision boundary even when a
        // complete rewrite has no unchanged word next to the added block.
        result.push((a.start, a.start, b.start, b.start));
        if a.text == b.text {
            result.push((
                a.start,
                a.start + a.text.len(),
                b.start,
                b.start + b.text.len(),
            ));
        } else {
            result.extend(
                word_matches(a.text, b.text)
                    .into_iter()
                    .map(|(x, y, u, v)| (a.start + x, a.start + y, b.start + u, b.start + v)),
            );
        }
        before_cursor = a.start + a.text.len();
        after_cursor = b.start + b.text.len();
        result.push((before_cursor, before_cursor, after_cursor, after_cursor));
    }
    result.extend(
        boundary_matches(&before[before_cursor..], &after[after_cursor..], false)
            .into_iter()
            .map(|(x, y, u, v)| {
                (
                    before_cursor + x,
                    before_cursor + y,
                    after_cursor + u,
                    after_cursor + v,
                )
            }),
    );
    result
}

/// A replacement's deleted and inserted text forms one unit. Whitespace alone
/// cannot separate adjacent replacement tokens; unchanged words can. Whole
/// retained blocks and changed block pairs are established before word matching.
pub fn discrete_edits(parent_id: Uuid, before: &str, after: &str) -> Vec<ReviewSuggestionEdit> {
    let unchanged = source_matches(before, after);
    let mut ranges: Vec<(usize, usize, usize, usize)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut may_merge = true;
    for (start, end, after_start, after_end) in unchanged.into_iter().chain(std::iter::once((
        before.len(),
        before.len(),
        after.len(),
        after.len(),
    ))) {
        if i != start || j != after_start {
            let next = (i, start, j, after_start);
            if let Some(previous) = ranges.last_mut().filter(|previous| {
                may_merge
                    && before[previous.1..next.0].chars().all(char::is_whitespace)
                    && after[previous.3..next.2].chars().all(char::is_whitespace)
                    && !before[previous.1..next.0].contains('\n')
                    && !after[previous.3..next.2].contains('\n')
            }) {
                previous.1 = next.1;
                previous.3 = next.3;
            } else {
                ranges.push(next);
            }
        }
        i = end;
        j = after_end;
        may_merge = start != end || after_start != after_end;
    }
    let mut edits = Vec::new();
    let mut capture_ordinals = HashMap::<(usize, usize), usize>::new();
    for (mut start, mut end, mut after_start, mut after_end) in ranges {
        // LCS can assign an unchanged separator to an adjacent replacement.
        // Keep shared boundary whitespace canonical, outside the decision.
        while let (Some(a), Some(b)) = (
            before[start..end].chars().next_back(),
            after[after_start..after_end].chars().next_back(),
        ) {
            if a != b || !a.is_whitespace() {
                break;
            }
            end -= a.len_utf8();
            after_end -= b.len_utf8();
        }
        while let (Some(a), Some(b)) = (
            before[start..end].chars().next(),
            after[after_start..after_end].chars().next(),
        ) {
            if a != b || !a.is_whitespace() {
                break;
            }
            start += a.len_utf8();
            after_start += b.len_utf8();
        }
        let proposed = &after[after_start..after_end];
        // Multiple added blocks share a captured insertion point. Before a
        // retained block, each child owns its closing separator; at EOF it owns
        // its opening separator. Either child must stand alone, and their sum
        // still reproduces the exact proposed source.
        let parts = if start == end {
            let before_retained_block = (start == 0 || before[..start].ends_with('\n'))
                && before[end..].chars().any(|ch| !ch.is_whitespace())
                && proposed.ends_with('\n');
            paragraph_insertions(proposed, before_retained_block)
        } else {
            vec![proposed]
        };
        for part in parts {
            // Separate source ranges can end at the same captured point. One
            // source-order ordinal preserves distinct identical fragments.
            let part_index = capture_ordinals.entry((start, end)).or_default();
            let captured = &before[start..end];
            let identity = format!(
                "{start}:{end}:{part_index}:{}:{captured}:{part}",
                captured.len()
            );
            *part_index += 1;
            edits.push(ReviewSuggestionEdit {
                id: Uuid::new_v5(&parent_id, identity.as_bytes()),
                start,
                end,
                before_text: captured.to_owned(),
                after_text: part.to_owned(),
                accepted_after_text: None,
                status: "open".to_owned(),
                status_by: None,
                status_at: None,
                comment_id: None,
                checkpoint_pending: false,
                written_hash: None,
                current_start: start,
                current_end: end,
                source_conflict: false,
                expected_before_lines: String::new(),
                expected_after_lines: String::new(),
            });
        }
    }
    // Constructed ranges are source boundaries, so this cannot fail. Consumers
    // still validate deserialized ledger records before any canonical mutation.
    let _ = refresh_edit_projection(before, &mut edits);
    edits
}

fn paragraph_insertions(source: &str, trailing_separators: bool) -> Vec<&str> {
    let mut boundaries = vec![0];
    let mut depth = 0usize;
    let mut previous_block = false;
    for (event, range) in
        pulldown_cmark::Parser::new_ext(source, pulldown_cmark::Options::all()).into_offset_iter()
    {
        match event {
            pulldown_cmark::Event::Start(_) | pulldown_cmark::Event::Rule => {
                if depth == 0 {
                    if previous_block {
                        let mut boundary = range.start;
                        if !trailing_separators {
                            while let Some((offset, ch)) =
                                source[..boundary].char_indices().next_back()
                            {
                                if !ch.is_whitespace() {
                                    break;
                                }
                                boundary = offset;
                            }
                        }
                        if boundary > *boundaries.last().unwrap() {
                            boundaries.push(boundary);
                        }
                    }
                    previous_block = true;
                }
                if matches!(event, pulldown_cmark::Event::Start(_)) {
                    depth += 1;
                }
            }
            pulldown_cmark::Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    boundaries.push(source.len());
    boundaries
        .windows(2)
        .filter(|pair| pair[0] != pair[1])
        .map(|pair| &source[pair[0]..pair[1]])
        .collect()
}

fn validate(before: &str, edits: &[ReviewSuggestionEdit]) -> Result<(), String> {
    let mut cursor = 0;
    let mut ids = HashSet::new();
    for edit in edits {
        if !ids.insert(edit.id)
            || edit.start < cursor
            || edit.start > edit.end
            || before.get(edit.start..edit.end) != Some(edit.before_text.as_str())
        {
            return Err("Invalid captured review edit ledger".to_owned());
        }
        cursor = edit.end;
    }
    Ok(())
}

pub fn compose_current(before: &str, edits: &[ReviewSuggestionEdit]) -> Result<String, String> {
    compose(before, edits, &HashSet::new(), &HashMap::new())
}

/// Publication equivalence includes rejected proposals without reopening them.
/// Accepted owner overrides remain canonical, rather than reverting to an old
/// reviewer proposal when a reviewer updates and pushes the remaining changes.
pub fn compose_proposed(before: &str, edits: &[ReviewSuggestionEdit]) -> Result<String, String> {
    let mut proposal = edits.to_vec();
    let mut selected = HashSet::new();
    for edit in &mut proposal {
        if edit.status != "accepted" {
            edit.status = "open".to_owned();
            selected.insert(edit.id);
        }
    }
    compose(before, &proposal, &selected, &HashMap::new())
}

fn compose(
    before: &str,
    edits: &[ReviewSuggestionEdit],
    selected: &HashSet<Uuid>,
    overrides: &HashMap<Uuid, String>,
) -> Result<String, String> {
    validate(before, edits)?;
    let mut output = String::new();
    let mut cursor = 0;
    for edit in edits {
        output.push_str(&before[cursor..edit.start]);
        if edit.status == "accepted" {
            output.push_str(
                edit.accepted_after_text
                    .as_deref()
                    .unwrap_or(&edit.after_text),
            );
        } else if selected.contains(&edit.id) {
            if edit.status != "open" {
                return Err("Review edit is already decided".to_owned());
            }
            output.push_str(overrides.get(&edit.id).unwrap_or(&edit.after_text));
        } else {
            output.push_str(&edit.before_text);
        }
        cursor = edit.end;
    }
    output.push_str(&before[cursor..]);
    Ok(output)
}

pub fn compose_before_and_after(
    before: &str,
    edits: &[ReviewSuggestionEdit],
    selected: &[Uuid],
    overrides: &HashMap<Uuid, String>,
) -> Result<(String, String), String> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    if selected
        .iter()
        .any(|id| !edits.iter().any(|edit| edit.id == *id))
        || overrides.keys().any(|id| !selected.contains(id))
    {
        return Err("Unknown selected review edit".to_owned());
    }
    Ok((
        compose_current(before, edits)?,
        compose(before, edits, &selected, overrides)?,
    ))
}

pub fn refresh_edit_projection(
    before: &str,
    edits: &mut [ReviewSuggestionEdit],
) -> Result<(), String> {
    let current = compose_current(before, edits)?;
    // Each parent also exposes this common current block once. Small legacy
    // projections stay compatible; large ledgers carry raw deltas and offsets
    // without allocating/serializing a complete document for every child.
    let materialize_context =
        current.len().saturating_mul(edits.len()).saturating_mul(2) <= 4 * 1024 * 1024;
    let mut shift: i128 = 0;
    for index in 0..edits.len() {
        let edit = &edits[index];
        let start = (edit.start as i128 + shift) as usize;
        let length = if edit.status == "accepted" {
            edit.accepted_after_text
                .as_ref()
                .unwrap_or(&edit.after_text)
                .len()
        } else {
            edit.end - edit.start
        };
        let after = if !materialize_context {
            String::new()
        } else if edit.status == "open" {
            compose(before, edits, &HashSet::from([edit.id]), &HashMap::new())?
        } else {
            current.clone()
        };
        let edit = &mut edits[index];
        edit.current_start = start;
        edit.current_end = start + length;
        edit.expected_before_lines = if materialize_context {
            current.clone()
        } else {
            String::new()
        };
        edit.expected_after_lines = after;
        if edit.status == "accepted" {
            shift += length as i128 - (edit.end - edit.start) as i128;
        }
    }
    Ok(())
}

/// Insertions on the same point compete; insertion at a replacement boundary
/// can compose, whereas an insertion inside removed source overlaps it.
pub fn byte_spans_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    match (a.0 == a.1, b.0 == b.1) {
        (true, true) => a.0 == b.0,
        (true, false) => b.0 < a.0 && a.0 < b.1,
        (false, true) => a.0 < b.0 && b.0 < a.1,
        (false, false) => a.0 < b.1 && b.0 < a.1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(before: &str, after: &str) -> Vec<ReviewSuggestionEdit> {
        discrete_edits(Uuid::nil(), before, after)
    }

    #[test]
    fn screenshot_replacement_and_added_block_are_independent() {
        let before = "new line from reviewer.";
        let after = "suggest to add this line from reviewer.\n\none more thing....";
        let mut edits = proposal(before, after);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].before_text, "new");
        assert_eq!(edits[0].after_text, "suggest to add this");
        assert_eq!(edits[1].after_text, "\n\none more thing....");
        assert_eq!(edits, proposal(before, after));
        let (_, selected) =
            compose_before_and_after(before, &edits, &[edits[0].id], &HashMap::new()).unwrap();
        assert_eq!(selected, "suggest to add this line from reviewer.");
        edits[0].status = "accepted".into();
        edits[1].status = "rejected".into();
        assert_eq!(compose_current(before, &edits).unwrap(), selected);
    }

    #[test]
    fn same_line_edits_compose_in_both_orders_including_override() {
        let before = "The quick fox jumps high.";
        let after = "The slow fox jumps low.";
        for first in [0, 1] {
            let mut edits = proposal(before, after);
            assert_eq!(edits.len(), 2);
            let overrides = HashMap::from([(edits[first].id, "edited".into())]);
            let (_, written) =
                compose_before_and_after(before, &edits, &[edits[first].id], &overrides).unwrap();
            edits[first].status = "accepted".into();
            edits[first].accepted_after_text = Some("edited".into());
            refresh_edit_projection(before, &mut edits).unwrap();
            assert_eq!(compose_current(before, &edits).unwrap(), written);
            let other = 1 - first;
            let (_, completed) =
                compose_before_and_after(before, &edits, &[edits[other].id], &HashMap::new())
                    .unwrap();
            assert_eq!(
                completed,
                if first == 0 {
                    "The edited fox jumps low."
                } else {
                    "The slow fox jumps edited."
                }
            );
            assert_eq!(
                &written[edits[other].current_start..edits[other].current_end],
                edits[other].before_text
            );
        }
    }

    #[test]
    fn independent_inserted_paragraphs_compose_in_source_order() {
        let mut edits = proposal("Title", "Title\n\nFirst\n\nSecond");
        assert_eq!(edits.len(), 2);
        edits[1].status = "accepted".into();
        assert_eq!(compose_current("Title", &edits).unwrap(), "Title\n\nSecond");
        let (_, result) =
            compose_before_and_after("Title", &edits, &[edits[0].id], &HashMap::new()).unwrap();
        assert_eq!(result, "Title\n\nFirst\n\nSecond");
    }

    #[test]
    fn utf8_crlf_and_markdown_bytes_are_preserved() {
        let before = "# Résumé\r\n\r\n你好 [link](old) and café.\r\n";
        let after = "# Résumé\r\n\r\n您好 [link](new) and tea.\r\n";
        let edits = proposal(before, after);
        assert_eq!(edits.len(), 3);
        let ids: Vec<_> = edits.iter().map(|edit| edit.id).collect();
        assert_eq!(
            compose_before_and_after(before, &edits, &ids, &HashMap::new())
                .unwrap()
                .1,
            after
        );
        for edit in &edits {
            assert!(before.is_char_boundary(edit.start));
            assert!(before.is_char_boundary(edit.end));
        }
    }

    #[test]
    fn invalid_source_or_unknown_identity_refuses() {
        let edits = proposal("old content", "new content");
        assert!(compose_current("stale content", &edits).is_err());
        assert!(compose_before_and_after(
            "old content",
            &edits,
            &[Uuid::new_v4()],
            &HashMap::new()
        )
        .is_err());
        let mut corrupt = edits;
        corrupt[0].start = usize::MAX;
        assert!(compose_current("old content", &corrupt).is_err());
    }

    #[test]
    fn word_replacements_are_not_character_decisions() {
        assert_eq!(proposal("owner i started", "owner I started").len(), 1);
        assert_eq!(proposal("a red blue fox", "a green gold fox").len(), 1);
        assert!(proposal("same", "same").is_empty());
    }

    #[test]
    fn inserted_code_block_is_not_split_at_its_blank_lines() {
        let before = "Title";
        let after = "Title\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n\nSeparate paragraph.";
        let edits = proposal(before, after);
        assert_eq!(edits.len(), 2);
        assert_eq!(
            edits[0].after_text,
            "\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```"
        );
        let ids: Vec<_> = edits.iter().map(|edit| edit.id).collect();
        assert_eq!(
            compose_before_and_after(before, &edits, &ids, &HashMap::new())
                .unwrap()
                .1,
            after
        );
    }

    #[test]
    fn publication_equivalence_keeps_rejection_and_owner_override_distinct() {
        let before = "red fox runs high";
        let mut edits = proposal(before, "blue fox runs low");
        edits[0].status = "accepted".into();
        edits[0].accepted_after_text = Some("green".into());
        edits[1].status = "rejected".into();
        assert_eq!(
            compose_current(before, &edits).unwrap(),
            "green fox runs high"
        );
        assert_eq!(
            compose_proposed(before, &edits).unwrap(),
            "green fox runs low"
        );
        assert_eq!(edits[1].status, "rejected");
    }

    #[test]
    fn large_passage_keeps_separated_edits_with_bounded_alignment() {
        let before = (0..1_200)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let after = before
            .replace("word10 ", "changed10 ")
            .replace("word1190 ", "changed1190 ");
        let edits = proposal(&before, &after);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].before_text, "word10");
        assert_eq!(edits[1].before_text, "word1190");
        assert_eq!(
            compose_before_and_after(&before, &edits, &[edits[1].id], &HashMap::new())
                .unwrap()
                .1,
            before.replace("word1190 ", "changed1190 ")
        );
    }

    #[test]
    fn competition_uses_source_bytes_not_shared_line() {
        assert!(!byte_spans_overlap((4, 9), (20, 24)));
        assert!(byte_spans_overlap((4, 9), (8, 12)));
        assert!(!byte_spans_overlap((4, 9), (9, 9)));
        assert!(byte_spans_overlap((4, 9), (5, 5)));
    }

    #[test]
    fn many_independent_edits_do_not_duplicate_the_entire_parent_context() {
        let before = (0..600)
            .map(|index| format!("old{index} keep{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let after = before.replace("old", "new");
        let edits = proposal(&before, &after);
        assert_eq!(edits.len(), 600);
        assert!(edits
            .iter()
            .all(|edit| edit.expected_before_lines.is_empty()
                && edit.expected_after_lines.is_empty()));
        for edit in &edits {
            assert_eq!(
                &before[edit.current_start..edit.current_end],
                edit.before_text
            );
        }
        assert!(serde_json::to_vec(&edits).unwrap().len() < 400_000);
        assert_eq!(
            compose_before_and_after(&before, &edits, &[edits[599].id], &HashMap::new())
                .unwrap()
                .1,
            before.replace("old599", "new599")
        );
    }
}

#[cfg(test)]
mod independent_edit_regressions {
    use super::*;

    #[test]
    fn repeated_unchanged_separators_keep_independent_choices_across_alignment_limit() {
        for count in [200, 260, 600] {
            let before = std::iter::repeat_n("old keep", count)
                .collect::<Vec<_>>()
                .join(" ");
            let after = before.replace("old", "new");
            let mut edits = discrete_edits(Uuid::nil(), &before, &after);
            assert_eq!(edits.len(), count);
            assert_eq!(edits, discrete_edits(Uuid::nil(), &before, &after));
            let ids = edits.iter().map(|edit| edit.id).collect::<Vec<_>>();
            assert_eq!(
                compose_before_and_after(&before, &edits, &ids, &HashMap::new())
                    .unwrap()
                    .1,
                after
            );
            let first = edits[0].id;
            let (_, selected) =
                compose_before_and_after(&before, &edits, &[first], &HashMap::new()).unwrap();
            assert_eq!(selected, before.replacen("old", "new", 1));
            edits[0].status = "accepted".into();
            edits[1].status = "rejected".into();
            assert_eq!(compose_current(&before, &edits).unwrap(), selected);
            assert_eq!(
                edits.iter().filter(|edit| edit.status == "open").count(),
                count - 2
            );
        }
    }

    #[test]
    fn separately_added_rules_and_paragraphs_keep_separate_decisions_and_bytes() {
        for (addition, expected) in [
            ("\n---\n\nFirst paragraph\n", "Title\n\nFirst paragraph\n"),
            ("\nFirst paragraph\n\n---\n", "Title\n\n---\n"),
            ("\n---\n\n***\n", "Title\n\n***\n"),
        ] {
            let before = "Title\n";
            let after = format!("{before}{addition}");
            let mut edits = discrete_edits(Uuid::nil(), before, &after);
            assert_eq!(edits.len(), 2, "{addition:?}");
            let ids = edits.iter().map(|edit| edit.id).collect::<Vec<_>>();
            assert_ne!(ids[0], ids[1]);
            assert_eq!(
                compose_before_and_after(before, &edits, &ids, &HashMap::new())
                    .unwrap()
                    .1,
                after
            );
            edits[0].status = "rejected".into();
            edits[1].status = "accepted".into();
            assert_eq!(compose_current(before, &edits).unwrap(), expected);
        }
    }

    #[test]
    fn linear_frontier_split_preserves_exact_common_subsequence_length() {
        fn lcs(a: &[Token<'_>], b: &[Token<'_>]) -> usize {
            let mut row = vec![0; b.len() + 1];
            for token in a {
                let mut diagonal = 0;
                for (j, other) in b.iter().enumerate() {
                    let previous = row[j + 1];
                    row[j + 1] = if token.text == other.text {
                        diagonal + 1
                    } else {
                        row[j + 1].max(row[j])
                    };
                    diagonal = previous;
                }
            }
            row[b.len()]
        }
        let mut state = 13u64;
        for _ in 0..5000 {
            let mut generate = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let n = ((state >> 24) % 32) as usize;
                (0..n)
                    .map(|_| {
                        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                        Token {
                            text: ["a", "b", "c", " "][((state >> 32) % 4) as usize],
                            start: 0,
                        }
                    })
                    .collect::<Vec<_>>()
            };
            let left = generate();
            let right = generate();
            let optimum = lcs(&left, &right);
            match bisect(&left, &right) {
                Some((i, j)) => {
                    assert!(i <= left.len() && j <= right.len());
                    assert_eq!(
                        lcs(&left[..i], &right[..j]) + lcs(&left[i..], &right[j..]),
                        optimum
                    );
                }
                None => assert_eq!(optimum, 0),
            }
        }
    }
}

#[cfg(test)]
mod independent_block_boundary_regressions {
    use super::*;

    #[test]
    fn first_middle_paragraph_stays_separate_from_canonical_following_text() {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let after = "Intro.\n\nFirst added block.\n\nSecond added block.\n\nQuiet.\n\nTail.\n";
        let edits = discrete_edits(Uuid::nil(), before, after);
        assert_eq!(edits.len(), 2);
        assert_eq!(
            compose_before_and_after(before, &edits, &[edits[0].id], &HashMap::new())
                .unwrap()
                .1,
            "Intro.\n\nFirst added block.\n\nQuiet.\n\nTail.\n"
        );
    }

    #[test]
    fn canonical_final_newline_does_not_split_a_new_fenced_block() {
        let before = "Intro.\n";
        let after = "Intro.\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n\nSecond added block.\n";
        let edits = discrete_edits(Uuid::nil(), before, after);
        assert_eq!(edits.len(), 2);
        assert_eq!(
            compose_before_and_after(before, &edits, &[edits[0].id], &HashMap::new())
                .unwrap()
                .1,
            "Intro.\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n"
        );
    }

    #[test]
    fn inserted_blocks_keep_canonical_neighbors_for_every_subset_and_decision_order() {
        for newline in ["\n", "\r\n"] {
            let separator = newline.repeat(2);
            for (prefix, suffix) in [
                (String::new(), format!("Quiet.{separator}Tail.{newline}")),
                (
                    format!("Intro.{separator}"),
                    format!("Quiet.{separator}Tail.{newline}"),
                ),
                (format!("Intro.{separator}"), String::new()),
                ("Intro.".to_owned(), newline.to_owned()),
            ] {
                let before = format!("{prefix}{suffix}");
                // EOF with no existing block separator needs the leading boundary.
                let leading = if prefix.ends_with(&separator) || prefix.is_empty() {
                    "".to_owned()
                } else {
                    separator.clone()
                };
                let trailing = if suffix.trim().is_empty() {
                    "".to_owned()
                } else {
                    separator.clone()
                };
                for (first, second) in [
                    (
                        "First added block.".to_owned(),
                        "Second added block.".to_owned(),
                    ),
                    (
                        format!(
                            "```rust{newline}let a = 1;{newline}{newline}let b = 2;{newline}```"
                        ),
                        "Second added block.".to_owned(),
                    ),
                    ("---".to_owned(), "Second added block.".to_owned()),
                    ("First added block.".to_owned(), "***".to_owned()),
                ] {
                    let after =
                        format!("{prefix}{leading}{first}{separator}{second}{trailing}{suffix}");
                    let edits = discrete_edits(Uuid::nil(), &before, &after);
                    assert_eq!(edits.len(), 2, "{before:?} => {after:?}; {edits:?}");
                    assert_eq!(compose_proposed(&before, &edits).unwrap(), after);
                    for index in [0, 1] {
                        let block = [&first, &second][index];
                        let expected = format!("{prefix}{leading}{block}{trailing}{suffix}");
                        let (current, selected) = compose_before_and_after(
                            &before,
                            &edits,
                            &[edits[index].id],
                            &HashMap::new(),
                        )
                        .unwrap();
                        assert_eq!(current, before);
                        if suffix.trim().is_empty() {
                            // With an existing final blank line, separator
                            // ownership can leave additional blank lines at
                            // EOF. Preserve canonical bytes and block meaning.
                            assert!(selected.starts_with(&prefix));
                            assert!(selected.ends_with(&suffix));
                            assert_eq!(
                                pulldown_cmark::Parser::new(&selected).collect::<Vec<_>>(),
                                pulldown_cmark::Parser::new(&expected).collect::<Vec<_>>()
                            );
                        } else {
                            assert_eq!(
                                selected, expected,
                                "selected={index}, before={before:?}, after={after:?}"
                            );
                        }
                        let mut mixed = edits.clone();
                        mixed[index].status = "accepted".into();
                        mixed[1 - index].status = "rejected".into();
                        refresh_edit_projection(&before, &mut mixed).unwrap();
                        assert_eq!(compose_current(&before, &mixed).unwrap(), selected);
                        mixed[1 - index].status = "open".into();
                        let (current, both) = compose_before_and_after(
                            &before,
                            &mixed,
                            &[mixed[1 - index].id],
                            &HashMap::new(),
                        )
                        .unwrap();
                        assert_eq!(current, selected);
                        assert_eq!(both, after, "order={index}, before={before:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn edited_middle_block_keeps_its_canonical_separator() {
        let before = "Intro.\n\nQuiet.\n\nTail.\n";
        let after = "Intro.\n\nFirst added block.\n\nSecond added block.\n\nQuiet.\n\nTail.\n";
        let edits = discrete_edits(Uuid::nil(), before, after);
        assert_eq!(edits.len(), 2);
        let overrides = HashMap::from([(edits[0].id, "Edited block.\n\n".into())]);
        assert_eq!(
            compose_before_and_after(before, &edits, &[edits[0].id], &overrides)
                .unwrap()
                .1,
            "Intro.\n\nEdited block.\n\nQuiet.\n\nTail.\n"
        );
    }
}

#[cfg(test)]
mod complete_block_boundary_regressions {
    use super::*;

    #[test]
    fn retained_blocks_cannot_match_only_an_inserted_prefix_or_suffix() {
        for newline in ["\n", "\r\n"] {
            let separator = newline.repeat(2);
            for (prefix, suffix) in [
                (String::new(), format!("Quiet.{separator}Tail.{newline}")),
                (
                    format!("Intro.{separator}"),
                    format!("Quiet.{separator}Tail.{newline}"),
                ),
            ] {
                let before = format!("{prefix}{suffix}");
                for first in ["Quiet. added", "Added Quiet."] {
                    let after =
                        format!("{prefix}{first}{separator}Second added block.{separator}{suffix}");
                    let edits = discrete_edits(Uuid::nil(), &before, &after);
                    assert_eq!(edits.len(), 2, "{before:?} => {after:?}: {edits:?}");
                    assert!(edits.iter().all(|edit| edit.before_text.is_empty()));
                    assert_eq!(compose_proposed(&before, &edits).unwrap(), after);
                    for index in [0, 1] {
                        let block = [first, "Second added block."][index];
                        let expected = format!("{prefix}{block}{separator}{suffix}");
                        assert_eq!(
                            compose_before_and_after(
                                &before,
                                &edits,
                                &[edits[index].id],
                                &HashMap::new()
                            )
                            .unwrap()
                            .1,
                            expected
                        );
                    }
                }
            }
            let before = format!("Intro.{separator}Quiet.{newline}");
            let after = format!("Intro.{separator}Quiet.{separator}First added block.{separator}Added Quiet.{newline}");
            let edits = discrete_edits(Uuid::nil(), &before, &after);
            assert_eq!(edits.len(), 2, "{edits:?}");
            assert!(edits.iter().all(|edit| edit.before_text.is_empty()));
            assert_eq!(compose_proposed(&before, &edits).unwrap(), after);
        }
    }

    #[test]
    fn complete_replacement_and_excess_blocks_have_independent_choices() {
        for newline in ["\n", "\r\n"] {
            let separator = newline.repeat(2);
            for (prefix, suffix) in [
                (String::new(), newline.to_owned()),
                (String::new(), format!("{separator}Tail.{newline}")),
                (
                    format!("Intro.{separator}"),
                    format!("{separator}Tail.{newline}"),
                ),
            ] {
                let before = format!("{prefix}old{suffix}");
                for addition in [
                    "addition".to_owned(),
                    format!("```js{newline}addition{newline}{newline}more{newline}```"),
                    format!("- addition{newline}{newline}  continuation{newline}- second"),
                ] {
                    let after = format!("{prefix}replacement{separator}{addition}{suffix}");
                    let edits = discrete_edits(Uuid::nil(), &before, &after);
                    assert_eq!(edits.len(), 2, "{before:?} => {after:?}: {edits:?}");
                    assert_eq!(edits[0].before_text, "old");
                    assert_eq!(edits[0].after_text, "replacement");
                    assert!(edits[1].before_text.is_empty());
                    assert_eq!(compose_proposed(&before, &edits).unwrap(), after);
                    assert_eq!(
                        compose_before_and_after(&before, &edits, &[edits[0].id], &HashMap::new())
                            .unwrap()
                            .1,
                        format!("{prefix}replacement{suffix}")
                    );
                    for first in [0, 1] {
                        let mut settled = edits.clone();
                        settled[first].status = "accepted".into();
                        refresh_edit_projection(&before, &mut settled).unwrap();
                        assert_eq!(
                            compose_before_and_after(
                                &before,
                                &settled,
                                &[settled[1 - first].id],
                                &HashMap::new()
                            )
                            .unwrap()
                            .1,
                            after
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn separate_ranges_at_one_source_point_keep_distinct_identical_fragment_ids() {
        let before = "-\n";
        let after = "- m\n\n  continuation\n- y\n<div m\n\n  continuation\n```\n";
        let edits = discrete_edits(Uuid::nil(), before, after);
        assert_eq!(edits.len(), 6, "{edits:?}");
        assert_eq!(
            edits
                .iter()
                .map(|edit| edit.id)
                .collect::<HashSet<_>>()
                .len(),
            edits.len()
        );
        assert_eq!(edits, discrete_edits(Uuid::nil(), before, after));
        assert_eq!(compose_proposed(before, &edits).unwrap(), after);
        let repeated = edits
            .iter()
            .filter(|edit| edit.after_text == "\n\n  continuation")
            .collect::<Vec<_>>();
        assert_eq!(repeated.len(), 2);
        for first in [0, 1] {
            let mut decided = edits.clone();
            let first_id = repeated[first].id;
            let second_id = repeated[1 - first].id;
            let (_, partial) =
                compose_before_and_after(before, &decided, &[first_id], &HashMap::new()).unwrap();
            assert_eq!(partial, "-\n\n  continuation\n");
            decided
                .iter_mut()
                .find(|edit| edit.id == first_id)
                .unwrap()
                .status = "accepted".into();
            refresh_edit_projection(before, &mut decided).unwrap();
            assert_eq!(
                compose_before_and_after(before, &decided, &[second_id], &HashMap::new())
                    .unwrap()
                    .1,
                "-\n\n  continuation\n\n  continuation\n"
            );
        }
    }

    #[test]
    fn rewritten_word_and_prefix_equal_insertions_share_one_retained_block_frame() {
        let before = "old\n\nQuiet.\n";
        let after = "replacement\n\nQuiet. added\n\nSecond added block.\n\nQuiet.\n";
        let edits = discrete_edits(Uuid::nil(), before, after);
        assert_eq!(edits.len(), 3, "{edits:?}");
        assert_eq!(
            compose_before_and_after(before, &edits, &[edits[0].id], &HashMap::new())
                .unwrap()
                .1,
            "replacement\n\nQuiet.\n"
        );
        assert_eq!(
            compose_before_and_after(before, &edits, &[edits[0].id, edits[1].id], &HashMap::new())
                .unwrap()
                .1,
            "replacement\n\nQuiet. added\n\nQuiet.\n"
        );
        assert_eq!(compose_proposed(before, &edits).unwrap(), after);
    }
}
