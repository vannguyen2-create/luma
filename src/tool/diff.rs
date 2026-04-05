//! Unified diff generation for tool output display.
//!
//! Two entry points:
//! - `make_edit_diff` — O(n) context-based diff for edit tool (knows old/new strings + position)
//! - `make_diff` — LCS-based full-file diff for write tool
//!
//! Output format per line: `{lineno:>w} {marker} {content}`
//! where marker is `+`, `-`, or ` `. Separator lines are `...`.
//! Renderer parses this back via `parse_diff_line`.

const CONTEXT_LINES: usize = 3;

/// Parsed diff line — used by renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub lineno: u32,
    pub text: String,
}

/// Diff line type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Add,
    Del,
    Context,
    Separator,
}

/// Parse a serialized diff line back into structured data.
pub fn parse_diff_line(raw: &str) -> DiffLine {
    if raw == "..." {
        return DiffLine {
            kind: DiffKind::Separator,
            lineno: 0,
            text: String::new(),
        };
    }
    // Format: `{lineno:>w} {marker} {content}`
    // Find first non-space digit sequence, then marker after space
    let trimmed = raw.trim_start();
    let num_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let lineno: u32 = trimmed[..num_end].parse().unwrap_or(0);
    let rest = &trimmed[num_end..];

    // rest should be " + content" or " - content" or "   content"
    if rest.len() >= 3 {
        let marker = rest.as_bytes()[1];
        let content_start = 3.min(rest.len());
        let text = rest[content_start..].to_owned();
        let kind = match marker {
            b'+' => DiffKind::Add,
            b'-' => DiffKind::Del,
            _ => DiffKind::Context,
        };
        return DiffLine { kind, lineno, text };
    }

    DiffLine {
        kind: DiffKind::Context,
        lineno,
        text: rest.to_owned(),
    }
}

// ── Edit tool: context-based diff (no LCS needed) ──

/// Generate diff for edit tool — knows exact position in file.
/// `file_content` is the ORIGINAL file, `old_str`/`new_str` are the replacement pair.
/// `replace_all` controls single vs global replace.
pub fn make_edit_diff(
    file_content: &str,
    old_str: &str,
    new_str: &str,
    replace_all: bool,
) -> Vec<String> {
    let file_lines: Vec<&str> = file_content.lines().collect();
    let num_w = line_num_width(file_lines.len() + new_str.lines().count());
    let mut result = Vec::new();

    // Find all match positions (byte offsets)
    let positions: Vec<usize> = if replace_all {
        file_content
            .match_indices(old_str)
            .map(|(i, _)| i)
            .collect()
    } else {
        file_content.find(old_str).into_iter().collect()
    };

    for &byte_pos in &positions {
        // 0-based line index containing byte_pos
        let start_line = file_content[..byte_pos].matches('\n').count();

        let old_line_count = old_str.lines().count().max(1);
        let new_lines: Vec<&str> = new_str.lines().collect();

        // Context before
        let ctx_start = start_line.saturating_sub(CONTEXT_LINES);
        if ctx_start > 0
            && (result.is_empty() || result.last().is_some_and(|l: &String| l != "..."))
        {
            result.push("...".to_owned());
        }
        for i in ctx_start..start_line {
            if i < file_lines.len() {
                result.push(format!("{:>num_w$}   {}", i + 1, file_lines[i]));
            }
        }

        // Deleted lines
        for i in 0..old_line_count {
            let li = start_line + i;
            if li < file_lines.len() {
                result.push(format!("{:>num_w$} - {}", li + 1, file_lines[li]));
            }
        }

        // Added lines
        for (i, line) in new_lines.iter().enumerate() {
            result.push(format!("{:>num_w$} + {}", start_line + i + 1, line));
        }

        // Context after
        let after_start = (start_line + old_line_count).min(file_lines.len());
        let after_end = (after_start + CONTEXT_LINES).min(file_lines.len());
        for (i, line) in file_lines[after_start..after_end].iter().enumerate() {
            result.push(format!("{:>num_w$}   {}", after_start + i + 1, line));
        }
    }

    result
}

// ── Write tool: LCS-based full-file diff ──

/// Generate full-file diff (for write tool — old vs new content).
pub fn make_diff(old: &str, new: &str) -> Vec<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let num_w = line_num_width(old_lines.len().max(new_lines.len()));

    if old.is_empty() {
        return new_lines
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:>num_w$} + {l}", i + 1))
            .collect();
    }

    let edits = lcs_diff(&old_lines, &new_lines);
    extract_hunks(&edits, &old_lines, &new_lines, num_w)
}

fn line_num_width(max: usize) -> usize {
    if max >= 10000 {
        5
    } else if max >= 1000 {
        4
    } else {
        3
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Edit {
    Keep,
    Delete,
    Insert,
}

/// LCS-based diff on lines.
fn lcs_diff(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let (n, m) = (old.len(), new.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if old[i] == new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut edits = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            edits.push(Edit::Keep);
            i += 1;
            j += 1;
        } else if j < m && (i == n || dp[i][j + 1] >= dp[i + 1][j]) {
            edits.push(Edit::Insert);
            j += 1;
        } else {
            edits.push(Edit::Delete);
            i += 1;
        }
    }
    edits
}

/// Extract context-windowed hunks with line numbers.
fn extract_hunks(edits: &[Edit], old: &[&str], new: &[&str], num_w: usize) -> Vec<String> {
    let mut changed = vec![false; edits.len()];
    for (i, e) in edits.iter().enumerate() {
        if *e != Edit::Keep {
            let start = i.saturating_sub(CONTEXT_LINES);
            let end = (i + CONTEXT_LINES + 1).min(edits.len());
            for c in &mut changed[start..end] {
                *c = true;
            }
        }
    }

    let mut result = Vec::new();
    let (mut oi, mut ni) = (0usize, 0usize);
    let mut in_hunk = false;

    for (i, edit) in edits.iter().enumerate() {
        if changed[i] {
            if !in_hunk && (oi > 0 || ni > 0) {
                result.push("...".to_owned());
            }
            in_hunk = true;
            match edit {
                Edit::Keep => {
                    result.push(format!("{:>num_w$}   {}", oi + 1, old[oi]));
                    oi += 1;
                    ni += 1;
                }
                Edit::Delete => {
                    result.push(format!("{:>num_w$} - {}", oi + 1, old[oi]));
                    oi += 1;
                }
                Edit::Insert => {
                    result.push(format!("{:>num_w$} + {}", ni + 1, new[ni]));
                    ni += 1;
                }
            }
        } else {
            in_hunk = false;
            match edit {
                Edit::Keep => {
                    oi += 1;
                    ni += 1;
                }
                Edit::Delete => {
                    oi += 1;
                }
                Edit::Insert => {
                    ni += 1;
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── make_diff (LCS) ──

    #[test]
    fn new_file() {
        let diff = make_diff("", "line1\nline2");
        assert_eq!(diff.len(), 2);
        assert!(diff[0].contains("+ line1"), "got: {}", diff[0]);
        assert!(diff[1].contains("+ line2"), "got: {}", diff[1]);
    }

    #[test]
    fn simple_edit() {
        let old = "a\nb\nc\nd";
        let new = "a\nB\nc\nd";
        let diff = make_diff(old, new);
        assert!(
            diff.iter().any(|l| l.contains(" - b")),
            "missing -b: {diff:?}"
        );
        assert!(
            diff.iter().any(|l| l.contains(" + B")),
            "missing +B: {diff:?}"
        );
    }

    #[test]
    fn no_changes() {
        let diff = make_diff("same\n", "same\n");
        assert!(diff.is_empty(), "no-op: {diff:?}");
    }

    #[test]
    fn large_file_context() {
        let old: String = (0..20).map(|i| format!("line{i}\n")).collect();
        let new = old.replace("line10", "CHANGED");
        let diff = make_diff(&old, &new);
        assert!(diff.iter().any(|l| l == "..."), "missing ...: {diff:?}");
        assert!(diff.iter().any(|l| l.contains(" - line10")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + CHANGED")), "{diff:?}");
    }

    // ── make_edit_diff (context-based) ──

    #[test]
    fn edit_diff_single() {
        let file = "aaa\nbbb\nccc\nddd\neee";
        let diff = make_edit_diff(file, "bbb", "BBB", false);
        assert!(diff.iter().any(|l| l.contains(" - bbb")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + BBB")), "{diff:?}");
        // Context: aaa before, ccc+ddd+eee after
        assert!(
            diff.iter().any(|l| l.contains("aaa")),
            "ctx before: {diff:?}"
        );
        assert!(
            diff.iter().any(|l| l.contains("ccc")),
            "ctx after: {diff:?}"
        );
    }

    #[test]
    fn edit_diff_multiline() {
        let file = "a\nb\nc\nd\ne\nf";
        let diff = make_edit_diff(file, "b\nc", "X\nY\nZ", false);
        assert!(diff.iter().any(|l| l.contains(" - b")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" - c")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + X")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + Y")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + Z")), "{diff:?}");
    }

    #[test]
    fn edit_diff_at_start() {
        let file = "first\nsecond\nthird";
        let diff = make_edit_diff(file, "first", "FIRST", false);
        assert!(diff.iter().any(|l| l.contains(" - first")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + FIRST")), "{diff:?}");
        // No ... before (we're at line 1)
        assert!(
            !diff.iter().any(|l| l == "..."),
            "no separator at start: {diff:?}"
        );
    }

    #[test]
    fn edit_diff_at_end() {
        let file = "a\nb\nc\nd\nlast";
        let diff = make_edit_diff(file, "last", "LAST", false);
        assert!(diff.iter().any(|l| l.contains(" - last")), "{diff:?}");
        assert!(diff.iter().any(|l| l.contains(" + LAST")), "{diff:?}");
    }

    #[test]
    fn edit_diff_line_numbers() {
        let file = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
        let diff = make_edit_diff(file, "5", "FIVE", false);
        let del = diff.iter().find(|l| l.contains("- 5")).unwrap();
        let parsed = parse_diff_line(del);
        assert_eq!(parsed.lineno, 5, "line 5: {del}");
        assert_eq!(parsed.kind, DiffKind::Del);
        assert_eq!(parsed.text, "5");
    }

    // ── parse_diff_line ──

    #[test]
    fn parse_add() {
        let dl = parse_diff_line("  42 + fn main() {");
        assert_eq!(dl.kind, DiffKind::Add);
        assert_eq!(dl.lineno, 42);
        assert_eq!(dl.text, "fn main() {");
    }

    #[test]
    fn parse_del() {
        let dl = parse_diff_line(" 7 - old line");
        assert_eq!(dl.kind, DiffKind::Del);
        assert_eq!(dl.lineno, 7);
        assert_eq!(dl.text, "old line");
    }

    #[test]
    fn parse_context() {
        let dl = parse_diff_line("100   let x = 1;");
        assert_eq!(dl.kind, DiffKind::Context);
        assert_eq!(dl.lineno, 100);
        assert_eq!(dl.text, "let x = 1;");
    }

    #[test]
    fn parse_separator() {
        let dl = parse_diff_line("...");
        assert_eq!(dl.kind, DiffKind::Separator);
    }
}
