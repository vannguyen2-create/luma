/// OutputLog facade — public API for the app to push content and query visible lines.
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
        }
    }

    /// Advance spinner and re-render active tool block.
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
        // Clear splash logo on first interaction
        if self.has_logo {
            self.clear();
            self.has_logo = false;
        }
        // Ensure exactly 1 gap before user block
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
        // Gap before text when preceded by tool/skill (visual separation)
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
        for line in lines {
            self.push(Block::Logo(line.to_string()));
        }
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
        // If block already exists (e.g. early ToolStart from SSE), just update summary
        if let Some((idx, Block::Tool(tb))) = self.blocks.iter_mut().enumerate().rev().find(|(_, b)| {
            matches!(b, Block::Tool(tb) if tb.name == name && !tb.is_done)
        }) {
            if !summary.is_empty() {
                tb.summary = summary.to_owned();
                // Clear input preview — real tool output will follow
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

    /// Append streaming tool input preview (content being written/edited).
    pub fn tool_input(&mut self, name: &str, chunk: &str) {
        // Create block if not yet started
        if !self.blocks.iter().rev().any(|b| {
            matches!(b, Block::Tool(tb) if tb.name == name && !tb.is_done)
        }) {
            self.tool_start(name, "");
        }
        // Feed to stream for live preview
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
            self.auto_scroll();
        }
    }

    /// Append streaming tool output (finds matching active tool by name).
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
            self.auto_scroll();
        }
    }

    /// Finish a tool invocation (searches backwards for matching name).
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
            self.clamp_scroll();
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

    /// Finalize any in-progress blocks (tool spinner, thinking, text stream).
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
        self.clamp_scroll();
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
    }

    // ── Scroll ──

    /// Scroll up by n lines.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll.up(n);
    }

    /// Scroll down by n lines.
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.max_scroll();
        self.scroll.down(n, max);
    }

    /// Jump to a scroll offset (for scrollbar drag).
    pub fn scroll_to(&mut self, offset: usize) {
        let max = self.max_scroll();
        self.scroll.set_offset(offset, max);
    }

    /// Total lines, visible height, and current scroll offset.
    pub fn scroll_info(&mut self) -> (usize, usize, usize) {
        self.ensure_fresh();
        (self.cache.total_lines(), self.height, self.scroll.offset)
    }

    // ── Hit testing ──

    /// Find which block index is at a screen row.
    pub fn hit_test_block(&mut self, screen_row: usize, region_row: usize) -> Option<usize> {
        self.ensure_fresh();
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
            self.clamp_scroll();
            return true;
        }
        false
    }

    /// Get text at a screen row (for copy selection).
    #[allow(dead_code)]
    pub fn text_at_row(&mut self, screen_row: usize, region_row: usize) -> String {
        self.ensure_fresh();
        let abs = self.scroll.offset + screen_row.saturating_sub(region_row);
        self.cache.text_at(abs)
    }

    // ── Render ──

    /// Get the visible lines for the current scroll position (clones into window buf).
    pub fn visible_lines(&mut self) -> &[Line] {
        self.ensure_fresh();
        self.cache.visible(self.scroll.offset, self.height)
    }

    /// Zero-copy iterator over visible lines — for direct Renderer painting.
    pub fn visible_iter(&mut self) -> crate::tui::viewport::ViewportIter<'_> {
        self.ensure_fresh();
        self.cache.window_iter(self.scroll.offset, self.height)
    }

    // ── Private ──

    fn push(&mut self, block: Block) {
        let rendered = render_block(&block, self.width, self.spinner_frame);
        self.blocks.push(block);
        self.cache.push(rendered);
        self.auto_scroll();
    }

    fn feed_last(&mut self, token: &str) {
        let idx = self.blocks.len().wrapping_sub(1);
        match self.blocks.last_mut() {
            Some(Block::Thinking(s)) => s.feed(token),
            Some(Block::Text(tb)) => tb.feed(token),
            _ => return,
        }
        self.cache.mark_dirty(idx);
        self.auto_scroll();
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

    fn ensure_fresh(&mut self) {
        self.cache
            .refresh(&mut self.blocks, self.width, self.spinner_frame);
    }

    fn auto_scroll(&mut self) {
        self.ensure_fresh();
        let total = self.cache.total_lines();
        self.scroll.auto_scroll(total, self.height);
    }

    fn clamp_scroll(&mut self) {
        self.ensure_fresh();
        let total = self.cache.total_lines();
        self.scroll.clamp(total, self.height);
    }

    fn max_scroll(&mut self) -> usize {
        self.ensure_fresh();
        self.cache.total_lines().saturating_sub(self.height)
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
            // Copy one UTF-8 character
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

    #[test]
    fn basic_content() {
        let mut log = OutputLog::new(80, 10);
        log.info("hello");
        log.divider();
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

        let (before, _, _) = log.scroll_info();
        let _idx = log.hit_test_block(1, 1).unwrap_or(0);
        // Find the tool block
        let mut tool_idx = None;
        for i in 0..log.blocks.len() {
            if matches!(&log.blocks[i], Block::Tool(_)) {
                tool_idx = Some(i);
                break;
            }
        }
        let ti = tool_idx.unwrap();
        assert!(log.toggle_expand(ti));
        let (after, _, _) = log.scroll_info();
        assert!(after > before);

        assert!(log.toggle_expand(ti)); // collapse
        let (collapsed, _, _) = log.scroll_info();
        assert_eq!(collapsed, before);
    }

    #[test]
    fn user_scrolled_prevents_auto_scroll() {
        let mut log = OutputLog::new(80, 5);
        for i in 0..20 {
            log.info(&format!("line {i}"));
        }
        log.scroll_up(5);
        let (_, _, offset_before) = log.scroll_info();
        log.info("new content");
        let (_, _, offset_after) = log.scroll_info();
        assert_eq!(offset_before, offset_after);
    }

    #[test]
    fn scroll_to_bottom_resumes_auto() {
        let mut log = OutputLog::new(80, 5);
        for i in 0..20 {
            log.info(&format!("line {i}"));
        }
        log.scroll_up(5);
        log.scroll_down(999);
        log.info("new");
        // Should have auto-scrolled to show "new"
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
        let (total, _, _) = log.scroll_info();
        assert!(total > 0);
    }

    #[test]
    fn streaming_visible_content() {
        let mut log = OutputLog::new(80, 20);
        log.append_token("hello ");
        let vis = log.visible_lines().to_vec();
        let text: String = vis.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("hello"), "expected 'hello' in visible, got: {text:?}");

        log.append_token("world\n");
        let vis = log.visible_lines().to_vec();
        let text: String = vis.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains("hello"), "after newline: {text:?}");

        log.append_token("line2");
        log.newline();
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
        // Streaming response
        log.append_token("Hi ");
        log.append_token("there!\n");
        log.append_token("How can I help?");
        log.newline();
        log.divider_with_label("1.0s");

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

        // Test via iterator path (what renderer uses)
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
        let (total, _, _) = log.scroll_info();
        assert!(total > 0);
    }

    /// Thinking → text spacing: exactly 1 empty line between, no more.
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

        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().enumerate().map(|(i, l)| {
            let w = l.visible_width();
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: [w={w:2}] {t}")
        }).collect();

        // Check no triple consecutive empty lines
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

        // Verify thinking and answer are both present
        let all: String = dump.join("\n");
        assert!(all.contains("Thinking:"), "missing thinking:\n{all}");
        assert!(all.contains("Here is my answer"), "missing answer:\n{all}");
    }

    /// Thinking wrap: line 2+ should NOT have cont_pad indent.
    #[test]
    fn thinking_wrap_no_extra_indent() {
        let mut log = OutputLog::new(30, 40);
        // First line gets "Thinking: " prefix (10 chars) + 30 chars text = wraps
        // Second committed line has no prefix — wrap should NOT add "  " pad
        log.append_thinking("short first line\n");
        log.append_thinking(&"x".repeat(50)); // 50 chars, wraps at width 30
        log.newline();

        let vis = log.visible_lines().to_vec();
        let dump: Vec<String> = vis.iter().enumerate().map(|(i, l)| {
            let t: String = l.spans.iter().map(|s| s.text.as_str()).collect();
            format!("{i:2}: [{:2}] |{t}|", l.visible_width())
        }).collect();

        // Find wrapped continuation of the "xxx..." line
        // It should start with "x", not "  x"
        let long_lines: Vec<&str> = vis.iter()
            .map(|l| l.spans.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .leak() // test only
            .iter()
            .map(|s| s.as_str())
            .filter(|t| t.starts_with("xx"))
            .collect();
        for t in &long_lines {
            assert!(!t.starts_with("  "), "wrapped line has unwanted indent:\n{}", dump.join("\n"));
        }
    }

    /// Thinking with blank lines — no excessive spacing.
    #[test]
    fn thinking_blank_lines_no_excess() {
        let mut log = OutputLog::new(80, 40);
        log.append_thinking("line1\n\nline3\n");
        log.append_token("answer");
        log.newline();

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
        let (total, _, _) = log.scroll_info();
        assert_eq!(total, 0);
    }

    #[test]
    fn assistant_message_renders() {
        let mut log = OutputLog::new(80, 10);
        log.assistant_message("hello\nworld");
        let (total, _, _) = log.scroll_info();
        assert!(total >= 2);
    }

    #[test]
    fn tool_history_renders_collapsed() {
        let mut log = OutputLog::new(80, 10);
        log.tool_history("bash", "$ ls");
        let (total, _, _) = log.scroll_info();
        assert!(total >= 1);
        // Should be done and not expandable (no output)
        assert!(!log.toggle_expand(0));
    }

    /// No consecutive empty lines in full streaming + tools flow.
    #[test]
    fn no_triple_empty_lines_in_markdown() {
        let mut log = OutputLog::new(80, 500);
        log.assistant_message("# Title\n\nParagraph one.\n\n## Section\n\n1. **Item one** — detail\n   - sub-item\n\n2. **Item two** — detail\n\n### Sub-section\n\nFinal paragraph.");
        let vis = log.visible_lines().to_vec();
        let mut consec = 0;
        for (i, l) in vis.iter().enumerate() {
            if l.visible_width() == 0 {
                consec += 1;
                assert!(consec < 3,
                    "3+ consecutive empty at line {i}");
            } else {
                consec = 0;
            }
        }
    }

    /// No consecutive empty lines in full streaming + tools flow.
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

    /// User blocks have consistent spacing: 1 gap before, 1 gap after.
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

        // Verify block pattern around each User block
        let user_indices: Vec<usize> = log.blocks.iter().enumerate()
            .filter(|(_, b)| matches!(b, Block::User(_)))
            .map(|(i, _)| i)
            .collect();

        assert_eq!(user_indices.len(), 2);
        for &ui in &user_indices {
            // Block before User must be Gap
            assert!(ui > 0);
            assert!(matches!(&log.blocks[ui - 1], Block::Gap),
                "block before User[{ui}] is not Gap: {:?}",
                std::mem::discriminant(&log.blocks[ui - 1]));
            // Block after User must be Gap
            assert!(ui + 1 < log.blocks.len());
            assert!(matches!(&log.blocks[ui + 1], Block::Gap),
                "block after User[{ui}] is not Gap: {:?}",
                std::mem::discriminant(&log.blocks[ui + 1]));
        }
    }
}
