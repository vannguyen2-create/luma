/// Cell-buffer renderer — regions paint into ScreenBuffer, diff per-row hash.
use crate::tui::text::{CellFlag, Line, Padding, ScreenBuffer};
use crate::tui::theme::Rgb;
use std::io::{self, BufWriter, Stdout, Write};

/// A named rectangular region on screen with padding.
#[derive(Debug, Clone)]
pub struct Region {
    pub row: u16,
    pub col: u16,
    pub width: u16,
    pub height: u16,
    pub bg: Rgb,
    pub padding: Padding,
}

impl Region {
    /// Content area start column (1-indexed terminal position).
    #[cfg(test)]
    pub fn content_col(&self) -> u16 {
        self.col + self.padding.left
    }

    /// Content area width.
    pub fn content_width(&self) -> u16 {
        self.width
            .saturating_sub(self.padding.left + self.padding.right)
    }

    /// Content area height.
    pub fn content_height(&self) -> u16 {
        self.height
            .saturating_sub(self.padding.top + self.padding.bottom)
    }

    /// Content area start row (1-indexed terminal position).
    #[cfg(test)]
    pub fn content_row(&self) -> u16 {
        self.row + self.padding.top
    }
}

/// Cursor position or hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorState {
    Visible { row: u16, col: u16 },
    Hidden,
}

/// Scrollbar cell type — supports fractional thumb edges.
#[derive(Clone, Copy)]
pub enum ScrollCell {
    Track,
    Thumb,
    /// Top edge: thumb starts at 1/8th offset (1-7). Render lower block.
    TopEdge(u8),
    /// Bottom edge: thumb fills top N/8ths (1-7). Render upper block.
    BottomEdge(u8),
}

/// Scrollbar overlay.
pub struct Overlay {
    pub row: u16,
    pub col: u16,
    pub fg_thumb: Rgb,
    pub fg_track: Rgb,
    pub cells: Vec<ScrollCell>,
}

/// Screen-space selection highlight (1-indexed, inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub r0: u16,
    pub c0: u16,
    pub r1: u16,
    pub c1: u16,
}

/// A named screen region with its current content lines.
struct RegionSlot {
    name: String,
    region: Region,
    lines: Vec<Line>,
}

/// Cell-buffer terminal renderer with per-row hash diff.
pub struct Renderer {
    out: BufWriter<Stdout>,
    slots: Vec<RegionSlot>,
    buf: ScreenBuffer,
    prev_hashes: Vec<u64>,
    overlay: Option<Overlay>,
    selection: Option<SelectionRange>,
    cursor: CursorState,
}

impl Renderer {
    /// Create renderer for given terminal dimensions.
    pub fn new(term_width: u16, term_height: u16) -> Self {
        Self {
            out: BufWriter::with_capacity(8192, io::stdout()),
            slots: Vec::new(),
            buf: ScreenBuffer::new(term_width, term_height, Rgb(0, 0, 0)),
            prev_hashes: Vec::new(),
            overlay: None,
            selection: None,
            cursor: CursorState::Hidden,
        }
    }

    /// Define a named region.
    pub fn define(&mut self, name: &str, region: Region) {
        self.slots.push(RegionSlot {
            name: name.to_owned(),
            region,
            lines: Vec::new(),
        });
    }

    /// Update region geometry.
    pub fn update_region(&mut self, name: &str, region: Region) {
        if let Some(slot) = self.slot_mut(name) {
            slot.region = region;
        }
    }

    /// Update terminal dimensions (on resize).
    pub fn set_term_size(&mut self, w: u16, h: u16) {
        let bg = self
            .slots
            .first()
            .map(|s| s.region.bg)
            .unwrap_or(Rgb(0, 0, 0));
        self.buf.resize(w, h, bg);
        self.prev_hashes.clear();
    }

    /// Set rendered lines for a region (clones — use for small regions only).
    pub fn set_lines(&mut self, name: &str, lines: &[Line]) {
        if let Some(slot) = self.slot_mut(name) {
            slot.lines.clear();
            slot.lines.extend_from_slice(lines);
        }
    }

    /// Set lines from an iterator — avoids intermediate slice allocation.
    pub fn set_lines_iter<'a>(&mut self, name: &str, lines: impl Iterator<Item = &'a Line>) {
        if let Some(slot) = self.slot_mut(name) {
            slot.lines.clear();
            slot.lines.extend(lines.cloned());
        }
    }

    /// Set scrollbar overlay.
    pub fn set_overlay(&mut self, overlay: Option<Overlay>) {
        self.overlay = overlay;
    }

    /// Set selection highlight range.
    pub fn set_selection(&mut self, sel: Option<SelectionRange>) {
        self.selection = sel;
    }

    /// Set cursor state.
    pub fn set_cursor(&mut self, state: CursorState) {
        self.cursor = state;
    }

    /// Clear screen and invalidate previous frame.
    pub fn clear_screen(&mut self) {
        let _ = write!(self.out, "\x1b[2J");
        self.prev_hashes.clear();
    }

    /// Access the screen buffer (for selection copy).
    pub fn buffer(&self) -> &ScreenBuffer {
        &self.buf
    }

    /// Compose all regions into cell buffer, diff against prev, write changes.
    /// Uses synchronized rendering (DEC mode 2026) to prevent flicker.
    pub fn flush(&mut self) -> io::Result<()> {
        let bg = self
            .slots
            .first()
            .map(|s| s.region.bg)
            .unwrap_or(Rgb(0, 0, 0));
        self.buf.clear(bg);

        // Paint each region
        for i in 0..self.slots.len() {
            Self::paint_iter(
                &mut self.buf,
                &self.slots[i].region,
                self.slots[i].lines.iter(),
            );
        }

        // Paint scrollbar into cell buffer (before diff — no flicker)
        if let Some(ov) = &self.overlay {
            const LOWER: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];
            let col = ov.col.saturating_sub(1); // 1-indexed → 0-indexed
            for (j, &cell) in ov.cells.iter().enumerate() {
                let row = ov.row.saturating_sub(1) + j as u16;
                if row >= self.buf.height || col >= self.buf.width {
                    continue;
                }
                // If this cell was the trailing half of a wide char,
                // clear the wide char in the previous cell to avoid artifacts.
                {
                    let c = self.buf.get(row, col);
                    if c.flags.is_wide_pad() && col > 0 {
                        let p = self.buf.get_mut(row, col - 1);
                        p.ch = ' ';
                    }
                }
                let c = self.buf.get_mut(row, col);
                c.flags = CellFlag(CellFlag::NONE);
                match cell {
                    ScrollCell::Track => {
                        c.ch = '█';
                        c.fg = ov.fg_track;
                        c.bg = ov.fg_track;
                    }
                    ScrollCell::Thumb => {
                        c.ch = '█';
                        c.fg = ov.fg_thumb;
                        c.bg = ov.fg_thumb;
                    }
                    ScrollCell::TopEdge(eighths) => {
                        // Lower block: bottom=fg(thumb), top=bg(track)
                        let fill = (8 - eighths).min(7) as usize;
                        c.ch = LOWER[fill];
                        c.fg = ov.fg_thumb;
                        c.bg = ov.fg_track;
                    }
                    ScrollCell::BottomEdge(eighths) => {
                        // Lower block: bottom=fg(track), top=bg(thumb)
                        let empty = (8 - eighths).min(7) as usize;
                        c.ch = LOWER[empty];
                        c.fg = ov.fg_track;
                        c.bg = ov.fg_thumb;
                    }
                }
            }
        }

        // Apply selection reverse
        if let Some(sel) = self.selection {
            self.apply_selection(sel);
        }

        // Diff and write changed rows
        let total_rows = self.buf.height as usize;
        self.prev_hashes.resize(total_rows, 0);
        let mut any_diff = false;

        // Synchronized rendering: buffer output until done (prevents flicker).
        let _ = write!(self.out, "\x1b[?2026h");
        // Hide cursor before any row writes to prevent visible jumping
        let _ = write!(self.out, "\x1b[?25l");

        for row in 0..self.buf.height {
            let hash = self.buf.row_hash(row);
            if hash != self.prev_hashes[row as usize] {
                let ansi = self.buf.render_row(row);
                let _ = write!(self.out, "\x1b[{};1H{}", row + 1, ansi);
                self.prev_hashes[row as usize] = hash;
                any_diff = true;
            }
        }

        // Restore cursor after all writes
        if let CursorState::Visible { row, col } = self.cursor {
            let _ = write!(self.out, "\x1b[{row};{col}H\x1b[?25h");
            any_diff = true;
        }

        // End synchronized rendering — terminal paints atomically.
        let _ = write!(self.out, "\x1b[?2026l");

        if any_diff {
            self.out.flush()?;
        }
        Ok(())
    }

    fn slot_mut(&mut self, name: &str) -> Option<&mut RegionSlot> {
        self.slots.iter_mut().find(|s| s.name == name)
    }

    /// Paint lines into buffer for a region — shared by slot-based and iterator paths.
    fn paint_iter<'a>(
        buf: &mut ScreenBuffer,
        region: &Region,
        lines: impl Iterator<Item = &'a Line>,
    ) {
        let r = region.row.saturating_sub(1); // 1→0 indexed
        let c = region.col.saturating_sub(1);

        // Fill entire region area with bg
        buf.fill_bg(r, c, region.width, region.height, region.bg);

        // Mark padding cells
        if region.padding.left > 0 {
            buf.fill_padding(r, c, region.padding.left, region.height, region.bg);
        }
        if region.padding.right > 0 {
            let right_col = c + region.width - region.padding.right;
            buf.fill_padding(r, right_col, region.padding.right, region.height, region.bg);
        }
        if region.padding.top > 0 {
            let cc = c + region.padding.left;
            let cw = region.content_width();
            buf.fill_padding(r, cc, cw, region.padding.top, region.bg);
        }
        if region.padding.bottom > 0 {
            let cc = c + region.padding.left;
            let cw = region.content_width();
            let br = r + region.height - region.padding.bottom;
            buf.fill_padding(br, cc, cw, region.padding.bottom, region.bg);
        }

        // Write content lines
        let content_col = c + region.padding.left;
        let content_row = r + region.padding.top;
        let content_w = region.content_width();
        let content_h = region.content_height();

        for (j, line) in lines.enumerate() {
            if j as u16 >= content_h {
                break;
            }
            let row = content_row + j as u16;

            // Line-level background
            if let Some(line_bg) = line.bg {
                if line.margin {
                    buf.fill_bg(row, content_col, content_w, 1, line_bg);
                } else {
                    buf.fill_bg(row, c, region.width, 1, line_bg);
                    if region.padding.left > 0 {
                        buf.fill_padding(row, c, region.padding.left, 1, line_bg);
                    }
                    if region.padding.right > 0 {
                        let rc = c + region.width - region.padding.right;
                        buf.fill_padding(row, rc, region.padding.right, 1, line_bg);
                    }
                }
            }

            // Write spans with indent, margin, and bleed handling
            let indent = line.indent;
            if line.margin && line.bg.is_some() {
                const MARGIN: u16 = 2;
                let inner_col = content_col + MARGIN;
                let inner_w = content_w.saturating_sub(MARGIN * 2);
                if inner_w > 0 {
                    buf.write_line(line, row, inner_col, inner_w);
                }
            } else if line.bleed > 0 {
                let bleed_col = content_col.saturating_sub(line.bleed);
                let bleed_w = content_w + line.bleed;
                buf.write_line(line, row, bleed_col, bleed_w);
            } else if indent > 0 {
                let line_bg = line.bg.unwrap_or(region.bg);
                buf.fill_padding(row, content_col, indent, 1, line_bg);
                let text_col = content_col + indent;
                let text_w = content_w.saturating_sub(indent);
                buf.write_line(line, row, text_col, text_w);
            } else {
                buf.write_line(line, row, content_col, content_w);
            }
        }
    }

    fn apply_selection(&mut self, sel: SelectionRange) {
        for row_1 in sel.r0..=sel.r1 {
            let row = row_1.saturating_sub(1);
            if row >= self.buf.height {
                continue;
            }
            let content_s = self.buf.content_start(row);
            let content_e = self.buf.content_end(row);
            if content_s >= content_e {
                continue;
            }
            // Clamp selection to content area
            let c_start = if row_1 == sel.r0 {
                sel.c0.saturating_sub(1).max(content_s)
            } else {
                content_s
            };
            let c_end = if row_1 == sel.r1 {
                (sel.c1.saturating_sub(1)).min(content_e)
            } else {
                content_e
            };
            for c in c_start..c_end {
                if c < self.buf.width {
                    let cell = self.buf.get_mut(row, c);
                    if cell.flags.is_content() {
                        std::mem::swap(&mut cell.fg, &mut cell.bg);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::text::Span;
    use crate::tui::theme::palette;
    use smallvec::smallvec;

    fn test_region() -> Region {
        Region {
            row: 1,
            col: 3,
            width: 30,
            height: 5,
            bg: palette::BG,
            padding: Padding::horizontal(2),
        }
    }

    #[test]
    fn region_content_area() {
        let r = test_region();
        assert_eq!(r.content_col(), 5);
        assert_eq!(r.content_width(), 26);
        assert_eq!(r.content_height(), 5);
    }

    #[test]
    fn define_and_set_lines() {
        let mut r = Renderer::new(40, 10);
        r.define("main", test_region());
        let lines = vec![Line::new(smallvec![Span::new("hello", palette::FG)])];
        r.set_lines("main", &lines);
        assert_eq!(r.slots[0].lines.len(), 1);
    }

    #[test]
    fn padding_creates_padding_cells() {
        let mut r = Renderer::new(20, 3);
        let region = Region {
            row: 1,
            col: 1,
            width: 20,
            height: 3,
            bg: palette::BG,
            padding: Padding::horizontal(2),
        };
        r.define("test", region);
        let lines = vec![Line::new(smallvec![Span::new("hi", palette::FG)])];
        r.set_lines("test", &lines);
        let reg = r.slots[0].region.clone();
        Renderer::paint_iter(&mut r.buf, &reg, lines.iter());
        assert!(!r.buf.get(0, 0).flags.is_content()); // padding
        assert!(r.buf.get(0, 2).flags.is_content()); // content
    }

    #[test]
    fn selection_swaps_colors() {
        let mut r = Renderer::new(20, 3);
        let region = Region {
            row: 1,
            col: 1,
            width: 20,
            height: 3,
            bg: palette::BG,
            padding: Padding::zero(),
        };
        r.define("main", region);
        let lines = vec![Line::new(smallvec![Span::new("hello", palette::FG)])];
        r.set_lines("main", &lines);
        let reg = r.slots[0].region.clone();
        Renderer::paint_iter(&mut r.buf, &reg, lines.iter());
        let fg_before = r.buf.get(0, 0).fg;
        let bg_before = r.buf.get(0, 0).bg;
        r.apply_selection(SelectionRange {
            r0: 1,
            c0: 1,
            r1: 1,
            c1: 3,
        });
        assert_eq!(r.buf.get(0, 0).fg, bg_before);
        assert_eq!(r.buf.get(0, 0).bg, fg_before);
    }

    #[test]
    fn user_bg_extends_to_padding() {
        let mut r = Renderer::new(20, 3);
        let region = Region {
            row: 1,
            col: 1,
            width: 20,
            height: 3,
            bg: palette::BG,
            padding: Padding::horizontal(2),
        };
        r.define("test", region);
        let user_line = Line {
            spans: smallvec![Span::new("hi", palette::FG)],
            bg: Some(palette::USER_BG),
            margin: false,
            indent: 0,
            bleed: 0,
        };
        let reg = r.slots[0].region.clone();
        Renderer::paint_iter(&mut r.buf, &reg, [user_line].iter());
        // Padding cell at col 0 should have user bg
        assert_eq!(r.buf.get(0, 0).bg, palette::USER_BG);
        // Content cell should also have user bg
        assert_eq!(r.buf.get(0, 2).bg, palette::USER_BG);
    }
}
