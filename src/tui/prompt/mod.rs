/// Input prompt — handles keystrokes, paste, history, autocomplete.
mod completion;

use crate::event::KeyEvent;
use crate::tui::text::{Line, Span};
use crate::tui::theme::palette;
use completion::{dropdown_line, highlight_at_refs, Completion};
use smallvec::smallvec;

pub use completion::Command;

/// An image attached to the prompt.
#[derive(Clone)]
pub struct AttachedImage {
    pub media_type: String,
    pub data: Vec<u8>,
    pub label: String,
}

/// Prompt result after handling a key.
pub enum PromptAction {
    None,
    Redraw,
    Submit(String),
    Interrupt,
    ToggleThinking,
    PasteImage,
}

/// Input prompt state.
pub struct PromptState {
    buffer: String,
    cursor: usize,
    history: Vec<String>,
    history_idx: Option<usize>,
    paste: Option<String>,
    pub images: Vec<AttachedImage>,
    comp: Completion,
}

impl PromptState {
    /// Create an empty prompt.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: None,
            paste: None,
            images: Vec::new(),
            comp: Completion::new(),
        }
    }

    /// Register a slash command.
    pub fn add_command(&mut self, name: impl Into<String>, desc: impl Into<String>) {
        self.comp.commands.push(Command {
            name: name.into(),
            desc: desc.into(),
        });
    }

    /// Attach an image to the current prompt.
    pub fn attach_image(&mut self, media_type: String, data: Vec<u8>, label: String) {
        self.images.push(AttachedImage {
            media_type,
            data,
            label,
        });
    }

    /// Take and clear attached images (called on submit).
    pub fn take_images(&mut self) -> Vec<AttachedImage> {
        std::mem::take(&mut self.images)
    }

    /// Cursor column position for the renderer.
    pub fn cursor_column(&self) -> usize {
        if self.paste.is_some() {
            return 0;
        }
        let before_cursor: String = self.buffer.chars().take(self.cursor).collect();
        use crate::tui::text::display_width;
        before_cursor
            .rsplit_once('\n')
            .map(|(_, after)| display_width(after))
            .unwrap_or(display_width(&before_cursor))
    }

    /// Whether paste preview is active.
    pub fn has_paste(&self) -> bool {
        self.paste.is_some()
    }

    /// Handle a key event. Returns what the app should do.
    pub fn handle_key(&mut self, key: KeyEvent) -> PromptAction {
        if self.paste.is_some() {
            return match key {
                KeyEvent::Enter => self.submit_paste(),
                KeyEvent::Escape | KeyEvent::CtrlC => {
                    self.paste = None;
                    PromptAction::Redraw
                }
                _ => PromptAction::None,
            };
        }

        // Dropdown navigation intercepts arrow keys, Tab, Enter
        if self.has_dropdown() {
            match key {
                KeyEvent::ArrowUp => {
                    self.comp.dropdown_idx = self.comp.dropdown_idx.saturating_sub(1);
                    return PromptAction::Redraw;
                }
                KeyEvent::ArrowDown => {
                    let count = self.dropdown_count();
                    self.comp.dropdown_idx =
                        (self.comp.dropdown_idx + 1).min(count.saturating_sub(1));
                    return PromptAction::Redraw;
                }
                KeyEvent::Tab | KeyEvent::Enter => {
                    self.accept_dropdown();
                    return PromptAction::Redraw;
                }
                KeyEvent::Escape => {
                    self.buffer.clear();
                    self.cursor = 0;
                    self.comp.dropdown_idx = 0;
                    return PromptAction::Redraw;
                }
                _ => {}
            }
        }

        match key {
            KeyEvent::Enter => self.on_enter(),
            KeyEvent::Tab => {
                self.apply_ghost();
                PromptAction::Redraw
            }
            KeyEvent::CtrlC => {
                if self.buffer.is_empty() {
                    PromptAction::Interrupt
                } else {
                    self.buffer.clear();
                    self.cursor = 0;
                    PromptAction::Redraw
                }
            }
            KeyEvent::CtrlT => PromptAction::ToggleThinking,
            KeyEvent::Escape => {
                if self.is_command_mode() || self.buffer.contains('\n') {
                    self.buffer.clear();
                    self.cursor = 0;
                    PromptAction::Redraw
                } else {
                    PromptAction::None
                }
            }
            KeyEvent::Backspace => {
                if self.cursor > 0 {
                    let byte_pos = self.char_to_byte(self.cursor - 1);
                    let next_byte = self.char_to_byte(self.cursor);
                    self.buffer.replace_range(byte_pos..next_byte, "");
                    self.cursor -= 1;
                }
                self.comp.dropdown_idx = 0;
                PromptAction::Redraw
            }
            KeyEvent::ArrowUp => self.history_prev(),
            KeyEvent::ArrowDown => self.history_next(),
            KeyEvent::ArrowLeft => {
                self.cursor = self.cursor.saturating_sub(1);
                PromptAction::Redraw
            }
            KeyEvent::ArrowRight => {
                self.cursor = (self.cursor + 1).min(self.char_count());
                PromptAction::Redraw
            }
            KeyEvent::CtrlA => {
                self.cursor = 0;
                PromptAction::Redraw
            }
            KeyEvent::CtrlE => {
                self.cursor = self.char_count();
                PromptAction::Redraw
            }
            KeyEvent::CtrlU => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer = self.buffer[byte_pos..].to_owned();
                self.cursor = 0;
                PromptAction::Redraw
            }
            KeyEvent::Char(c) => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer.insert(byte_pos, c);
                self.cursor += 1;
                self.comp.dropdown_idx = 0;
                if c == '@' {
                    self.comp.refresh_file_cache();
                }
                PromptAction::Redraw
            }
            KeyEvent::AltEnter => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer.insert(byte_pos, '\n');
                self.cursor += 1;
                PromptAction::Redraw
            }
            KeyEvent::Paste(text) => {
                self.insert_paste(text);
                PromptAction::Redraw
            }
            KeyEvent::AltV => PromptAction::PasteImage,
        }
    }

    /// Render the prompt lines.
    pub fn lines(&self) -> Vec<Line> {
        if let Some(pasted) = &self.paste {
            let n = pasted.lines().count();
            let first = pasted.lines().next().unwrap_or("");
            let preview = if first.len() > 30 {
                format!("{}...", &first[..30])
            } else {
                first.to_owned()
            };
            return vec![
                Line::new(smallvec![
                    Span::new(format!("[Pasted ~{n} lines] "), palette::WARN),
                    Span::new(preview, palette::DIM),
                ]),
                Line::new(smallvec![
                    Span::new("enter", palette::ACCENT),
                    Span::new(" send  ", palette::DIM),
                    Span::new("esc", palette::ACCENT),
                    Span::new(" cancel", palette::DIM),
                ]),
            ];
        }

        let ghost = self.ghost();
        let line_count = self.buffer.lines().count();
        if line_count > 1 {
            let last_line = self.buffer.lines().last().unwrap_or("");
            return vec![
                Line::new(smallvec![Span::new(last_line.to_owned(), palette::FG)]),
                Line::new(smallvec![
                    Span::new(format!("{line_count} lines "), palette::DIM),
                    Span::new("enter", palette::ACCENT),
                    Span::new(" send  ", palette::DIM),
                    Span::new("esc", palette::ACCENT),
                    Span::new(" clear", palette::DIM),
                ]),
            ];
        }
        let mut spans = highlight_at_refs(&self.buffer);
        if !ghost.is_empty() {
            spans.push(Span::new(ghost, palette::MUTED));
        }
        let mut lines = vec![Line::new(spans)];
        if !self.images.is_empty() {
            let labels: Vec<String> = self
                .images
                .iter()
                .map(|img| format!("[{}]", img.label))
                .collect();
            lines.push(Line::new(smallvec![
                Span::new(labels.join(" "), palette::DIM),
                Span::new("  alt+v", palette::ACCENT),
                Span::new(" add image", palette::DIM),
            ]));
        }
        lines
    }

    /// Render dropdown for commands or @file autocomplete.
    pub fn dropdown(&self) -> Vec<Line> {
        use crate::tui::theme::icon;
        let bar = icon::PROMPT;

        if let Some(query) = self.at_file_query() {
            let matches = self.comp.file_matches(&query);
            if matches.is_empty() {
                return Vec::new();
            }
            return matches
                .iter()
                .enumerate()
                .take(8)
                .map(|(i, path)| {
                    dropdown_line(
                        bar,
                        &format!("@{path}"),
                        "",
                        i == self.comp.dropdown_idx,
                        palette::FILE_REF,
                    )
                })
                .collect();
        }

        let matches = self.get_matches();
        if matches.is_empty() {
            return Vec::new();
        }
        let max_name = matches.iter().map(|c| c.name.len()).max().unwrap_or(0);
        matches
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let pad = max_name - c.name.len();
                dropdown_line(
                    bar,
                    &format!("/{}", c.name),
                    &format!("{}  {}", " ".repeat(pad), c.desc),
                    i == self.comp.dropdown_idx,
                    palette::ACCENT,
                )
            })
            .collect()
    }

    // ── Dropdown ──

    /// Whether any dropdown is visible.
    fn has_dropdown(&self) -> bool {
        if let Some(q) = self.at_file_query() {
            return !self.comp.file_matches(&q).is_empty();
        }
        if self.is_command_mode() {
            let matches = self.get_matches();
            if matches.is_empty() {
                return false;
            }
            if matches.len() == 1 && matches[0].name == self.command_query() {
                return false;
            }
            return true;
        }
        false
    }

    /// Number of items in current dropdown.
    fn dropdown_count(&self) -> usize {
        if let Some(q) = self.at_file_query() {
            return self.comp.file_matches(&q).len().min(8);
        }
        if self.is_command_mode() {
            return self.get_matches().len();
        }
        0
    }

    /// Accept the currently selected dropdown item.
    fn accept_dropdown(&mut self) {
        if let Some(query) = self.at_file_query() {
            let matches = self.comp.file_matches(&query);
            if let Some(path) = matches.get(self.comp.dropdown_idx) {
                let before: String = self.buffer.chars().take(self.cursor).collect();
                if let Some(at_pos) = before.rfind('@') {
                    let after: String = self.buffer.chars().skip(self.cursor).collect();
                    self.buffer = format!("{}@{} {}", &before[..at_pos], path, after);
                    self.cursor = before[..at_pos].chars().count() + 1 + path.chars().count() + 1;
                }
            }
        } else if self.is_command_mode() {
            let matches = self.get_matches();
            if let Some(cmd) = matches.get(self.comp.dropdown_idx) {
                self.buffer = format!("/{}", cmd.name);
                self.cursor = self.char_count();
            }
        }
        self.comp.dropdown_idx = 0;
    }

    // ── Private ──

    fn char_count(&self) -> usize {
        self.buffer.chars().count()
    }

    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }

    fn is_command_mode(&self) -> bool {
        self.buffer.starts_with('/')
    }
    fn command_query(&self) -> &str {
        &self.buffer[1..]
    }

    fn get_matches(&self) -> Vec<&Command> {
        if !self.is_command_mode() {
            return Vec::new();
        }
        self.comp.command_matches(self.command_query())
    }

    /// Extract @file query at cursor position.
    fn at_file_query(&self) -> Option<String> {
        let before: String = self.buffer.chars().take(self.cursor).collect();
        let at_pos = before.rfind('@')?;
        if at_pos > 0 && !before.as_bytes()[at_pos - 1].is_ascii_whitespace() {
            return None;
        }
        let query = &before[at_pos + 1..];
        if query.contains(char::is_whitespace) {
            return None;
        }
        Some(query.to_owned())
    }

    /// Fuzzy match files (delegates to Completion).
    fn file_matches(&self, query: &str) -> Vec<String> {
        self.comp.file_matches(query)
    }

    fn ghost(&self) -> String {
        if let Some(query) = self.at_file_query() {
            let matches = self.file_matches(&query);
            if matches.len() == 1 && matches[0] != query {
                return matches[0][query.len()..].to_owned();
            }
            return String::new();
        }
        if !self.is_command_mode() {
            return String::new();
        }
        let matches = self.get_matches();
        if matches.len() != 1 {
            return String::new();
        }
        let q = self.command_query();
        if matches[0].name == q {
            return String::new();
        }
        matches[0].name[q.len()..].to_owned()
    }

    fn apply_ghost(&mut self) {
        let g = self.ghost();
        if !g.is_empty() {
            let byte_pos = self.char_to_byte(self.cursor);
            self.buffer.insert_str(byte_pos, &g);
            self.cursor += g.chars().count();
        }
    }

    fn insert_paste(&mut self, text: String) {
        const PASTE_PREVIEW_THRESHOLD: usize = 5;
        let line_count = text.lines().count();
        if line_count < PASTE_PREVIEW_THRESHOLD {
            let byte_pos = self.char_to_byte(self.cursor);
            let trimmed = text.trim_end_matches('\n');
            self.buffer.insert_str(byte_pos, trimmed);
            self.cursor += trimmed.chars().count();
        } else {
            let combined = if self.buffer.is_empty() {
                text
            } else {
                format!("{}\n{}", self.buffer, text)
            };
            self.paste = Some(combined);
            self.buffer.clear();
            self.cursor = 0;
        }
    }

    fn submit_paste(&mut self) -> PromptAction {
        let text = self.paste.take().unwrap_or_default();
        let trimmed = text.trim().to_owned();
        if trimmed.is_empty() {
            return PromptAction::Redraw;
        }
        self.history.push(trimmed.clone());
        self.history_idx = None;
        PromptAction::Submit(trimmed)
    }

    fn on_enter(&mut self) -> PromptAction {
        if self.is_command_mode() {
            let g = self.ghost();
            if !g.is_empty() {
                self.buffer.push_str(&g);
                self.cursor = self.char_count();
                return PromptAction::Redraw;
            }
            let query = self.command_query().to_owned();
            let found = self.comp.commands.iter().any(|c| c.name == query);
            self.buffer.clear();
            self.cursor = 0;
            if found {
                return PromptAction::Submit(format!("/{query}"));
            }
            return PromptAction::Redraw;
        }
        let text = self.buffer.trim().to_owned();
        if text.is_empty() {
            return PromptAction::Redraw;
        }
        self.history.push(text.clone());
        self.history_idx = None;
        self.buffer.clear();
        self.cursor = 0;
        PromptAction::Submit(text)
    }

    fn history_prev(&mut self) -> PromptAction {
        if self.history.is_empty() {
            return PromptAction::Redraw;
        }
        if self.history_idx.is_none() && !self.buffer.is_empty() {
            return PromptAction::Redraw;
        }
        let idx = self
            .history_idx
            .unwrap_or(self.history.len())
            .saturating_sub(1);
        self.history_idx = Some(idx);
        self.buffer = self.history.get(idx).cloned().unwrap_or_default();
        self.cursor = self.char_count();
        PromptAction::Redraw
    }

    fn history_next(&mut self) -> PromptAction {
        let Some(idx) = self.history_idx else {
            return PromptAction::Redraw;
        };
        let idx = (idx + 1).min(self.history.len());
        self.history_idx = Some(idx);
        self.buffer = self.history.get(idx).cloned().unwrap_or_default();
        self.cursor = self.char_count();
        PromptAction::Redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_str(p: &mut PromptState, s: &str) {
        for c in s.chars() {
            p.handle_key(KeyEvent::Char(c));
        }
    }

    #[test]
    fn type_and_submit() {
        let mut p = PromptState::new();
        type_str(&mut p, "hello");
        match p.handle_key(KeyEvent::Enter) {
            PromptAction::Submit(text) => assert_eq!(text, "hello"),
            _ => panic!("expected Submit"),
        }
    }

    #[test]
    fn empty_enter_no_submit() {
        let mut p = PromptState::new();
        assert!(matches!(
            p.handle_key(KeyEvent::Enter),
            PromptAction::Redraw
        ));
    }

    #[test]
    fn history_navigation() {
        let mut p = PromptState::new();
        type_str(&mut p, "first");
        p.handle_key(KeyEvent::Enter);
        type_str(&mut p, "second");
        p.handle_key(KeyEvent::Enter);
        p.handle_key(KeyEvent::ArrowUp);
        assert_eq!(p.buffer, "second");
        p.handle_key(KeyEvent::ArrowUp);
        assert_eq!(p.buffer, "first");
    }

    #[test]
    fn history_only_when_empty() {
        let mut p = PromptState::new();
        type_str(&mut p, "first");
        p.handle_key(KeyEvent::Enter);
        type_str(&mut p, "x");
        p.handle_key(KeyEvent::ArrowUp);
        assert_eq!(p.buffer, "x");
    }

    #[test]
    fn command_ghost() {
        let mut p = PromptState::new();
        p.add_command("model", "switch model");
        type_str(&mut p, "/mo");
        assert_eq!(p.ghost(), "del");
    }

    #[test]
    fn command_submit() {
        let mut p = PromptState::new();
        p.add_command("new", "new thread");
        type_str(&mut p, "/new");
        match p.handle_key(KeyEvent::Enter) {
            PromptAction::Submit(cmd) => assert_eq!(cmd, "/new"),
            _ => panic!("expected Submit"),
        }
    }

    #[test]
    fn ctrl_c_clears_buffer() {
        let mut p = PromptState::new();
        type_str(&mut p, "hello");
        assert!(matches!(
            p.handle_key(KeyEvent::CtrlC),
            PromptAction::Redraw
        ));
        assert!(p.buffer.is_empty());
    }

    #[test]
    fn ctrl_c_empty_interrupts() {
        let mut p = PromptState::new();
        assert!(matches!(
            p.handle_key(KeyEvent::CtrlC),
            PromptAction::Interrupt
        ));
    }

    #[test]
    fn cursor_column() {
        let mut p = PromptState::new();
        type_str(&mut p, "abc");
        assert_eq!(p.cursor_column(), 3);
    }

    #[test]
    fn paste_single_line_inline() {
        let mut p = PromptState::new();
        type_str(&mut p, "pre ");
        p.handle_key(KeyEvent::Paste("hello".into()));
        assert_eq!(p.buffer, "pre hello");
        assert!(!p.has_paste());
    }

    #[test]
    fn paste_short_multiline_inline() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("a\nb\nc".into()));
        assert!(!p.has_paste());
        assert_eq!(p.buffer, "a\nb\nc");
    }

    #[test]
    fn paste_long_multiline_preview() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        assert!(p.has_paste());
        assert_eq!(p.cursor_column(), 0);
    }

    #[test]
    fn paste_enter_submits() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        match p.handle_key(KeyEvent::Enter) {
            PromptAction::Submit(text) => assert_eq!(text, "1\n2\n3\n4\n5"),
            _ => panic!("expected Submit"),
        }
        assert!(!p.has_paste());
    }

    #[test]
    fn paste_escape_cancels() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        p.handle_key(KeyEvent::Escape);
        assert!(!p.has_paste());
        assert!(p.buffer.is_empty());
    }

    #[test]
    fn paste_with_existing_buffer() {
        let mut p = PromptState::new();
        type_str(&mut p, "prefix");
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        assert!(p.has_paste());
        match p.handle_key(KeyEvent::Enter) {
            PromptAction::Submit(text) => assert!(text.starts_with("prefix\n")),
            _ => panic!("expected Submit"),
        }
    }

    #[test]
    fn alt_enter_newline() {
        let mut p = PromptState::new();
        type_str(&mut p, "line1");
        p.handle_key(KeyEvent::AltEnter);
        type_str(&mut p, "line2");
        assert_eq!(p.buffer, "line1\nline2");
    }

    #[test]
    fn multiline_cursor_column() {
        let mut p = PromptState::new();
        type_str(&mut p, "abc");
        p.handle_key(KeyEvent::AltEnter);
        type_str(&mut p, "xy");
        assert_eq!(p.cursor_column(), 2);
    }

    #[test]
    fn at_file_query_detected() {
        let mut p = PromptState::new();
        type_str(&mut p, "check @src/");
        assert_eq!(p.at_file_query(), Some("src/".into()));
    }

    #[test]
    fn at_file_query_none_without_at() {
        let mut p = PromptState::new();
        type_str(&mut p, "hello world");
        assert_eq!(p.at_file_query(), None);
    }

    #[test]
    fn at_file_query_email_ignored() {
        let mut p = PromptState::new();
        type_str(&mut p, "user@example.com ");
        assert_eq!(p.at_file_query(), None);
    }

    #[test]
    fn at_file_cache_populated() {
        let mut p = PromptState::new();
        type_str(&mut p, "@");
        assert!(p.comp.file_cache_valid);
        assert!(!p.comp.file_cache.is_empty());
        assert!(p.comp.file_cache.iter().any(|f| f == "Cargo.toml"));
    }

    #[test]
    fn at_file_matches_filters() {
        let mut p = PromptState::new();
        type_str(&mut p, "@");
        let matches = p.file_matches("Cargo");
        assert!(!matches.is_empty());
        assert!(matches[0].starts_with("Cargo"));
    }

    #[test]
    fn dropdown_arrow_navigates() {
        let mut p = PromptState::new();
        p.add_command("model", "switch model");
        p.add_command("new", "new thread");
        type_str(&mut p, "/");
        assert!(p.has_dropdown());
        assert_eq!(p.comp.dropdown_idx, 0);
        p.handle_key(KeyEvent::ArrowDown);
        assert_eq!(p.comp.dropdown_idx, 1);
        p.handle_key(KeyEvent::ArrowUp);
        assert_eq!(p.comp.dropdown_idx, 0);
    }

    #[test]
    fn dropdown_enter_accepts() {
        let mut p = PromptState::new();
        p.add_command("model", "switch model");
        p.add_command("new", "new thread");
        type_str(&mut p, "/");
        p.handle_key(KeyEvent::ArrowDown);
        p.handle_key(KeyEvent::Enter);
        assert_eq!(p.buffer, "/new");
    }

    #[test]
    fn dropdown_resets_on_typing() {
        let mut p = PromptState::new();
        p.add_command("model", "switch model");
        p.add_command("new", "new thread");
        type_str(&mut p, "/");
        p.handle_key(KeyEvent::ArrowDown);
        assert_eq!(p.comp.dropdown_idx, 1);
        p.handle_key(KeyEvent::Char('m'));
        assert_eq!(p.comp.dropdown_idx, 0);
    }

    #[test]
    fn command_exact_match_no_dropdown() {
        let mut p = PromptState::new();
        p.add_command("new", "new thread");
        type_str(&mut p, "/new");
        assert!(!p.has_dropdown());
    }
}
