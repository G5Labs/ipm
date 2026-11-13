//! Unified diff text split into hunks with line numbers. Git produces the
//! text; the task and change-graph views present it.

use g5_api_foundation::contracts::tasks::{DiffHunk, DiffLine};

/// Parse `diff_text` into hunks, returning them with the addition and
/// deletion counts. File headers are skipped; lines outside a hunk are
/// ignored.
pub fn parse_unified_diff(diff_text: &str) -> (Vec<DiffHunk>, i64, i64) {
    let mut hunks = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    let mut current: Option<DiffHunk> = None;
    let mut old_line = 0;
    let mut new_line = 0;
    for line in diff_text.lines() {
        if let Some(rest) = line.strip_prefix("@@ ") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let parts: Vec<_> = rest.split(' ').collect();
            old_line = parts
                .first()
                .and_then(|part| part.strip_prefix('-'))
                .and_then(|part| part.split(',').next())
                .and_then(|value| value.parse().ok())
                .unwrap_or(1);
            new_line = parts
                .get(1)
                .and_then(|part| part.strip_prefix('+'))
                .and_then(|part| part.split(',').next())
                .and_then(|value| value.parse().ok())
                .unwrap_or(1);
            current = Some(DiffHunk {
                start_line: old_line,
                header: format!("@@ {rest}"),
                lines: Vec::new(),
            });
        } else if line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("new file mode ")
            || line.starts_with("deleted file mode ")
        {
            continue;
        } else if let Some(hunk) = current.as_mut() {
            let (line_type, old, new) = if line.starts_with('+') {
                additions += 1;
                let current_new = new_line;
                new_line += 1;
                ("addition", None, Some(current_new))
            } else if line.starts_with('-') {
                deletions += 1;
                let current_old = old_line;
                old_line += 1;
                ("deletion", Some(current_old), None)
            } else {
                let current_old = old_line;
                let current_new = new_line;
                old_line += 1;
                new_line += 1;
                ("context", Some(current_old), Some(current_new))
            };
            hunk.lines.push(DiffLine {
                line_type: line_type.to_string(),
                old_line: old,
                new_line: new,
                content: line.to_string(),
            });
        }
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    (hunks, additions, deletions)
}
