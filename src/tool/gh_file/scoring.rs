//! Structure-aware code block scoring for GhFile objective extraction.

use regex::Regex;

const MAX_BLOCKS: usize = 8;
const MAX_OUTPUT_BYTES: usize = 262_144;
const WINDOW_SIZE: usize = 60;
const WINDOW_STEP: usize = 24;

struct CodeBlock {
    start: usize,
    end: usize,
    text: String,
    is_structure: bool,
}

struct QueryTerms {
    tokens: Vec<String>,
    phrase: Option<Regex>,
}

/// Score and extract relevant code blocks from file content.
pub fn format_blocks(content: &str, objective: &str, path: &str) -> Vec<String> {
    let terms = parse_terms(objective);
    if terms.tokens.is_empty() {
        return vec![clip(content)];
    }
    let lines: Vec<&str> = content.lines().collect();
    let blocks = build_blocks(&lines);
    let mut scored: Vec<(usize, f64)> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (i, score_block(b, &terms, path)))
        .filter(|(_, s)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if scored.is_empty() {
        return vec![clip(content)];
    }
    // Deduplicate overlapping blocks
    let mut chosen: Vec<usize> = Vec::new();
    for (idx, _) in &scored {
        let b = &blocks[*idx];
        let overlaps = chosen.iter().any(|&ci| {
            let c = &blocks[ci];
            !(b.end < c.start || b.start > c.end)
        });
        if !overlaps {
            chosen.push(*idx);
        }
        if chosen.len() >= MAX_BLOCKS {
            break;
        }
    }
    chosen.sort_by_key(|&i| blocks[i].start);
    chosen
        .iter()
        .map(|&i| {
            let b = &blocks[i];
            let numbered: String = b
                .text
                .lines()
                .enumerate()
                .map(|(li, line)| format!("{}: {line}", b.start + li))
                .collect::<Vec<_>>()
                .join("\n");
            clip(&numbered)
        })
        .collect()
}

fn parse_terms(objective: &str) -> QueryTerms {
    let tokens: Vec<String> = objective
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(|s| s.to_owned())
        .collect();
    let phrase = if tokens.len() >= 2 {
        let pat = tokens
            .iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join(r"\W+");
        Regex::new(&format!("(?i){pat}")).ok()
    } else {
        None
    };
    QueryTerms { tokens, phrase }
}

/// Detect lines that start a structural code unit.
fn detect_structure_starts(lines: &[&str]) -> Vec<usize> {
    use std::sync::LazyLock;
    static REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        [
            r"^(pub\s+)?(async\s+)?fn\s",
            r"^(pub\s+)?(struct|enum|trait|impl)\s",
            r"^(export\s+)?(async\s+)?function\s",
            r"^(export\s+)?(default\s+)?class\s",
            r"^class\s",
            r"^(export\s+)?(type|interface)\s",
            r"^(pub\s+)?const\s+[A-Z]",
            r"^(export\s+)?const\s+[A-Za-z]",
        ]
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
    });
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            REGEXES.iter().any(|r| r.is_match(trimmed))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Build structure blocks + sliding window blocks.
fn build_blocks(lines: &[&str]) -> Vec<CodeBlock> {
    let starts = detect_structure_starts(lines);
    let mut blocks = Vec::new();
    for (i, &s) in starts.iter().enumerate() {
        let next = starts.get(i + 1).copied().unwrap_or(lines.len());
        let end = next.max(s + 1);
        let text = lines[s..end].join("\n");
        if !text.trim().is_empty() {
            blocks.push(CodeBlock {
                start: s + 1,
                end,
                text,
                is_structure: true,
            });
        }
    }
    let mut ws = 0;
    while ws < lines.len() {
        let we = (ws + WINDOW_SIZE).min(lines.len());
        let text = lines[ws..we].join("\n");
        if !text.trim().is_empty() {
            blocks.push(CodeBlock {
                start: ws + 1,
                end: we,
                text,
                is_structure: false,
            });
        }
        if we >= lines.len() {
            break;
        }
        ws += WINDOW_STEP;
    }
    blocks
}

/// Score a code block against query terms.
fn score_block(block: &CodeBlock, terms: &QueryTerms, path: &str) -> f64 {
    let lower = block.text.to_lowercase();
    let mut score = 0.0;
    if let Some(ph) = &terms.phrase
        && ph.is_match(&lower)
    {
        score += 6.0;
    }
    let lower_path = path.to_lowercase();
    for tok in &terms.tokens {
        score += lower.matches(tok.as_str()).count() as f64;
        if lower_path.contains(tok.as_str()) {
            score += 0.5;
        }
    }
    // First line bonus — reuse already lowercased text
    let first_end = lower.find('\n').unwrap_or(lower.len());
    let first = &lower[..first_end];
    if terms.tokens.iter().any(|t| first.contains(t.as_str())) {
        score += 3.0;
    }
    if block.is_structure {
        score += 1.0;
    }
    score
}

fn clip(text: &str) -> String {
    if text.len() <= MAX_OUTPUT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_OUTPUT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_rust_fn() {
        let lines = vec!["pub fn foo() {", "    bar()", "}"];
        assert_eq!(detect_structure_starts(&lines), vec![0]);
    }

    #[test]
    fn detect_js_function() {
        let lines = vec!["export async function handle() {", "  return 1;", "}"];
        assert_eq!(detect_structure_starts(&lines), vec![0]);
    }

    #[test]
    fn format_blocks_with_objective() {
        let content =
            "fn foo() {\n    println!(\"hello\");\n}\n\nfn bar() {\n    println!(\"world\");\n}";
        let blocks = format_blocks(content, "foo hello", "main.rs");
        assert!(!blocks.is_empty());
        assert!(blocks[0].contains("foo"));
    }

    #[test]
    fn format_blocks_no_objective() {
        let blocks = format_blocks("some code", "", "test.rs");
        assert_eq!(blocks, vec!["some code"]);
    }

    #[test]
    fn overlap_dedup() {
        let content: String = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let blocks = format_blocks(&content, "line", "test.txt");
        assert!(blocks.len() <= MAX_BLOCKS);
    }
}
