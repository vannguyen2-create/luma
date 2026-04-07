/// Input prompt — struct definitions, public API, private helpers.
mod completion;
mod keys;
mod render;

use completion::Completion;

pub use completion::Command;

/// An image attached to the prompt.
#[derive(Clone)]
pub struct AttachedImage {
    pub media_type: String,
    pub data: Vec<u8>,
}

/// Prompt result after handling a key.
pub enum PromptAction {
    None,
    Redraw,
    Submit(String),
    Interrupt,
    ToggleThinking,
    PasteImage,
    PasteImagePath(String),
}

/// Input prompt state.
pub struct PromptState {
    pub(super) buffer: String,
    pub(super) cursor: usize,
    pub(super) history: Vec<String>,
    pub(super) history_idx: Option<usize>,
    pub(super) paste: Option<String>,
    pub images: Vec<AttachedImage>,
    pub(super) comp: Completion,
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
    pub fn attach_image(&mut self, media_type: String, data: Vec<u8>) {
        self.images.push(AttachedImage { media_type, data });
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

    // ── Dropdown helpers ──

    pub(super) fn has_dropdown(&self) -> bool {
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

    pub(super) fn dropdown_count(&self) -> usize {
        if let Some(q) = self.at_file_query() {
            return self.comp.file_matches(&q).len().min(8);
        }
        if self.is_command_mode() {
            return self.get_matches().len();
        }
        0
    }

    pub(super) fn accept_dropdown(&mut self) {
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

    pub(super) fn char_count(&self) -> usize {
        self.buffer.chars().count()
    }

    pub(super) fn char_to_byte(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }

    pub(super) fn is_command_mode(&self) -> bool {
        self.buffer.starts_with('/')
    }
    pub(super) fn command_query(&self) -> &str {
        &self.buffer[1..]
    }

    pub(super) fn get_matches(&self) -> Vec<&Command> {
        if !self.is_command_mode() {
            return Vec::new();
        }
        self.comp.command_matches(self.command_query())
    }

    pub(super) fn at_file_query(&self) -> Option<String> {
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

    pub(super) fn file_matches(&self, query: &str) -> Vec<String> {
        self.comp.file_matches(query)
    }

    pub(super) fn ghost(&self) -> String {
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

    pub(super) fn apply_ghost(&mut self) {
        let g = self.ghost();
        if !g.is_empty() {
            let byte_pos = self.char_to_byte(self.cursor);
            self.buffer.insert_str(byte_pos, &g);
            self.cursor += g.chars().count();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::KeyEvent;

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
    }

    #[test]
    fn paste_enter_submits() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        match p.handle_key(KeyEvent::Enter) {
            PromptAction::Submit(text) => assert_eq!(text, "1\n2\n3\n4\n5"),
            _ => panic!("expected Submit"),
        }
    }

    #[test]
    fn paste_escape_cancels() {
        let mut p = PromptState::new();
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
        p.handle_key(KeyEvent::Escape);
        assert!(!p.has_paste());
    }

    #[test]
    fn paste_with_existing_buffer() {
        let mut p = PromptState::new();
        type_str(&mut p, "prefix");
        p.handle_key(KeyEvent::Paste("1\n2\n3\n4\n5".into()));
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
