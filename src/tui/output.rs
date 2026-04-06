/// OutputLog facade — public API for the app to push content and query visible lines.
///
/// Two-phase architecture:
/// - **Event phase**: mutate state + mark dirty. NEVER call ensure_fresh.
/// - **Render phase**: `prepare_frame()` calls ensure_fresh ONCE, then auto_scroll + clamp.
///
/// All scroll/query methods use `cached_total` (updated by prepare_frame).
/// This avoids O(n²) re-renders when pushing many blocks (session load)
/// and eliminates ensure_fresh from scroll event handlers.
use crate::tui::block::{render_block, Block, SkillBlock, TextBlock, ToolBlock};
use crate::tui::stream::StreamBuf;
use crate::tui::scroll::ScrollView;
use crate::tui::text::Line;
use crate::tui::theme::icon;
use crate::tui::viewport::Viewport;

const TOOL_PREVIEW_LINES: usize = 4;

/// The main output model — owns blocks, scroll state, and render cache.
pub struct OutputLog {
    blocks: Vec<Block>,
    scroll: ScrollView,
    cache: Viewport,
    width: usize,
    height: usize,
    spinner_frame: usize,
    has_logo: bool,
    /// Total rendered lines from last prepare_frame(). Used by scroll methods
    /// so they never need ensure_fresh. Stale by at most 1 frame — invisible
    /// to the user since render hasn't happened yet.
    cached_total: usize,
}

impl OutputLog {
    /// Create with viewport dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            blocks: Vec::new(),
            scroll: ScrollView::new(),
            cache: Viewport::new(),
            width,
            height,
            spinner_frame: 0,
            has_logo: false,
            cached_total: 0,
        }
    }

    // ════════════════════════════════════════════════════════════════
    // RENDER PHASE — called once per frame from App::render()
    // ════════════════════════════════════════════════════════════════

    /// Reconcile all state for this frame. Must be called exactly once
    /// at the start of each render, before any visible_lines/scroll_info.
    pub fn prepare_frame(&mut self) {
        self.cache
            .refresh(&mut self.blocks, self.width, self.spinner_frame);
        self.cached_total = self.cache.total_lines();
        self.scroll.auto_scroll(self.cached_total, self.height);
        self.scroll.clamp(self.cached_total, self.height);
    }

    /// Get the visible lines for the current scroll position.
    /// prepare_frame() must have been called this frame.
    pub fn visible_lines(&mut self) -> &[Line] {
        self.cache.visible(self.scroll.offset, self.height)
    }

    /// Zero-copy iterator over visible lines — for direct Renderer painting.
    /// prepare_frame() must have been called this frame.
    pub fn visible_iter(&mut self) -> crate::tui::viewport::ViewportIter<'_> {
        self.cache.window_iter(self.scroll.offset, self.height)
    }

    // ════════════════════════════════════════════════════════════════
    // EVENT PHASE — cheap state mutation only, never ensure_fresh
    // ════════════════════════════════════════════════════════════════

    /// Advance spinner, mark active tool block dirty.
    pub fn tick(&mut self) {
        self.spinner_frame = (self.spinner_frame + 1) % icon::SPINNER.len();
        if let Some(Block::Tool(tb)) = self.blocks.last()
            && !tb.is_done
        {
            self.cache.mark_dirty(self.blocks.len() - 1);
        }
    }

    /// Update viewport dimensions (e.g. on resize).
    pub fn set_size(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cache.mark_all_dirty();
    }

    // ── Content mutation ──

    /// Add a user message block. Always 1 gap before + 1 gap after.
    pub fn user_message(&mut self, text: &str) {
        self.commit_last();
        if self.has_logo {
            self.clear();
            self.has_logo = false;
        }
        if !matches!(self.blocks.last(), Some(Block::Gap)) {
            self.push(Block::Gap);
        }
        self.push(Block::User(
            text.split('\n').map(|s| s.to_owned()).collect(),
        ));
        self.push(Block::Gap);
    }

    /// Append a thinking token.
    pub fn append_thinking(&mut self, token: &str) {
        if !matches!(self.blocks.last(), Some(Block::Thinking(_))) {
            self.commit_last();
            self.push(Block::Thinking(StreamBuf::new()));
        }
        self.feed_last(token);
    }

    /// Append an assistant text token.
    pub fn append_token(&mut self, token: &str) {
        if matches!(self.blocks.last(), Some(Block::Text(_))) {
            self.feed_last(token);
            return;
        }
        self.commit_last();
        if matches!(self.blocks.last(), Some(Block::Tool(_) | Block::Skill(_))) {
            self.push(Block::Gap);
        }
        self.push(Block::Text(TextBlock::new()));
        self.feed_last(token);
    }

    /// Add an info message.
    pub fn info(&mut self, text: &str) {
        self.commit_last();
        self.push(Block::Info(text.to_owned()));
    }

    /// Add a completed assistant text block (for history replay).
    pub fn assistant_message(&mut self, text: &str) {
        self.commit_last();
        let trimmed = text.trim_start_matches('\n');
        if trimmed.is_empty() { return; }
        if matches!(self.blocks.last(), Some(Block::Tool(_) | Block::Skill(_))) {
            self.push(Block::Gap);
        }
        let mut tb = TextBlock::new();
        tb.feed(trimmed);
        tb.flush();
        self.push(Block::Text(tb));
    }

    /// Add a completed tool block (for history replay, collapsed).
    pub fn tool_history(&mut self, name: &str, summary: &str) {
        self.commit_last();
        if matches!(self.blocks.last(), Some(Block::Text(_) | Block::Skill(_))) {
            self.push(Block::Gap);
        }
        self.push(Block::Tool(ToolBlock {
            name: name.to_owned(),
            summary: summary.to_owned(),
            output: Vec::new(),
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: false,
        }));
    }

    /// Add a success message.
    #[allow(dead_code)]
    pub fn success(&mut self, text: &str) {
        self.commit_last();
        self.push(Block::Success(text.to_owned()));
    }

    /// Add an error message.
    pub fn error(&mut self, text: &str) {
        self.commit_last();
        self.push(Block::Error(text.to_owned()));
    }

    /// Add a warning message.
    pub fn warn(&mut self, text: &str) {
        self.commit_last();
        self.push(Block::Warn(text.to_owned()));
    }

    /// Add centered logo lines.
    pub fn logo(&mut self, lines: &[&str]) {
        self.commit_last();
        let owned: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let max_w = owned
            .iter()
            .map(|l| crate::tui::text::display_width(l))
            .max()
            .unwrap_or(0);
        self.push(Block::Logo(owned, max_w));
        self.has_logo = true;
    }

    /// Add a gap divider.
    pub fn divider(&mut self) {
        self.commit_last();
        self.push(Block::Gap);
    }

    /// Add a divider with a label (e.g. duration).
    pub fn divider_with_label(&mut self, label: &str) {
        self.commit_last();
        self.push(Block::Gap);
        self.push(Block::GapLabel(label.to_owned()));
    }

    /// Start a tool invocation block (or update summary if already active).
    pub fn tool_start(&mut self, name: &str, summary: &str) {
        if let Some((idx, Block::Tool(tb))) = self.blocks.iter_mut().enumerate().rev().find(|(_, b)| {
            matches!(b, Block::Tool(tb) if tb.name == name && !tb.is_done)
        }) {
            if !summary.is_empty() {
                tb.summary = summary.to_owned();
                tb.stream = Some(StreamBuf::new());
                self.cache.mark_dirty(idx);
            }
            return;
        }
        self.commit_last();
        if matches!(self.blocks.last(), Some(Block::Text(_) | Block::Skill(_))) {
            self.push(Block::Gap);
        }
        self.push(Block::Tool(ToolBlock {
            name: name.to_owned(),
            summary: summary.to_owned(),
            output: Vec::new(),
            stream: Some(StreamBuf::new()),
            is_done: false,
            end_summary: String::new(),
            is_expanded: false,
        }));
    }

    /// Append streaming tool input preview.
    pub fn tool_input(&mut self, name: &str, chunk: &str) {
        if !self.blocks.iter().rev().any(|b| {
            matches!(b, Block::Tool(tb) if tb.name == name && !tb.is_done)
        }) {
            self.tool_start(name, "");
        }
        let found = self.blocks.iter().enumerate().rev().find_map(|(i, b)| {
            if let Block::Tool(tb) = b
                && tb.name == name
                && !tb.is_done
            {
                Some(i)
            } else {
                None
            }
        });
        if let Some(idx) = found
            && let Block::Tool(tb) = &mut self.blocks[idx]
        {
            if let Some(stream) = &mut tb.stream {
                stream.feed(chunk);
            }
            self.cache.mark_dirty(idx);
        }
    }

    /// Append streaming tool output.
    pub fn tool_output(&mut self, name: &str, chunk: &str) {
        let found = self.blocks.iter().enumerate().rev().find_map(|(i, b)| {
            if let Block::Tool(tb) = b
                && tb.name == name
                && !tb.is_done
            {
                Some(i)
            } else {
                None
            }
        });
        let Some(idx) = found else { return };
        if let Block::Tool(tb) = &mut self.blocks[idx] {
            if let Some(stream) = &mut tb.stream {
                stream.feed(chunk);
                for line in stream.committed.drain(..) {
                    tb.output.push(strip_ansi(&line));
                }
            }
            self.cache.mark_dirty(idx);
        }
    }

    /// Finish a tool invocation.
    pub fn tool_end(&mut self, name: &str, summary: &str) {
        self.commit_last();
        let found = self.blocks.iter().enumerate().rev().find_map(|(i, b)| {
            if let Block::Tool(tb) = b
                && tb.name == name
                && !tb.is_done
            {
                Some(i)
            } else {
                None
            }
        });
        if let Some(idx) = found
            && let Block::Tool(tb) = &mut self.blocks[idx]
        {
            tb.is_done = true;
            tb.end_summary = summary.to_owned();
            tb.stream = None;
            self.cache.mark_dirty(idx);
            // clamp deferred to prepare_frame
        }
    }

    /// Start a skill activation block.
    pub fn skill_start(&mut self, name: &str) {
        self.commit_last();
        if matches!(self.blocks.last(), Some(Block::Text(_) | Block::Tool(_))) {
            self.push(Block::Gap);
        }
        self.push(Block::Skill(SkillBlock {
            name: name.to_owned(),
            is_done: false,
            end_summary: String::new(),
        }));
    }

    /// Finish a skill activation.
    pub fn skill_end(&mut self, summary: &str) {
        let idx = self.blocks.len().wrapping_sub(1);
        self.commit_last();
        if let Some(Block::Skill(sb)) = self.blocks.last_mut() {
            if sb.is_done {
                return;
            }
            sb.is_done = true;
            sb.end_summary = summary.to_owned();
            self.cache.mark_dirty(idx);
        }
    }

    /// Finalize any in-progress blocks.
    pub fn abort(&mut self) {
        self.commit_last();
        let idx = self.blocks.len().wrapping_sub(1);
        match self.blocks.last_mut() {
            Some(Block::Tool(tb)) if !tb.is_done => {
                tb.is_done = true;
                tb.stream = None;
                self.cache.mark_dirty(idx);
            }
            Some(Block::Skill(sb)) if !sb.is_done => {
                sb.is_done = true;
                self.cache.mark_dirty(idx);
            }
            _ => {}
        }
        // clamp deferred to prepare_frame
    }

    /// Flush partial streaming data.
    pub fn newline(&mut self) {
        self.commit_last();
    }

    /// Clear all content.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.scroll.reset();
        self.cache.clear();
        self.cached_total = 0;
    }

    // ── Scroll (event phase — uses cached_total) ──

    /// Scroll up by n lines. Bounce-tolerant for trackpad inertia.
    pub fn scroll_up(&mut self, n: usize) {
        let max = self.cached_total.saturating_sub(self.height);
        self.scroll.up(n, max, 3);
    }

    /// Scroll down by n lines.
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.cached_total.saturating_sub(self.height);
        self.scroll.down(n, max);
    }

    /// Jump to a scroll offset (for scrollbar drag).
    pub fn scroll_to(&mut self, offset: usize) {
        let max = self.cached_total.saturating_sub(self.height);
        self.scroll.set_offset(offset, max);
    }

    /// Total lines, visible height, and current scroll offset.
    pub fn scroll_info(&self) -> (usize, usize, usize) {
        (self.cached_total, self.height, self.scroll.offset)
    }

    // ── Hit testing (event phase — uses cached offsets) ──

    /// Find which block index is at a screen row.
    pub fn hit_test_block(&self, screen_row: usize, region_row: usize) -> Option<usize> {
        let abs = self.scroll.offset + screen_row.saturating_sub(region_row);
        self.cache.hit_test(abs)
    }

    /// Toggle expand/collapse on a tool block. Returns true if toggled.
    pub fn toggle_expand(&mut self, block_idx: usize) -> bool {
        if let Some(Block::Tool(tb)) = self.blocks.get_mut(block_idx) {
            if !tb.is_done || tb.output.len() <= TOOL_PREVIEW_LINES {
                return false;
            }
            tb.is_expanded = !tb.is_expanded;
            self.cache.mark_dirty(block_idx);
            // clamp deferred to prepare_frame
            return true;
        }
        false
    }

    /// Get text at a screen row (for copy selection).
    #[allow(dead_code)]
    pub fn text_at_row(&self, screen_row: usize, region_row: usize) -> String {
        let abs = self.scroll.offset + screen_row.saturating_sub(region_row);
        self.cache.text_at(abs)
    }

    // ════════════════════════════════════════════════════════════════
    // PRIVATE — internal helpers
    // ════════════════════════════════════════════════════════════════

    fn push(&mut self, block: Block) {
        let rendered = render_block(&block, self.width, self.spinner_frame);
        self.blocks.push(block);
        self.cache.push(rendered);
        // No auto_scroll / ensure_fresh here.
        // prepare_frame() handles scroll reconciliation once per frame.
        // This keeps batch pushes (session load) O(n) instead of O(n²).
    }

    fn feed_last(&mut self, token: &str) {
        let idx = self.blocks.len().wrapping_sub(1);
        match self.blocks.last_mut() {
            Some(Block::Thinking(s)) => s.feed(token),
            Some(Block::Text(tb)) => tb.feed(token),
            _ => return,
        }
        self.cache.mark_dirty(idx);
    }

    fn commit_last(&mut self) {
        let idx = self.blocks.len().wrapping_sub(1);
        match self.blocks.last_mut() {
            Some(Block::Thinking(s)) => {
                if !s.is_empty() {
                    s.flush();
                    self.cache.mark_dirty(idx);
                }
            }
            Some(Block::Text(tb)) => {
                if !tb.is_empty() {
                    tb.flush();
                    self.cache.mark_dirty(idx);
                }
            }
            Some(Block::Tool(tb)) if !tb.is_done => {
                if let Some(stream) = &mut tb.stream {
                    stream.flush();
                    tb.output.append(&mut stream.committed);
                    tb.stream = None;
                }
                self.cache.mark_dirty(idx);
            }
            _ => {}
        }
    }
}

/// Strip ANSI escape sequences (CSI and OSC) from a string.
fn strip_ansi(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b && i + 1 < b.len() {
            if b[i + 1] == b'[' {
                i += 2;
                while i < b.len() && !b[i].is_ascii_alphabetic() { i += 1; }
                if i < b.len() { i += 1; }
            } else if b[i + 1] == b']' {
                i += 2;
                while i < b.len() {
                    if b[i] == 0x07 { i += 1; break; }
                    if b[i] == 0x1b && i + 1 < b.len() && b[i + 1] == b'\\' {
                        i += 2; break;
                    }
                    i += 1;
                }
            } else {
                i += 2;
            }
        } else {
            let start = i;
            i += 1;
            while i < b.len() && b[i] & 0xC0 == 0x80 { i += 1; }
            out.push_str(&s[start..i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: prepare_frame before any query in tests.
    fn pf(log: &mut OutputLog) { log.prepare_frame(); }

    #[test]
    fn basic_content() {
        let mut log = OutputLog::new(80, 10);
        log.info("hello");
        log.divider();
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert!(total >= 2);
    }

    #[test]
    fn tool_expand_collapse() {
        let mut log = OutputLog::new(80, 10);
        log.tool_start("bash", "$ ls");
        for i in 0..20 {
            log.tool_output("bash", &format!("line {i}\n"));
        }
        log.tool_end("bash", "");

        pf(&mut log);
        let (before, _, _) = log.scroll_info();
        let mut tool_idx = None;
        for i in 0..log.blocks.len() {
            if matches!(&log.blocks[i], Block::Tool(_)) {
                tool_idx = Some(i);
                break;
            }
        }
        let ti = tool_idx.unwrap();
        assert!(log.toggle_expand(ti));
        pf(&mut log);
        let (after, _, _) = log.scroll_info();
        assert!(after > before);

        assert!(log.toggle_expand(ti));
        pf(&mut log);
        let (collapsed, _, _) = log.scroll_info();
        assert_eq!(collapsed, before);
    }

    #[test]
    fn user_scrolled_prevents_auto_scroll() {
        let mut log = OutputLog::new(80, 5);
        for i in 0..20 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(5);
        let (_, _, offset_before) = log.scroll_info();
        log.info("new content");
        pf(&mut log);
        let (_, _, offset_after) = log.scroll_info();
        assert_eq!(offset_before, offset_after);
    }

    #[test]
    fn scroll_to_bottom_resumes_auto() {
        let mut log = OutputLog::new(80, 5);
        for i in 0..20 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(5);
        log.scroll_down(999);
        log.info("new");
        pf(&mut log);
        let vis = log.visible_lines();
        assert!(!vis.is_empty());
    }

    #[test]
    fn streaming_tokens() {
        let mut log = OutputLog::new(80, 10);
        log.append_token("hello ");
        log.append_token("world\n");
        log.append_token("line2");
        log.newline();
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert!(total > 0);
    }

    #[test]
    fn streaming_visible_content() {
        let mut log = OutputLog::new(80, 20);
        log.append_token("hello ");
        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let text: String = vis.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("hello"), "expected 'hello' in visible, got: {text:?}");

        log.append_token("world\n");
        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let text: String = vis.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("hello"), "after newline: {text:?}");

        log.append_token("line2");
        log.newline();
        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let text: String = vis.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("line2"), "after flush: {text:?}");
    }

    #[test]
    fn full_conversation_flow() {
        let mut log = OutputLog::new(80, 30);
        log.divider();
        log.info("luma");
        log.divider();

        log.user_message("hello");
        log.append_token("Hi ");
        log.append_token("there!\n");
        log.append_token("How can I help?");
        log.newline();
        log.divider_with_label("1.0s");

        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().map(|l| {
            l.spans.iter().map(|s| s.text.as_str()).collect::<String>()
        }).collect();
        let all = dump.join("\n");

        assert!(all.contains("hello"), "user msg missing:\n{all}");
        assert!(all.contains("Hi there!"), "assistant msg missing:\n{all}");
        assert!(all.contains("How can I help?"), "assistant line2 missing:\n{all}");
    }

    #[test]
    fn full_conversation_flow_iter() {
        let mut log = OutputLog::new(80, 30);
        log.divider();
        log.info("luma");
        log.divider();

        log.user_message("hello");
        log.append_token("Hi ");
        log.append_token("there!\n");
        log.append_token("How can I help?");
        log.newline();
        log.divider_with_label("1.0s");

        pf(&mut log);
        let texts: Vec<String> = log.visible_iter().map(|l| {
            l.spans.iter().map(|s| s.text.as_str()).collect::<String>()
        }).collect();
        let all = texts.join("\n");

        assert!(all.contains("hello"), "iter: user msg missing:\n{all}");
        assert!(all.contains("Hi there!"), "iter: assistant msg missing:\n{all}");
    }

    #[test]
    fn assistant_message_via_iter() {
        let mut log = OutputLog::new(80, 20);
        log.assistant_message("Dự án này rất hay!\n\nĐiểm nổi bật:\n- Fast\n- Clean");
        pf(&mut log);
        let texts: Vec<String> = log.visible_iter().map(|l| {
            l.spans.iter().map(|s| s.text.as_str()).collect::<String>()
        }).collect();
        let all = texts.join("|");
        assert!(all.contains("hay"), "missing content: {all}");
        assert!(all.contains("Fast"), "missing list: {all}");
    }

    #[test]
    fn thinking_to_text_transition() {
        let mut log = OutputLog::new(80, 10);
        log.append_thinking("hmm");
        log.append_token("done");
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert!(total > 0);
    }

    #[test]
    fn thinking_spacing() {
        let mut log = OutputLog::new(80, 40);
        log.user_message("hi");
        log.append_thinking("let me think\n");
        log.append_thinking("about this\n");
        log.append_token("Here is my answer.\n");
        log.append_token("Second line.");
        log.newline();
        log.divider_with_label("1.0s");

        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().enumerate().map(|(i, l)| {
            let w = l.visible_width();
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: [w={w:2}] {t}")
        }).collect();

        let mut consec = 0;
        for (i, l) in vis.iter().enumerate() {
            if l.visible_width() == 0 {
                consec += 1;
                assert!(consec < 3,
                    "3+ consecutive empty lines at {i}:\n{}", dump.join("\n"));
            } else {
                consec = 0;
            }
        }

        let all: String = dump.join("\n");
        assert!(all.contains("Thinking:"), "missing thinking:\n{all}");
        assert!(all.contains("Here is my answer"), "missing answer:\n{all}");
    }

    #[test]
    fn thinking_wrap_no_extra_indent() {
        let mut log = OutputLog::new(30, 40);
        log.append_thinking("short first line\n");
        log.append_thinking(&"x".repeat(50));
        log.newline();

        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().enumerate().map(|(i, l)| {
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: [{:2}] |{t}|", l.visible_width())
        }).collect();

        let long_lines: Vec<&str> = vis.iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .leak()
            .iter()
            .map(|s| s.as_str())
            .filter(|t| t.starts_with("xx"))
            .collect();
        for t in &long_lines {
            assert!(!t.starts_with("  "), "wrapped line has unwanted indent:\n{}", dump.join("\n"));
        }
    }

    #[test]
    fn thinking_blank_lines_no_excess() {
        let mut log = OutputLog::new(80, 40);
        log.append_thinking("line1\n\nline3\n");
        log.append_token("answer");
        log.newline();

        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let mut consec = 0;
        for (i, l) in vis.iter().enumerate() {
            if l.visible_width() == 0 {
                consec += 1;
                if consec >= 3 {
                    let dump: Vec<String> = vis.iter().enumerate().map(|(j, ll)| {
                        let t: String = ll.spans.iter().map(|s| s.text.as_str()).collect();
                        format!("{j:2}: [w={:2}] {t}", ll.visible_width())
                    }).collect();
                    panic!("3+ consecutive empty at line {i}:\n{}", dump.join("\n"));
                }
            } else {
                consec = 0;
            }
        }
    }

    #[test]
    fn clear_resets_everything() {
        let mut log = OutputLog::new(80, 10);
        log.info("hello");
        log.clear();
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert_eq!(total, 0);
    }

    #[test]
    fn assistant_message_renders() {
        let mut log = OutputLog::new(80, 10);
        log.assistant_message("hello\nworld");
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert!(total >= 2);
    }

    #[test]
    fn tool_history_renders_collapsed() {
        let mut log = OutputLog::new(80, 10);
        log.tool_history("bash", "$ ls");
        pf(&mut log);
        let (total, _, _) = log.scroll_info();
        assert!(total >= 1);
        assert!(!log.toggle_expand(0));
    }

    #[test]
    fn no_triple_empty_lines_in_markdown() {
        let mut log = OutputLog::new(80, 500);
        log.assistant_message("# Title\n\nParagraph one.\n\n## Section\n\n1. **Item one** — detail\n   - sub-item\n\n2. **Item two** — detail\n\n### Sub-section\n\nFinal paragraph.");
        pf(&mut log);
        let vis = log.visible_lines().to_vec();
        let mut consec = 0;
        for (i, l) in vis.iter().enumerate() {
            if l.visible_width() == 0 {
                consec += 1;
                assert!(consec < 3, "3+ consecutive empty at line {i}");
            } else {
                consec = 0;
            }
        }
    }

    #[test]
    fn no_consecutive_empty_lines_streaming() {
        let mut log = OutputLog::new(80, 500);
        log.divider();
        log.info("luma");
        log.divider();

        log.user_message("find skills");
        log.append_thinking("Let me check");
        log.append_token("I'll look for skills!\n");
        log.tool_start("bash", "$ npx skills find");
        log.tool_output("bash", "found 3 skills\n");
        log.tool_end("bash", "");
        log.append_token("Here are the results.\n\nDone!");
        log.newline();
        log.divider_with_label("2.0s");

        log.user_message("ok thanks");
        log.append_token("You're welcome!\n\n## Summary\n\nAll good.");
        log.newline();
        log.divider_with_label("1.0s");

        pf(&mut log);
        let vis = log.visible_lines();
        for i in 1..vis.len() {
            if vis[i].visible_width() == 0 && vis[i - 1].visible_width() == 0 {
                let ctx: Vec<String> = vis[i.saturating_sub(3)..(i+2).min(vis.len())]
                    .iter().map(|l| {
                        let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
                        format!("[w={}] {t}", l.visible_width())
                    }).collect();
                panic!("consecutive empty at lines [{},{}]:\n{}", i-1, i, ctx.join("\n"));
            }
        }
    }

    #[test]
    fn user_block_spacing_consistent() {
        let mut log = OutputLog::new(80, 100);

        log.divider();
        log.info("luma");
        log.divider();

        log.user_message("hello");
        log.append_token("response1");
        log.newline();
        log.divider_with_label("1.0s");

        log.user_message("world");
        log.append_token("response2");
        log.newline();
        log.divider_with_label("0.5s");

        let user_indices: Vec<usize> = log.blocks.iter().enumerate()
            .filter(|(_, b)| matches!(b, Block::User(_)))
            .map(|(i, _)| i)
            .collect();

        assert_eq!(user_indices.len(), 2);
        for &ui in &user_indices {
            assert!(ui > 0);
            assert!(matches!(&log.blocks[ui - 1], Block::Gap),
                "block before User[{ui}] is not Gap: {:?}",
                std::mem::discriminant(&log.blocks[ui - 1]));
            assert!(ui + 1 < log.blocks.len());
            assert!(matches!(&log.blocks[ui + 1], Block::Gap),
                "block after User[{ui}] is not Gap: {:?}",
                std::mem::discriminant(&log.blocks[ui + 1]));
        }
    }

    /// Scroll down to bottom DURING streaming should resume auto-scroll.
    #[test]
    fn scroll_to_bottom_during_stream_resumes_auto() {
        let mut log = OutputLog::new(80, 5);
        for i in 0..20 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(10);

        for i in 0..10 {
            log.append_token(&format!("stream{i} "));
        }

        log.scroll_down(999);

        for i in 10..30 {
            log.append_token(&format!("stream{i} "));
        }
        log.append_token("\n");

        pf(&mut log);
        let (total, h, off) = log.scroll_info();
        let max = total.saturating_sub(h);
        assert_eq!(off, max,
            "auto-scroll not resumed: total={total} h={h} off={off} max={max}");
    }

    /// User scrolls down step-by-step while content grows.
    #[test]
    fn scroll_down_stepwise_during_fast_stream_resumes_auto() {
        let mut log = OutputLog::new(80, 10);
        for i in 0..30 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(15);

        for step in 0..20 {
            log.append_token(&format!("new{step}\n"));
            pf(&mut log); // simulate tick/render
            log.scroll_down(3);
        }

        pf(&mut log);
        let (total, h, off) = log.scroll_info();
        let max = total.saturating_sub(h);
        assert_eq!(off, max,
            "auto-scroll should resume: total={total} h={h} off={off} max={max}");
    }

    /// After stream ends, scroll to bottom resumes auto-scroll.
    #[test]
    fn scroll_to_bottom_after_stream_ends_resumes_auto() {
        let mut log = OutputLog::new(80, 10);
        for i in 0..30 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(20);
        log.newline();

        for _ in 0..50 {
            log.scroll_down(3);
        }

        log.info("next turn content");
        pf(&mut log);
        let (total, h, off) = log.scroll_info();
        let max = total.saturating_sub(h);
        assert_eq!(off, max,
            "auto-scroll not resumed: total={total} h={h} off={off} max={max}");
    }

    /// Trackpad bounce at bottom should not break auto-scroll.
    #[test]
    fn trackpad_bounce_at_bottom_keeps_auto_scroll() {
        let mut log = OutputLog::new(80, 10);
        for i in 0..30 {
            log.info(&format!("line {i}"));
        }
        pf(&mut log);
        log.scroll_up(20);
        for _ in 0..30 {
            log.scroll_down(3);
        }

        pf(&mut log);
        let (total, h, off) = log.scroll_info();
        let max = total.saturating_sub(h);
        assert_eq!(off, max, "should be at bottom");

        // Trackpad bounce
        log.scroll_up(3);

        log.info("new content after bounce");
        pf(&mut log);
        let (total2, h2, off2) = log.scroll_info();
        let max2 = total2.saturating_sub(h2);
        assert_eq!(off2, max2,
            "trackpad bounce broke auto-scroll: total={total2} h={h2} off={off2} max={max2}");
    }

    /// Wire name "Write"/"Edit" (Anthropic) must be treated as write tools.
    #[test]
    fn wire_name_write_has_diff_colors() {
        use crate::tui::theme::palette;
        for wire_name in &["Write", "Edit"] {
            let mut log = OutputLog::new(80, 50);
            log.tool_start(wire_name, "src/main.rs");
            log.tool_output(wire_name, "  1 + fn main() {\n");
            log.tool_output(wire_name, "  2 +     println!(\"hello\");\n");
            log.tool_end(wire_name, "");
            pf(&mut log);

            let vis = log.visible_lines().to_vec();
            let fn_line = vis.iter().find(|l| {
                l.spans.iter().any(|s| s.text.contains("fn"))
            }).expect(&format!("{wire_name}: should find diff line with 'fn'"));
            assert!(fn_line.spans.iter().any(|s| s.bg == Some(palette::DIFF_ADD_BG)),
                "{wire_name}: missing DIFF_ADD_BG: {:?}",
                fn_line.spans.iter().map(|s| (&s.text, s.bg)).collect::<Vec<_>>());
        }
    }

    /// Full pipeline: tool_input (Claude-style) → tool_start → tool_output → tool_end.
    /// Verifies visible_lines() carry diff bg and syntax highlight colors.
    #[test]
    fn tool_full_pipeline_claude_visible_colors() {
        let mut log = OutputLog::new(80, 50);

        // Phase 1: Claude streams tool input (content preview)
        log.tool_input("write", "fn ");
        log.tool_input("write", "main() {\n");
        log.tool_input("write", "    println!(\"hello\");\n");
        log.tool_input("write", "}");

        // Phase 2: ToolStart resets stream
        log.tool_start("write", "src/main.rs (3 lines)");

        // Phase 3: Tool execution sends diff
        log.tool_output("write", "  1 + fn main() {\n");
        log.tool_output("write", "  2 +     println!(\"hello\");\n");
        log.tool_output("write", "  3 + }\n");

        // Phase 4: Tool done
        log.tool_end("write", "");

        // Phase 5: Render
        pf(&mut log);

        // Check tb.output
        let tb = log.blocks.iter().find_map(|b| {
            if let Block::Tool(tb) = b { Some(tb) } else { None }
        }).expect("tool block exists");
        assert!(tb.is_done);
        assert_eq!(tb.output.len(), 3,
            "expected 3 diff lines, got {}: {:?}", tb.output.len(), tb.output);

        // Check visible_lines has colors
        let vis = log.visible_lines().to_vec();
        assert!(vis.len() >= 4, "visible lines: {}", vis.len());

        // Find a line containing "fn" — should have DIFF_ADD_BG
        use crate::tui::theme::palette;
        let fn_line = vis.iter().find(|l| {
            l.spans.iter().any(|s| s.text.contains("fn"))
        }).expect("should find line with 'fn'");

        assert!(fn_line.spans.iter().any(|s| s.bg == Some(palette::DIFF_ADD_BG)),
            "missing DIFF_ADD_BG on fn line: {:?}",
            fn_line.spans.iter().map(|s| (&s.text, s.fg, s.bg)).collect::<Vec<_>>());

        // Check syntax highlight: 'fn' keyword should have KEYWORD color, not DIM
        let fn_span = fn_line.spans.iter().find(|s| s.text == "fn");
        if let Some(span) = fn_span {
            assert_ne!(span.fg, palette::DIM,
                "'fn' should be syntax-highlighted, not DIM: {:?}", span.fg);
        }
    }


}
