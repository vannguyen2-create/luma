/// Text selection, auto-scroll during drag, and cell-buffer-based copy.
use crate::tui::output::OutputLog;
use crate::tui::term;
use crate::tui::text::ScreenBuffer;
use std::io::Write;

const EDGE_SCROLL_ZONE: u16 = 2;
const EDGE_SCROLL_SPEED: usize = 2;

/// Tracks an active text selection with drag state.
pub struct Selection {
    pub start_row: u16,
    pub start_col: u16,
    pub end_row: u16,
    pub end_col: u16,
    pub is_active: bool,
}

impl Selection {
    /// Create inactive selection.
    pub fn new() -> Self {
        Self {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
            is_active: false,
        }
    }

    /// Begin selection at screen position.
    pub fn begin(&mut self, row: u16, col: u16) {
        self.start_row = row;
        self.start_col = col;
        self.end_row = row;
        self.end_col = col;
        self.is_active = true;
    }

    /// Update end position during drag.
    pub fn update(&mut self, row: u16, col: u16) {
        self.end_row = row;
        self.end_col = col;
    }

    /// Whether selection is non-empty (more than a click).
    pub fn has_range(&self) -> bool {
        self.start_row != self.end_row || self.start_col != self.end_col
    }

    /// Finish selection and reset.
    pub fn finish(&mut self) -> Option<(u16, u16, u16, u16)> {
        self.is_active = false;
        if !self.has_range() {
            return None;
        }
        let (mut r0, mut c0, mut r1, mut c1) =
            (self.start_row, self.start_col, self.end_row, self.end_col);
        if r0 > r1 || (r0 == r1 && c0 > c1) {
            std::mem::swap(&mut r0, &mut r1);
            std::mem::swap(&mut c0, &mut c1);
        }
        Some((r0, c0, r1, c1))
    }

    /// Auto-scroll if drag is near edge of output region. Returns true if scrolled.
    pub fn edge_scroll(&self, output: &mut OutputLog, region_top: u16, region_height: u16) -> bool {
        if !self.is_active {
            return false;
        }
        let region_bottom = region_top + region_height;

        if self.end_row < region_top + EDGE_SCROLL_ZONE {
            output.scroll_up(EDGE_SCROLL_SPEED);
            return true;
        }
        if self.end_row >= region_bottom.saturating_sub(EDGE_SCROLL_ZONE) {
            output.scroll_down(EDGE_SCROLL_SPEED);
            return true;
        }
        false
    }
}

/// Copy selected text from screen buffer. Skips PADDING and DECORATION cells.
pub fn copy_from_buffer(buf: &ScreenBuffer, r0: u16, c0: u16, r1: u16, c1: u16) {
    let mut lines = Vec::new();

    for row_1 in r0..=r1 {
        let row = row_1.saturating_sub(1);
        if row >= buf.height {
            continue;
        }
        let content_s = buf.content_start(row);
        let content_e = buf.content_end(row);
        if content_s >= content_e {
            lines.push(String::new());
            continue;
        }
        // Clamp selection to content area
        let col_start = if row_1 == r0 {
            c0.saturating_sub(1).max(content_s)
        } else {
            content_s
        };
        let col_end = if row_1 == r1 {
            (c1.saturating_sub(1)).min(content_e)
        } else {
            content_e
        };
        lines.push(buf.extract_text(row, col_start, col_end));
    }

    let text = lines.join("\n").trim().to_owned();

    if !text.is_empty() {
        let mut out = term::buffered_stdout();
        let _ = term::copy_to_clipboard(&mut out, &text);
        let _ = out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::text::{Line, Span};
    use crate::tui::theme::palette;
    use smallvec::smallvec;

    #[test]
    fn selection_lifecycle() {
        let mut sel = Selection::new();
        assert!(!sel.is_active);

        sel.begin(5, 10);
        assert!(sel.is_active);
        assert!(!sel.has_range());

        sel.update(8, 20);
        assert!(sel.has_range());

        let range = sel.finish().unwrap();
        assert_eq!(range, (5, 10, 8, 20));
        assert!(!sel.is_active);
    }

    #[test]
    fn selection_reversed() {
        let mut sel = Selection::new();
        sel.begin(10, 30);
        sel.update(5, 10);
        let (r0, _c0, r1, _c1) = sel.finish().unwrap();
        assert!(r0 <= r1);
    }

    #[test]
    fn selection_click_no_range() {
        let mut sel = Selection::new();
        sel.begin(5, 10);
        assert!(sel.finish().is_none());
    }

    #[test]
    fn extract_skips_padding() {
        let mut buf = ScreenBuffer::new(20, 1, palette::BG);
        buf.fill_padding(0, 0, 2, 1, palette::BG);
        let line = Line::new(smallvec![Span::new("hello", palette::FG)]);
        buf.write_line(&line, 0, 2, 18);
        let text = buf.extract_text(0, 0, 10);
        assert_eq!(text, "hello");
    }

    #[test]
    fn extract_skips_decoration() {
        let mut buf = ScreenBuffer::new(20, 1, palette::BG);
        let line = Line::new(smallvec![
            Span::new("· ", palette::DIM),
            Span::new("hello", palette::FG),
        ]);
        buf.write_line_decorated(&line, 0, 0, 20, 2);
        let text = buf.extract_text(0, 0, 10);
        assert_eq!(text, "hello");
    }

    #[test]
    fn content_start_skips_padding() {
        let mut buf = ScreenBuffer::new(20, 1, palette::BG);
        buf.fill_padding(0, 0, 4, 1, palette::BG);
        let line = Line::new(smallvec![Span::new("hi", palette::FG)]);
        buf.write_line(&line, 0, 4, 16);
        assert_eq!(buf.content_start(0), 4);
        assert_eq!(buf.content_end(0), 6);
    }
}
