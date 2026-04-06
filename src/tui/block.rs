/// Block types — content blocks for the output log.
use crate::tool::diff::{parse_diff_line, DiffKind};
use crate::tui::markdown::BlockState;
use crate::tui::markdown::highlight::highlight_code_with_lang;
use crate::tui::stream::StreamBuf;
use crate::tui::text::{Line, Span};
use crate::tui::theme::{icon, palette, Rgb};
use smallvec::smallvec;

const TOOL_PREVIEW_LINES: usize = 12;
const WRITE_TOOLS: &[&str] = &["Write", "Edit", "apply_patch"];
const SEARCH_TOOLS: &[&str] = &["web_search", "WebSearch"];

fn is_search_tool(name: &str) -> bool {
    SEARCH_TOOLS.contains(&name)
}

fn tool_icon(name: &str) -> &'static str {
    if is_write_tool(name) { icon::TOOL_IN }
    else { icon::TOOL_OUT }
}

/// A content block in the output log.
#[derive(Debug, Clone)]
pub enum Block {
    Gap,
    GapLabel(String),
    /// (lines, precomputed max display width)
    Logo(Vec<String>, usize),
    Info(String),
    Error(String),
    Warn(String),
    User(Vec<String>),
    Thinking(StreamBuf),
    Text(TextBlock),
    Tool(ToolBlock),
    Skill(SkillBlock),
}

/// Assistant text block — StreamBuf + incremental markdown cache.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub stream: StreamBuf,
    cache: MdCache,
}

/// Incremental markdown render cache.
#[derive(Debug, Clone)]
struct MdCache {
    /// Number of committed lines already parsed.
    parsed_count: usize,
    /// Rendered Lines from committed[0..parsed_count].
    lines: Vec<Line>,
    /// Markdown state after parsing committed[parsed_count-1].
    md_state: BlockState,
    /// Width used for wrapping — invalidate on change.
    width: usize,
    /// How many trailing Lines came from table rows (need re-render if table continues).
    trailing_table_lines: usize,
    /// How many trailing committed lines were table rows.
    trailing_table_rows: usize,
}

impl MdCache {
    fn new() -> Self {
        Self {
            parsed_count: 0,
            lines: Vec::new(),
            md_state: BlockState::new(),
            width: 0,
            trailing_table_lines: 0,
            trailing_table_rows: 0,
        }
    }

    fn invalidate(&mut self) {
        self.parsed_count = 0;
        self.lines.clear();
        self.md_state = BlockState::new();
        self.trailing_table_lines = 0;
        self.trailing_table_rows = 0;
    }
}

impl TextBlock {
    /// Create a new text block.
    pub fn new() -> Self {
        Self {
            stream: StreamBuf::new(),
            cache: MdCache::new(),
        }
    }

    /// Feed a streaming token.
    pub fn feed(&mut self, token: &str) {
        self.stream.feed(token);
    }

    /// Flush partial into committed.
    pub fn flush(&mut self) {
        self.stream.flush();
    }

    /// Whether there's any content.
    pub fn is_empty(&self) -> bool {
        self.stream.is_empty()
    }
}

/// Tool invocation block.
#[derive(Debug, Clone)]
pub struct ToolBlock {
    pub name: String,
    pub summary: String,
    pub output: Vec<String>,
    pub stream: Option<StreamBuf>,
    pub is_done: bool,
    pub end_summary: String,
    pub is_expanded: bool,
}

/// Skill activation block.
#[derive(Debug, Clone)]
pub struct SkillBlock {
    pub name: String,
    pub is_done: bool,
    pub end_summary: String,
}

/// Render a block into Lines for a given content width.
pub fn render_block(block: &Block, width: usize, spinner_frame: usize) -> Vec<Line> {
    match block {
        Block::Gap => vec![Line::empty()],
        Block::GapLabel(label) => {
            vec![Line::new(smallvec![Span::new(label.clone(), palette::MUTED)])]
        }
        Block::Logo(lines, max_w) => {
            let pad = (width.saturating_sub(*max_w) * 2 / 5) as u16;
            lines
                .iter()
                .map(|l| {
                    let mut line = Line::new(smallvec![Span::new(l.clone(), palette::MUTED)]);
                    line.indent = pad;
                    line
                })
                .collect()
        }

        Block::Info(t) => wrap_simple(icon::INFO, palette::DIM, t, width),

        Block::Error(t) => wrap_simple(icon::ERROR, palette::ERROR, t, width),
        Block::Warn(t) => wrap_simple(icon::WARN, palette::WARN, t, width),

        Block::User(lines) => render_user(lines, width),

        Block::Thinking(stream) => {
            let mut result = Vec::new();
            let mut first = true;
            for text in &stream.committed {
                let (line, pad) = if first {
                    first = false;
                    (Line::new(smallvec![
                        Span::italic("Thinking: ".to_owned(), palette::WARN),
                        Span::new(text.clone(), palette::MUTED),
                    ]), Some("  "))
                } else {
                    (Line::new(smallvec![Span::new(text.clone(), palette::MUTED)]), None)
                };
                result.extend(crate::tui::text::wrap_line(&line, width, pad));
            }
            if !stream.partial().is_empty() {
                let (line, pad) = if first {
                    (Line::new(smallvec![
                        Span::italic("Thinking: ".to_owned(), palette::WARN),
                        Span::new(stream.partial().to_owned(), palette::MUTED),
                    ]), Some("  "))
                } else {
                    (Line::new(smallvec![Span::new(stream.partial().to_owned(), palette::MUTED)]), None)
                };
                result.extend(crate::tui::text::wrap_line(&line, width, pad));
            }
            result.push(Line::empty());
            result
        }

        Block::Text(tb) => render_text_markdown(tb, width),

        Block::Tool(tb) => render_tool(tb, width, spinner_frame),

        Block::Skill(sb) => {
            if sb.is_done {
                wrap_simple(icon::SKILL, palette::MUTED, &sb.end_summary, width)
            } else {
                let line = Line::new(smallvec![
                    Span::new(format!("{} ", icon::SKILL), palette::SUCCESS),
                    Span::bold(sb.name.clone(), palette::SUCCESS),
                ]);
                crate::tui::text::wrap_line(&line, width, None)
            }
        }
    }
}

/// Render a block mutably — uses incremental cache for Text blocks.
pub fn render_block_mut(block: &mut Block, width: usize, spinner_frame: usize) -> Vec<Line> {
    if let Block::Text(tb) = block {
        return render_text_incremental(tb, width);
    }
    render_block(block, width, spinner_frame)
}

/// Incremental markdown render — only parses new committed lines.
fn render_text_incremental(tb: &mut TextBlock, width: usize) -> Vec<Line> {
    use crate::tui::markdown::{is_table_line, parse_inline_streaming, parse_line, render_table};

    let cache = &mut tb.cache;
    let committed = &tb.stream.committed;

    // Invalidate if width changed
    if cache.width != width {
        cache.invalidate();
        cache.width = width;
    }

    // Check if new lines continue a table that was partially cached
    let new_start = if cache.parsed_count > 0
        && cache.parsed_count <= committed.len()
        && cache.trailing_table_rows > 0
        && committed.get(cache.parsed_count).is_some_and(|l| is_table_line(l))
    {
        // Roll back: remove trailing table lines from cache, re-parse from there
        for _ in 0..cache.trailing_table_lines {
            cache.lines.pop();
        }
        cache.parsed_count -= cache.trailing_table_rows;
        // Restore md_state to before the table — Normal is safe since tables
        // don't nest and always start from Normal
        cache.md_state = BlockState::Normal;
        cache.trailing_table_lines = 0;
        cache.trailing_table_rows = 0;
        cache.parsed_count
    } else if cache.parsed_count <= committed.len() {
        cache.parsed_count
    } else {
        // committed shrank (shouldn't happen, but be safe)
        cache.invalidate();
        cache.width = width;
        0
    };

    // Parse new committed lines incrementally
    let mut table_rows: Vec<String> = Vec::new();

    for text in &committed[new_start..] {
        let is_table = is_table_line(text);

        if !table_rows.is_empty() && !is_table {
            let rendered = render_table(&table_rows);
            cache.trailing_table_lines = 0;
            cache.trailing_table_rows = 0;
            for rl in rendered {
                let is_empty = rl.visible_width() == 0;
                let prev_empty = cache.lines.last().is_none_or(|p: &Line| p.visible_width() == 0);
                if is_empty && prev_empty { continue; }
                cache.lines.push(rl);
            }
            table_rows.clear();
        }

        if is_table {
            table_rows.push(text.clone());
            let (_, new_state) = parse_line(text, &cache.md_state);
            cache.md_state = new_state;
        } else {
            let (lines, new_state) = parse_line(text, &cache.md_state);
            for l in lines {
                for wl in crate::tui::text::wrap_line(&l, width, None) {
                    let is_empty = wl.visible_width() == 0;
                    let prev_empty = cache.lines.last().is_none_or(|p: &Line| p.visible_width() == 0);
                    if is_empty && prev_empty { continue; }
                    cache.lines.push(wl);
                }
            }
            cache.md_state = new_state;
            cache.trailing_table_lines = 0;
            cache.trailing_table_rows = 0;
        }
    }

    // Flush pending table rows into cache
    if !table_rows.is_empty() {
        let rendered = render_table(&table_rows);
        let mut count = 0;
        for rl in rendered {
            let is_empty = rl.visible_width() == 0;
            let prev_empty = cache.lines.last().is_none_or(|p: &Line| p.visible_width() == 0);
            if is_empty && prev_empty { continue; }
            cache.lines.push(rl);
            count += 1;
        }
        cache.trailing_table_lines = count;
        cache.trailing_table_rows = table_rows.len();
    }

    cache.parsed_count = committed.len();

    // Build result: cached lines + partial
    let mut result = cache.lines.clone();

    // Partial line
    let partial = tb.stream.partial();
    if !partial.is_empty() && is_table_line(partial) {
        // Don't render partial table — wait for newline
    } else if !partial.is_empty() {
        let spans = parse_inline_streaming(partial);
        result.push(Line::new(spans));
    }

    // Strip leading/trailing empty lines
    let leading = result.iter().take_while(|l| l.visible_width() == 0).count();
    if leading > 0 {
        result.drain(..leading);
    }
    while result.last().is_some_and(|l| l.visible_width() == 0) {
        result.pop();
    }
    result
}

/// Non-incremental markdown render (for immutable Block references).
fn render_text_markdown(tb: &TextBlock, width: usize) -> Vec<Line> {
    use crate::tui::markdown::{is_table_line, parse_inline_streaming, parse_line, render_table};

    let stream = &tb.stream;
    let mut result = Vec::new();
    let mut md_state = BlockState::new();
    let mut table_rows: Vec<String> = Vec::new();

    for text in &stream.committed {
        let is_table = is_table_line(text);

        if !table_rows.is_empty() && !is_table {
            for rl in render_table(&table_rows) {
                let is_empty = rl.visible_width() == 0;
                let prev_empty = result.last().is_none_or(|p: &Line| p.visible_width() == 0);
                if is_empty && prev_empty { continue; }
                result.push(rl);
            }
            table_rows.clear();
        }

        if is_table {
            table_rows.push(text.clone());
            let (_, new_state) = parse_line(text, &md_state);
            md_state = new_state;
        } else {
            let (lines, new_state) = parse_line(text, &md_state);
            for l in lines {
                for wl in crate::tui::text::wrap_line(&l, width, None) {
                    let is_empty = wl.visible_width() == 0;
                    let prev_empty = result.last().is_none_or(|p: &Line| p.visible_width() == 0);
                    if is_empty && prev_empty { continue; }
                    result.push(wl);
                }
            }
            md_state = new_state;
        }
    }

    if !stream.partial().is_empty() && is_table_line(stream.partial()) {
        table_rows.push(stream.partial().to_owned());
    }
    if !table_rows.is_empty() {
        for rl in render_table(&table_rows) {
            let is_empty = rl.visible_width() == 0;
            let prev_empty = result.last().is_none_or(|p: &Line| p.visible_width() == 0);
            if is_empty && prev_empty { continue; }
            result.push(rl);
        }
    }

    if !stream.partial().is_empty() && !is_table_line(stream.partial()) {
        let spans = parse_inline_streaming(stream.partial());
        result.push(Line::new(spans));
    }

    let leading = result.iter().take_while(|l| l.visible_width() == 0).count();
    if leading > 0 {
        result.drain(..leading);
    }
    while result.last().is_some_and(|l| l.visible_width() == 0) {
        result.pop();
    }
    result
}

fn wrap_simple(ic: &str, color: Rgb, text: &str, w: usize) -> Vec<Line> {
    let line = Line::new(smallvec![
        Span::new(format!("{ic} "), color),
        Span::new(text.to_owned(), color),
    ]);
    crate::tui::text::wrap_line(&line, w, None)
}

fn render_user(lines: &[String], content_w: usize) -> Vec<Line> {
    use crate::tui::theme::CONTENT_PAD;
    let bg = palette::USER_BG;
    let bleed = CONTENT_PAD;
    let bar_str = format!("{}  ", icon::PROMPT);
    let bar_w = crate::tui::text::display_width(&bar_str);
    let inner_w = content_w.saturating_sub(bar_w);
    let bar_line = Line {
        spans: smallvec![Span::deco(icon::PROMPT.to_owned(), palette::ACCENT)],
        bg: Some(bg), margin: false, indent: 0, bleed,
    };
    let mut result = vec![bar_line.clone()];
    for t in lines {
        let plain = Line::new(smallvec![Span::new(t.clone(), palette::FG)]);
        let wrapped = crate::tui::text::wrap_line(&plain, inner_w, None);
        for wl in wrapped {
            let mut spans = smallvec![Span::deco(bar_str.clone(), palette::ACCENT)];
            spans.extend(wl.spans);
            result.push(Line { spans, bg: Some(bg), margin: false, indent: 0, bleed });
        }
    }
    result.push(bar_line);
    result
}

/// Whether a tool is a write/edit tool (block layout when completed with output).
fn is_write_tool(name: &str) -> bool {
    WRITE_TOOLS.contains(&name)
}

/// Infer language hint from a file path (for syntax highlighting).
fn lang_from_path(path: &str) -> Option<&str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "mjs" | "cjs" => Some("js"),
        "ts" | "mts" | "cts" | "tsx" | "jsx" => Some("ts"),
        _ => None,
    }
}

/// Render a diff output line — line number + marker + syntax-highlighted content + bg color.
#[cfg(test)]
fn diff_line(raw: &str) -> Line {
    diff_line_lang(raw, None)
}

/// Render a diff output line with optional language hint for syntax highlighting.
fn diff_line_lang(raw: &str, lang: Option<&str>) -> Line {
    let dl = parse_diff_line(raw);

    if dl.kind == DiffKind::Separator {
        return Line::new(smallvec![
            Span::new("  ".to_owned(), palette::DIM),
            Span::new("...".to_owned(), palette::MUTED),
        ]);
    }

    let (marker, marker_color, bg) = match dl.kind {
        DiffKind::Add => ("+", palette::DIFF_ADD, Some(palette::DIFF_ADD_BG)),
        DiffKind::Del => ("-", palette::DIFF_DEL, Some(palette::DIFF_DEL_BG)),
        DiffKind::Context => (" ", palette::DIM, None),
        DiffKind::Separator => unreachable!(),
    };

    let mut spans = smallvec![
        Span::new("  ".to_owned(), palette::DIM),
    ];

    // Line number
    if dl.lineno > 0 {
        let mut num_span = Span::new(format!("{:>3} ", dl.lineno), palette::DIFF_NUM);
        num_span.bg = bg;
        spans.push(num_span);
    }

    // Marker
    let mut marker_span = Span::new(format!("{marker} "), marker_color);
    marker_span.bg = bg;
    spans.push(marker_span);

    // Content — syntax highlighted for add/del, plain for context
    if dl.kind == DiffKind::Add || dl.kind == DiffKind::Del {
        let code_spans = highlight_code_with_lang(&dl.text, lang);
        for mut s in code_spans {
            s.bg = bg;
            spans.push(s);
        }
    } else {
        let mut s = Span::new(dl.text, palette::DIM);
        s.bg = bg;
        spans.push(s);
    }

    Line::new(spans)
}

fn render_tool(tb: &ToolBlock, content_w: usize, spinner_frame: usize) -> Vec<Line> {
    let is_write = is_write_tool(&tb.name);

    // ── Pending / streaming (not done) ──
    if !tb.is_done {
        let spinner = icon::SPINNER[spinner_frame % icon::SPINNER.len()];
        let has_content = !tb.output.is_empty()
            || tb.stream.as_ref().is_some_and(|s| !s.is_empty());

        let mut h = smallvec![Span::new(format!("{spinner} "), palette::ACCENT)];
        if has_content || !tb.summary.is_empty() {
            h.push(Span::bold(tb.name.clone(), palette::ACCENT));
            h.push(Span::new(format!(" {}", tb.summary), palette::DIM));
        } else {
            h.push(Span::new(format!("preparing {}...", tb.name), palette::MUTED));
        }
        let mut result = crate::tui::text::wrap_line(&Line::new(h), content_w, None);

        // Show streaming content for write tools
        if is_write {
            let lang = lang_from_path(&tb.summary);
            // Diff lines already drained from tool_output
            for t in &tb.output {
                result.push(diff_line_lang(t, lang));
            }
            // Streaming preview from tool_input (content being written)
            if let Some(stream) = &tb.stream {
                let total = stream.committed.len();
                if total > 0 {
                    let show = &stream.committed[total.saturating_sub(TOOL_PREVIEW_LINES)..];
                    for line in show {
                        result.push(Line::new(smallvec![
                            Span::new("  ".to_owned(), palette::DIM),
                            Span::new(line.clone(), palette::DIM),
                        ]));
                    }
                }
                if !stream.partial().is_empty() {
                    result.push(Line::new(smallvec![
                        Span::new("  ".to_owned(), palette::DIM),
                        Span::new(stream.partial().to_owned(), palette::DIM),
                    ]));
                }
            }
        } else {
            // Non-write: show last few output lines
            let total = tb.output.len();
            if total > 0 {
                let show = &tb.output[total.saturating_sub(TOOL_PREVIEW_LINES)..];
                for t in show {
                    let color = if t.starts_with("... ") { palette::MUTED } else { palette::DIM };
                    result.push(Line::new(smallvec![
                        Span::new("  ".to_owned(), palette::DIM),
                        Span::new(t.clone(), color),
                    ]));
                }
            }
            if let Some(stream) = &tb.stream
                && !stream.partial().is_empty()
            {
                result.push(Line::new(smallvec![
                    Span::new("  ".to_owned(), palette::DIM),
                    Span::new(stream.partial().to_owned(), palette::DIM),
                ]));
            }
        }
        return result;
    }

    // ── Completed: write tools → block layout with diff ──
    if is_write && !tb.output.is_empty() {
        return render_tool_block(tb, content_w);
    }

    // ── Completed: search tools → query + results ──
    if is_search_tool(&tb.name) {
        return render_search_block(tb, content_w);
    }

    // ── Completed: inline layout ──
    render_tool_inline(tb, content_w)
}

/// Completed write/edit tool — block layout with title bar + diff content.
fn render_tool_block(tb: &ToolBlock, content_w: usize) -> Vec<Line> {
    let ic = tool_icon(&tb.name);
    let title = format!(
        "{} {} {}",
        ic,
        tb.name,
        tb.summary,
    );
    let mut h = smallvec![Span::new(title, palette::DIM)];
    if !tb.end_summary.is_empty() {
        let sc = if tb.end_summary.contains("exit") { palette::ERROR } else { palette::DIM };
        h.push(Span::new(format!(" {}", tb.end_summary), sc));
    }
    let total = tb.output.len();
    if total > TOOL_PREVIEW_LINES {
        if tb.is_expanded {
            h.push(Span::new(" (click to collapse)".to_owned(), palette::MUTED));
        } else {
            h.push(Span::new(
                format!(" ({total} lines · click to expand)"),
                palette::MUTED,
            ));
        }
    }
    let mut result = crate::tui::text::wrap_line(&Line::new(h), content_w, None);

    let show = if tb.is_expanded || total <= TOOL_PREVIEW_LINES {
        &tb.output[..]
    } else {
        &tb.output[total.saturating_sub(TOOL_PREVIEW_LINES)..]
    };
    let lang = lang_from_path(&tb.summary);
    for t in show {
        result.push(diff_line_lang(t, lang));
    }
    result
}

/// Completed non-write tool — single inline line.
/// Completed search tool — query + numbered results with title, URL, snippet.
fn render_search_block(tb: &ToolBlock, content_w: usize) -> Vec<Line> {
    let ic = tool_icon(&tb.name);
    let query = &tb.summary;
    let mut h = smallvec![
        Span::new(format!("{ic} "), palette::DIM),
        Span::new("Search".to_owned(), palette::DIM),
    ];
    if !query.is_empty() {
        h.push(Span::new(format!("  \"{query}\""), palette::FG));
    }
    if !tb.end_summary.is_empty() {
        h.push(Span::new(format!("  {}", tb.end_summary), palette::MUTED));
    }
    let mut result = crate::tui::text::wrap_line(&Line::new(h), content_w, None);

    // Parse structured output: blocks of "title\nurl\n[snippet\n]\n"
    // Render 1 line per result: "  title — domain"
    let mut idx = 0;
    let mut hit_num = 0;
    while idx < tb.output.len() {
        let title = tb.output[idx].trim();
        if title.is_empty() { idx += 1; continue; }
        hit_num += 1;
        let url = tb.output.get(idx + 1).map(|s| s.trim()).unwrap_or("");
        let snippet = if idx + 2 < tb.output.len() {
            let s = tb.output[idx + 2].trim();
            if s.is_empty() || s.starts_with("http") { "" } else { s }
        } else { "" };

        let domain = if url.is_empty() { String::new() } else { extract_domain(url) };

        // Truncate title to fit: "  N. title — domain"
        let prefix_len = format!("  {hit_num}. ").len();
        let suffix = if domain.is_empty() { String::new() } else { format!(" -- {domain}") };
        let max_title = content_w.saturating_sub(prefix_len + suffix.len());
        let display_title = if title.len() > max_title && max_title > 3 {
            format!("{}...", &title[..max_title - 3])
        } else {
            title.to_owned()
        };

        result.push(Line::new(smallvec![
            Span::new(format!("  {hit_num}. "), palette::MUTED),
            Span::new(display_title, palette::FG),
            Span::new(suffix, palette::MUTED),
        ]));

        // Advance past title + url + optional snippet + blank
        idx += if snippet.is_empty() { 3 } else { 4 };
    }
    result
}

/// Extract domain from URL: "https://docs.rs/tokio/..." → "docs.rs"
fn extract_domain(url: &str) -> String {
    url.trim_start_matches("https://")
       .trim_start_matches("http://")
       .trim_start_matches("www.")
       .split('/')
       .next()
       .unwrap_or(url)
       .to_owned()
}

fn render_tool_inline(tb: &ToolBlock, content_w: usize) -> Vec<Line> {
    let ic = tool_icon(&tb.name);
    let mut h = smallvec![
        Span::new(format!("{ic} "), palette::DIM),
        Span::new(tb.name.clone(), palette::DIM),
    ];
    if !tb.summary.is_empty() {
        h.push(Span::new(format!(" {}", tb.summary), palette::DIM));
    }
    if !tb.end_summary.is_empty() {
        let sc = if tb.end_summary.contains("exit") { palette::ERROR } else { palette::DIM };
        h.push(Span::new(format!(" {}", tb.end_summary), sc));
    }
    let total = tb.output.len();
    if total > TOOL_PREVIEW_LINES {
        if tb.is_expanded {
            h.push(Span::new(" (click to collapse)".to_owned(), palette::MUTED));
        } else {
            h.push(Span::new(
                format!(" ({total} lines · click to expand)"),
                palette::MUTED,
            ));
        }
    }
    let mut result = crate::tui::text::wrap_line(&Line::new(h), content_w, None);

    if total > 0 {
        let show = if tb.is_expanded || total <= TOOL_PREVIEW_LINES {
            &tb.output[..]
        } else {
            &tb.output[total.saturating_sub(TOOL_PREVIEW_LINES)..]
        };
        for t in show {
            let color = if t.starts_with("... ") { palette::MUTED } else { palette::DIM };
            result.push(Line::new(smallvec![
                Span::new("  ".to_owned(), palette::DIM),
                Span::new(t.clone(), color),
            ]));
        }
    }
    result
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_gap() {
        let lines = render_block(&Block::Gap, 80, 0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].visible_width(), 0);
    }

    #[test]
    fn render_info() {
        let lines = render_block(&Block::Info("hello".into()), 80, 0);
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_user_block() {
        let lines = render_block(&Block::User(vec!["hi".into()]), 80, 0);
        assert!(lines.len() >= 3);
    }

    #[test]
    fn render_tool_collapsed() {
        let tb = ToolBlock {
            name: "Bash".into(),
            summary: "$ ls".into(),
            output: (0..20).map(|i| format!("line {i}")).collect(),
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        assert!(lines.len() < 20);
    }

    #[test]
    fn render_tool_expanded() {
        let tb = ToolBlock {
            name: "Bash".into(),
            summary: "$ ls".into(),
            output: (0..20).map(|i| format!("line {i}")).collect(),
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: true,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        assert!(lines.len() >= 21);
    }

    #[test]
    fn tool_icon_write_vs_read() {
        assert_eq!(tool_icon("Write"), icon::TOOL_IN);
        assert_eq!(tool_icon("Edit"), icon::TOOL_IN);
        assert_eq!(tool_icon("Bash"), icon::TOOL_OUT);
        assert_eq!(tool_icon("Grep"), icon::TOOL_OUT);
        assert_eq!(tool_icon("web_search"), icon::TOOL_OUT);
    }

    #[test]
    fn tool_pending_shows_spinner() {
        let tb = ToolBlock {
            name: "Edit".into(),
            summary: String::new(),
            output: Vec::new(),
            stream: None,
            is_done: false,
            end_summary: String::new(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        let text: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("Edit"), "should show tool name: {text}");
        assert!(text.contains("preparing Edit"), "write tool pending: {text}");
    }

    #[test]
    fn tool_inline_completed_read() {
        let tb = ToolBlock {
            name: "Read".into(),
            summary: "src/main.rs".into(),
            output: Vec::new(),
            stream: None,
            is_done: true,
            end_summary: "(45 lines)".into(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        assert_eq!(lines.len(), 1, "inline tool should be 1 line");
        let text: String = lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("→"), "read tool icon: {text}");
        assert!(text.contains("src/main.rs"), "file path: {text}");
        assert!(text.contains("(45 lines)"), "end summary: {text}");
    }

    #[test]
    fn tool_block_completed_write_with_diff() {
        let tb = ToolBlock {
            name: "Write".into(),
            summary: "src/main.rs".into(),
            output: vec![
                "  1 + fn main() {".into(),
                "  2 +     println!(\"hello\");".into(),
                "  3 + }".into(),
            ],
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        // Title + 3 diff lines = 4
        assert_eq!(lines.len(), 4, "block: title + 3 lines");
        let title: String = lines[0].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(title.contains("←"), "write icon in title: {title}");
        assert!(title.contains("Write"), "tool name in title: {title}");
        // Diff lines should have bg color and syntax highlight
        let first_diff: String = lines[1].spans.iter().map(|s| s.text.as_str()).collect();
        assert!(first_diff.contains("fn"), "diff content: {first_diff}");
        assert!(first_diff.contains("main"), "diff content: {first_diff}");
        // Should have add bg color
        assert!(lines[1].spans.iter().any(|s| s.bg == Some(palette::DIFF_ADD_BG)),
            "missing add bg: {:?}", lines[1].spans.iter().map(|s| (s.text.as_str(), s.bg)).collect::<Vec<_>>());
    }

    #[test]
    fn tool_write_no_output_uses_inline() {
        let tb = ToolBlock {
            name: "Write".into(),
            summary: "src/main.rs".into(),
            output: Vec::new(),
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        // No output → falls through to inline
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn tool_streaming_write_shows_content() {
        let mut stream = StreamBuf::new();
        stream.feed("  1 + fn main() {\n");
        stream.feed("  2 +     println!(\"hi\");\n");
        stream.feed("partial line");
        let tb = ToolBlock {
            name: "Write".into(),
            summary: "src/main.rs".into(),
            output: vec!["  1 + fn main() {".into(), "  2 +     println!(\"hi\");".into()],
            stream: Some(stream),
            is_done: false,
            end_summary: String::new(),
            is_expanded: false,
        };
        let lines = render_block(&Block::Tool(tb), 80, 0);
        let all: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(all.contains("fn"), "streaming content: {all}");
        assert!(all.contains("partial line"), "partial line: {all}");
    }

    #[test]
    fn diff_line_colors() {
        let add = diff_line("  1 + added");
        let del = diff_line("  2 - removed");
        let ctx = diff_line("  3   context");
        let sep = diff_line("...");
        // Add: has add bg
        assert!(add.spans.iter().any(|s| s.bg == Some(palette::DIFF_ADD_BG)), "add bg");
        // Del: has del bg
        assert!(del.spans.iter().any(|s| s.bg == Some(palette::DIFF_DEL_BG)), "del bg");
        // Context: no bg
        assert!(ctx.spans.iter().all(|s| s.bg.is_none()), "ctx no bg");
        // Separator
        assert!(sep.spans.iter().any(|s| s.text.contains("...")), "separator");
    }

    #[test]
    fn diff_line_syntax_highlight() {
        let line = diff_line("  1 + fn main() {");
        let all: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        assert!(all.contains("fn"), "content: {all}");
        assert!(all.contains("main"), "content: {all}");
        // `fn` keyword should be highlighted (not default FG)
        let fn_span = line.spans.iter().find(|s| s.text == "fn");
        assert!(fn_span.is_some(), "fn span missing. spans: {:?}",
            line.spans.iter().map(|s| (&s.text, s.fg)).collect::<Vec<_>>());
        assert_ne!(fn_span.unwrap().fg, palette::FG, "fn should be highlighted");
    }

    #[test]
    fn diff_line_ansi_has_bg() {
        use crate::tui::text::ScreenBuffer;
        let line = diff_line("  1 + let x = 42;");
        let mut buf = ScreenBuffer::new(80, 1, palette::BG);
        buf.write_line(&line, 0, 0, 80);
        let ansi = buf.render_row(0);
        // ANSI should contain bg color escape for DIFF_ADD_BG (30, 50, 30)
        assert!(ansi.contains("48;2;30;50;30"),
            "missing add bg in ANSI:\n{ansi}");
    }

    #[test]
    fn full_edit_tool_flow() {
        use crate::tui::output::OutputLog;
        let mut log = OutputLog::new(80, 30);
        log.tool_start("Edit", "");
        // Simulate tool sending diff via output
        log.tool_output("Edit", "  1   aaa\n");
        log.tool_output("Edit", "  2 - bbb\n");
        log.tool_output("Edit", "  2 + BBB\n");
        log.tool_output("Edit", "  3   ccc\n");
        log.tool_start("Edit", "test.rs");
        log.tool_end("Edit", "");

        log.prepare_frame();
        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().enumerate().map(|(i, l)| {
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: {t}")
        }).collect();
        let all = dump.join("\n");

        // Should contain diff content
        assert!(all.contains("bbb"), "missing old line:\n{all}");
        assert!(all.contains("BBB"), "missing new line:\n{all}");

        // Check spans have bg colors
        for l in &vis {
            let text: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            if text.contains("bbb") {
                assert!(l.spans.iter().any(|s| s.bg == Some(palette::DIFF_DEL_BG)),
                    "del line missing bg: {text}");
            }
            if text.contains("BBB") {
                assert!(l.spans.iter().any(|s| s.bg == Some(palette::DIFF_ADD_BG)),
                    "add line missing bg: {text}");
            }
        }
    }

    #[test]
    fn code_fence_then_header_with_emoji() {
        let input = "\
**Phase 2: Freemium Launch**
```
- Open source core
- Nếu traction tốt → expand
```

---

### 💡 **Unique Value Props (cần define rõ):**

Để kiếm tiền, cần trả lời:";

        let mut tb = TextBlock::new();
        tb.stream.feed(input);
        tb.stream.flush();

        let lines = render_text_incremental(&mut tb, 80);
        let dump: Vec<String> = lines.iter().enumerate().map(|(i, l)| {
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: {t}")
        }).collect();
        let all = dump.join("\n");

        assert!(all.contains("Unique Value Props"),
            "missing 💡 line after code fence:\n{all}");
    }

    #[test]
    fn incremental_code_fence_streaming() {
        // Simulate streaming: tokens arrive one by one, render called each time
        let mut tb = TextBlock::new();
        let tokens = [
            "**Phase 2:**\n",
            "```\n",
            "- Open source\n",
            "```\n",
            "\n",
            "---\n",
            "\n",
            "### 💡 **Unique Value Props:**\n",
            "\n",
            "Answer here.\n",
        ];

        for (i, tok) in tokens.iter().enumerate() {
            tb.stream.feed(tok);
            let r = render_text_incremental(&mut tb, 80);
            let text: String = r.iter()
                .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
                .collect();
            // After token 7 ("### 💡 ..."), should contain it
            if i >= 7 {
                assert!(text.contains("Unique Value Props"),
                    "after token {i}, missing Unique Value Props. rendered:\n{}",
                    r.iter().enumerate().map(|(j, l)| {
                        let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
                        format!("{j}: {t}")
                    }).collect::<Vec<_>>().join("\n"));
            }
        }
    }

    #[test]
    fn emoji_bold_line_renders() {
        let mut tb = TextBlock::new();
        tb.stream.feed("### Strategy\n\n");
        tb.stream.feed("1. First point\n");
        tb.stream.feed("2. 💡 **Unique Value Props (cần define rõ):**\n");
        tb.stream.feed("3. Third point\n");
        tb.stream.flush();

        let full = render_text_markdown(&tb, 80);
        let incr = render_text_incremental(&mut tb, 80);

        let full_text: String = full.iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>().join("\n");
        let incr_text: String = incr.iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>().join("\n");

        assert!(full_text.contains("Unique Value Props"), "full missing:\n{full_text}");
        assert!(incr_text.contains("Unique Value Props"), "incr missing:\n{incr_text}");
        assert!(full_text.contains("💡"), "full missing emoji:\n{full_text}");
        assert_eq!(full.len(), incr.len(), "full={} incr={}\nFULL:\n{full_text}\nINCR:\n{incr_text}",
            full.len(), incr.len());
    }

    #[test]
    fn incremental_matches_full() {
        let mut tb = TextBlock::new();
        tb.stream.feed("# Hello\nparagraph\n- item\n");
        tb.stream.flush();

        let full = render_text_markdown(&tb, 80);
        let incr = render_text_incremental(&mut tb, 80);
        assert_eq!(full.len(), incr.len());
        for (f, i) in full.iter().zip(incr.iter()) {
            let ft: String = f.spans.iter().map(|s| s.text.as_str()).collect();
            let it: String = i.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(ft, it);
        }
    }

    #[test]
    fn incremental_appends_only_new() {
        let mut tb = TextBlock::new();
        tb.stream.feed("line1\nline2\n");

        // First render parses all
        let _r1 = render_text_incremental(&mut tb, 80);
        assert_eq!(tb.cache.parsed_count, 2);
        let cached_len = tb.cache.lines.len();

        // Add more
        tb.stream.feed("line3\n");
        let r2 = render_text_incremental(&mut tb, 80);
        assert_eq!(tb.cache.parsed_count, 3);
        assert!(tb.cache.lines.len() > cached_len);

        // Full render should match
        let full = render_text_markdown(&tb, 80);
        assert_eq!(r2.len(), full.len());
    }

    #[test]
    fn incremental_width_change_invalidates() {
        let mut tb = TextBlock::new();
        tb.stream.feed("hello world\n");
        let _ = render_text_incremental(&mut tb, 80);
        assert_eq!(tb.cache.parsed_count, 1);

        let _ = render_text_incremental(&mut tb, 40);
        // Width changed, should have re-parsed from scratch
        assert_eq!(tb.cache.width, 40);
    }

    #[test]
    fn incremental_code_fence() {
        let mut tb = TextBlock::new();
        tb.stream.feed("```rust\nlet x = 1;\n```\nhello\n");
        tb.stream.flush();

        let full = render_text_markdown(&tb, 80);
        let incr = render_text_incremental(&mut tb, 80);
        assert_eq!(full.len(), incr.len());
    }

    #[test]
    fn incremental_with_partial() {
        let mut tb = TextBlock::new();
        tb.stream.feed("done\npartial text");
        // committed=["done"], partial="partial text"

        let r = render_text_incremental(&mut tb, 80);
        let texts: Vec<String> = r.iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect())
            .collect();
        assert!(texts.iter().any(|t| t.contains("done")));
        assert!(texts.iter().any(|t| t.contains("partial text")));
    }
}
