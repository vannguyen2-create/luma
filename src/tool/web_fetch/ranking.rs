//! Excerpt ranking — section parsing and orchestration for objective-driven extraction.

use super::bm25::{Section, Terms, compute_idf, excerpt_window, score_section};
use regex::Regex;

const MAX_SECTIONS: usize = 10;
const MAX_SECTION_WORDS: usize = 500;
const MIN_KEYWORD_LEN: usize = 3;
const MAX_BYTES: usize = 262_144;

/// Rank and extract relevant excerpts from markdown given an objective.
pub fn rank_excerpts(markdown: &str, objective: &str) -> String {
    let sections = chunk_oversized(&parse_heading_sections(markdown));
    if sections.is_empty() {
        return clip(markdown);
    }
    let terms = parse_terms(objective);
    if terms.unigrams.is_empty() {
        return clip(markdown);
    }
    let uni_re: Vec<Regex> = terms
        .unigrams
        .iter()
        .filter_map(|w| Regex::new(&format!(r"(?i)\b{}\b", regex::escape(w))).ok())
        .collect();
    let idf = compute_idf(&sections, &uni_re);
    let avg = sections
        .iter()
        .map(|s| s.text.split_whitespace().count())
        .sum::<usize>() as f64
        / sections.len().max(1) as f64;
    let n = sections.len();

    let mut hits: Vec<(usize, f64)> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| (i, score_section(s, &uni_re, &terms, &idf, avg, n)))
        .filter(|(_, s)| *s > 0.0)
        .collect();
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(MAX_SECTIONS);
    hits.sort_by_key(|(i, _)| *i);

    if hits.is_empty() {
        return clip(markdown);
    }
    let excerpts: Vec<String> = hits
        .iter()
        .map(|(i, _)| excerpt_window(&sections[*i].text, &terms))
        .collect();
    clip_many(&excerpts)
}

fn parse_terms(objective: &str) -> Terms {
    let words: Vec<String> = objective
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= MIN_KEYWORD_LEN && !is_stop(w))
        .map(|s| s.to_owned())
        .collect();
    let bigrams: Vec<Regex> = words
        .windows(2)
        .filter_map(|p| {
            Regex::new(&format!(
                r"(?i)\b{}\W+{}\b",
                regex::escape(&p[0]),
                regex::escape(&p[1])
            ))
            .ok()
        })
        .collect();
    let all: Vec<String> = objective
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= MIN_KEYWORD_LEN)
        .map(|s| s.to_owned())
        .collect();
    let phrase = if all.len() >= 2 {
        let pat = all
            .iter()
            .map(|w| regex::escape(w))
            .collect::<Vec<_>>()
            .join(r"\W+");
        Regex::new(&format!(r"(?i)\b{pat}\b")).ok()
    } else {
        None
    };
    Terms {
        unigrams: words,
        bigrams,
        phrase,
    }
}

fn parse_heading_sections(md: &str) -> Vec<Section> {
    let mut out = Vec::new();
    let mut heading = String::new();
    let mut body: Vec<&str> = Vec::new();
    for line in md.lines() {
        if line.starts_with('#') && line.len() > 1 && line.as_bytes()[1] != b'!' {
            flush_section(&heading, &body, &mut out);
            heading = line.to_owned();
            body.clear();
        } else {
            body.push(line);
        }
    }
    flush_section(&heading, &body, &mut out);
    out
}

fn flush_section(heading: &str, body: &[&str], out: &mut Vec<Section>) {
    let joined = body.join("\n").trim().to_owned();
    if heading.is_empty() && joined.is_empty() {
        return;
    }
    let text = if heading.is_empty() {
        joined
    } else {
        format!("{heading}\n{joined}")
    };
    out.push(Section {
        heading: heading.to_owned(),
        text,
        index: out.len(),
    });
}

fn chunk_oversized(sections: &[Section]) -> Vec<Section> {
    let mut out = Vec::new();
    for s in sections {
        if s.text.split_whitespace().count() <= MAX_SECTION_WORDS {
            out.push(Section {
                heading: s.heading.clone(),
                text: s.text.clone(),
                index: out.len(),
            });
            continue;
        }
        let mut chunk: Vec<&str> = Vec::new();
        let mut cw = 0;
        for para in s.text.split("\n\n") {
            let pw = para.split_whitespace().count();
            if cw + pw > MAX_SECTION_WORDS && !chunk.is_empty() {
                out.push(Section {
                    heading: s.heading.clone(),
                    text: chunk.join("\n\n"),
                    index: out.len(),
                });
                chunk.clear();
                cw = 0;
            }
            chunk.push(para);
            cw += pw;
        }
        if !chunk.is_empty() {
            out.push(Section {
                heading: s.heading.clone(),
                text: chunk.join("\n\n"),
                index: out.len(),
            });
        }
    }
    out
}

fn is_stop(w: &str) -> bool {
    matches!(
        w,
        "the"
            | "and"
            | "for"
            | "are"
            | "but"
            | "not"
            | "you"
            | "all"
            | "can"
            | "her"
            | "was"
            | "one"
            | "our"
            | "out"
            | "has"
            | "have"
            | "had"
            | "been"
            | "from"
            | "this"
            | "that"
            | "with"
            | "they"
            | "which"
            | "their"
            | "will"
            | "each"
            | "make"
            | "like"
            | "just"
            | "over"
            | "such"
            | "than"
            | "them"
            | "very"
            | "some"
            | "what"
            | "about"
            | "into"
            | "more"
            | "other"
            | "then"
            | "these"
            | "when"
            | "where"
            | "how"
            | "does"
            | "also"
            | "after"
            | "should"
            | "would"
            | "could"
            | "being"
            | "there"
            | "before"
            | "between"
            | "those"
            | "through"
            | "while"
            | "using"
    )
}

fn clip(text: &str) -> String {
    if text.len() <= MAX_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn clip_many(excerpts: &[String]) -> String {
    let mut used = 0;
    let mut parts = Vec::new();
    for e in excerpts {
        let eb = e.len() + 2;
        if used + eb > MAX_BYTES {
            let rem = MAX_BYTES - used;
            if rem > 100 {
                parts.push(clip(e));
            }
            break;
        }
        parts.push(e.clone());
        used += eb;
    }
    if parts.is_empty() {
        clip(&excerpts[0])
    } else {
        parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sections_by_heading() {
        let md = "# Intro\nsome text\n## Details\nmore text";
        let sections = parse_heading_sections(md);
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn chunk_large_section() {
        let paras: Vec<String> = (0..20)
            .map(|i| {
                (0..40)
                    .map(|j| format!("w{i}_{j}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let long = paras.join("\n\n");
        let sections = parse_heading_sections(&format!("# Big\n{long}"));
        assert!(chunk_oversized(&sections).len() > 1);
    }

    #[test]
    fn rank_returns_relevant() {
        let md =
            "# Install\nRun cargo install.\n\n# Usage\nUse `luma run` to start.\n\n# License\nMIT";
        let result = rank_excerpts(md, "how to use luma");
        assert!(result.contains("luma"));
    }

    #[test]
    fn rank_empty_objective_returns_full() {
        assert_eq!(rank_excerpts("content", ""), "content");
    }

    #[test]
    fn stop_words_filtered() {
        let terms = parse_terms("the quick brown fox");
        assert!(!terms.unigrams.contains(&"the".to_owned()));
        assert!(terms.unigrams.contains(&"quick".to_owned()));
    }
}
