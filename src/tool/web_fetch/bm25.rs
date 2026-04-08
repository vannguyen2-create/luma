//! BM25 section scoring and passage extraction for excerpt ranking.

use regex::Regex;

const HEADING_BOOST: f64 = 2.0;
const BIGRAM_BOOST: f64 = 1.5;
const PHRASE_BOOST: f64 = 2.5;
const POSITION_DECAY: f64 = 0.1;
const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;
const BOILERPLATE_PENALTY: f64 = 0.65;
const PASSAGE_WINDOW_WORDS: usize = 90;
const PASSAGE_STEP_WORDS: usize = 30;
const EXCERPT_WINDOW_WORDS: usize = 140;
const MIN_BOILERPLATE_WORDS: usize = 12;

const BOILERPLATE_MARKERS: &[&str] = &[
    "table of contents",
    "edit this page",
    "navigation menu",
    "skip to content",
];

pub struct Section {
    pub heading: String,
    pub text: String,
    pub index: usize,
}

pub struct Terms {
    pub unigrams: Vec<String>,
    pub bigrams: Vec<Regex>,
    pub phrase: Option<Regex>,
}

/// Compute IDF weights for unigram patterns across sections.
pub fn compute_idf(sections: &[Section], patterns: &[Regex]) -> Vec<f64> {
    let texts: Vec<String> = sections.iter().map(|s| s.text.to_lowercase()).collect();
    let n = sections.len() as f64;
    patterns
        .iter()
        .map(|p| {
            let df = texts.iter().filter(|t| p.is_match(t)).count() as f64;
            if df > 0.0 {
                ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
            } else {
                0.0
            }
        })
        .collect()
}

/// Score a section using BM25 + boosts. Lowercase computed once.
pub fn score_section(
    s: &Section,
    uni: &[Regex],
    terms: &Terms,
    idf: &[f64],
    avg: f64,
    total: usize,
) -> f64 {
    let lower = s.text.to_lowercase();
    let dl = lower.split_whitespace().count().max(1) as f64;
    let mut score = 0.0;
    for (i, p) in uni.iter().enumerate() {
        let tf = p.find_iter(&lower).count() as f64;
        if tf > 0.0 {
            score += idf[i] * (tf * (BM25_K1 + 1.0))
                / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avg)));
        }
    }
    for bi in &terms.bigrams {
        if bi.is_match(&lower) {
            score *= BIGRAM_BOOST;
        }
    }
    if let Some(ph) = &terms.phrase
        && ph.is_match(&lower)
    {
        score *= PHRASE_BOOST;
    }
    if !s.heading.is_empty() {
        let lh = s.heading.to_lowercase();
        if uni.iter().any(|p| p.is_match(&lh)) {
            score *= HEADING_BOOST;
        }
    }
    if is_boilerplate_lower(&lower) {
        score *= BOILERPLATE_PENALTY;
    }
    score * (1.0 + POSITION_DECAY * (1.0 - s.index as f64 / total.max(1) as f64))
}

/// Extract a windowed excerpt from text, centered on the best passage.
pub fn excerpt_window(text: &str, terms: &Terms) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= EXCERPT_WINDOW_WORDS {
        return text.to_owned();
    }
    let (ps, pl) = best_passage(&words, terms);
    let center = (ps + pl / 2).min(words.len().saturating_sub(1));
    let half = EXCERPT_WINDOW_WORDS / 2;
    let start = center.saturating_sub(half);
    let end = (start + EXCERPT_WINDOW_WORDS).min(words.len());
    let excerpt = words[start..end].join(" ");
    if start > 0 || end < words.len() {
        format!("...{excerpt}...")
    } else {
        excerpt
    }
}

/// Find the best passage window. Pre-lowercases words once.
fn best_passage(words: &[&str], terms: &Terms) -> (usize, usize) {
    let win = PASSAGE_WINDOW_WORDS.min(words.len());
    // Pre-lowercase all words once instead of per-window
    let lower_words: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let (mut bs, mut bl, mut bsc) = (0, win, -1.0f64);
    let mut i = 0;
    while i < words.len() {
        let end = (i + win).min(words.len());
        let sc = passage_score_slice(&lower_words[i..end], terms);
        if sc > bsc {
            bs = i;
            bl = end - i;
            bsc = sc;
        }
        if i + win >= words.len() {
            break;
        }
        i += PASSAGE_STEP_WORDS.max(1);
    }
    (bs, bl)
}

/// Score a passage from pre-lowercased word slice (avoids join + re-lowercase).
fn passage_score_slice(lower_words: &[String], terms: &Terms) -> f64 {
    // Join only when needed for regex matching
    let joined = lower_words.join(" ");
    let mut s = 0.0;
    if let Some(ph) = &terms.phrase
        && ph.is_match(&joined)
    {
        s += 6.0;
    }
    for bi in &terms.bigrams {
        if bi.is_match(&joined) {
            s += 3.0;
        }
    }
    s += terms
        .unigrams
        .iter()
        .filter(|w| joined.contains(w.as_str()))
        .count() as f64;
    if is_boilerplate_lower(&joined) {
        s *= BOILERPLATE_PENALTY;
    }
    s
}

/// Check boilerplate on already-lowercased text.
fn is_boilerplate_lower(lower: &str) -> bool {
    if lower.split_whitespace().count() < MIN_BOILERPLATE_WORDS {
        return true;
    }
    BOILERPLATE_MARKERS.iter().any(|m| lower.contains(m))
}
