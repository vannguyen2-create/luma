/// Key handling — keystrokes, paste, history, dropdown navigation.
use super::PromptAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const PASTE_PREVIEW_THRESHOLD: usize = 5;
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff"];

impl super::PromptState {
    /// Handle a key event. Returns what the app should do.
    pub fn handle_key(&mut self, key: &KeyEvent) -> PromptAction {
        if self.paste.is_some() {
            return self.handle_paste_key(key);
        }
        if self.has_dropdown() && let Some(action) = self.handle_dropdown_key(key) {
            return action;
        }
        self.handle_normal_key(key)
    }

    /// Handle a bracketed paste event (crossterm Event::Paste).
    pub fn handle_paste(&mut self, text: String) -> PromptAction {
        if text.is_empty() {
            return PromptAction::PasteImage;
        }
        let trimmed = strip_quotes(text.trim());
        if !trimmed.contains('\n') && is_image_path(&trimmed) {
            return PromptAction::PasteImagePath(trimmed.into_owned());
        }
        self.insert_paste(text);
        PromptAction::Redraw
    }

    fn handle_paste_key(&mut self, key: &KeyEvent) -> PromptAction {
        match key.code {
            KeyCode::Enter => self.submit_paste(),
            KeyCode::Esc => {
                self.paste = None;
                PromptAction::Redraw
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.paste = None;
                PromptAction::Redraw
            }
            _ => PromptAction::None,
        }
    }

    fn handle_dropdown_key(&mut self, key: &KeyEvent) -> Option<PromptAction> {
        match key.code {
            KeyCode::Up => {
                self.comp.dropdown_idx = self.comp.dropdown_idx.saturating_sub(1);
                Some(PromptAction::Redraw)
            }
            KeyCode::Down => {
                let count = self.dropdown_count();
                self.comp.dropdown_idx = (self.comp.dropdown_idx + 1).min(count.saturating_sub(1));
                Some(PromptAction::Redraw)
            }
            KeyCode::Tab | KeyCode::Enter => {
                self.accept_dropdown();
                Some(PromptAction::Redraw)
            }
            KeyCode::Esc => {
                self.buffer.clear();
                self.cursor = 0;
                self.comp.dropdown_idx = 0;
                Some(PromptAction::Redraw)
            }
            _ => None,
        }
    }

    fn handle_normal_key(&mut self, key: &KeyEvent) -> PromptAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);

        match key.code {
            KeyCode::Enter if alt => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer.insert(byte_pos, '\n');
                self.cursor += 1;
                PromptAction::Redraw
            }
            KeyCode::Enter => self.on_enter(),
            KeyCode::Tab => {
                self.apply_ghost();
                PromptAction::Redraw
            }
            KeyCode::Char('c') if ctrl => {
                if self.buffer.is_empty() {
                    PromptAction::Interrupt
                } else {
                    self.buffer.clear();
                    self.cursor = 0;
                    PromptAction::Redraw
                }
            }
            KeyCode::Char('t') if ctrl => PromptAction::ToggleThinking,
            KeyCode::Char('a') if ctrl => {
                self.cursor = 0;
                PromptAction::Redraw
            }
            KeyCode::Char('e') if ctrl => {
                self.cursor = self.char_count();
                PromptAction::Redraw
            }
            KeyCode::Char('u') if ctrl => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer = self.buffer[byte_pos..].to_owned();
                self.cursor = 0;
                PromptAction::Redraw
            }
            KeyCode::Char('v') if alt => PromptAction::PasteImage,
            KeyCode::Esc => {
                if self.is_command_mode() || self.buffer.contains('\n') {
                    self.buffer.clear();
                    self.cursor = 0;
                    PromptAction::Redraw
                } else {
                    PromptAction::None
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let byte_pos = self.char_to_byte(self.cursor - 1);
                    let next_byte = self.char_to_byte(self.cursor);
                    self.buffer.replace_range(byte_pos..next_byte, "");
                    self.cursor -= 1;
                }
                self.comp.dropdown_idx = 0;
                PromptAction::Redraw
            }
            KeyCode::Up => self.history_prev(),
            KeyCode::Down => self.history_next(),
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                PromptAction::Redraw
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.char_count());
                PromptAction::Redraw
            }
            KeyCode::Char(c) => {
                let byte_pos = self.char_to_byte(self.cursor);
                self.buffer.insert(byte_pos, c);
                self.cursor += 1;
                self.comp.dropdown_idx = 0;
                if c == '@' {
                    self.comp.refresh_file_cache();
                }
                PromptAction::Redraw
            }
            _ => PromptAction::None,
        }
    }

    pub(super) fn on_enter(&mut self) -> PromptAction {
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

    pub(super) fn insert_paste(&mut self, text: String) {
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

    pub(super) fn submit_paste(&mut self) -> PromptAction {
        let text = self.paste.take().unwrap_or_default();
        let trimmed = text.trim().to_owned();
        if trimmed.is_empty() {
            return PromptAction::Redraw;
        }
        self.history.push(trimmed.clone());
        self.history_idx = None;
        PromptAction::Submit(trimmed)
    }

    pub(super) fn history_prev(&mut self) -> PromptAction {
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

    pub(super) fn history_next(&mut self) -> PromptAction {
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

/// Strip surrounding single or double quotes from a string.
fn strip_quotes(text: &str) -> std::borrow::Cow<'_, str> {
    if (text.starts_with('\'') && text.ends_with('\''))
        || (text.starts_with('"') && text.ends_with('"'))
    {
        std::borrow::Cow::Borrowed(&text[1..text.len() - 1])
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// Check if text looks like a path to an image file.
fn is_image_path(text: &str) -> bool {
    let path = std::path::Path::new(text);
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        && path.is_file()
}
