/// Terminal cell — the atomic unit of the screen buffer.
use super::{CellFlag, Rgb};

/// A single terminal cell with color and metadata.
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: CellFlag,
}

impl Cell {
    /// Create a space cell with given background.
    pub fn blank(bg: Rgb) -> Self {
        Self {
            ch: ' ',
            fg: Rgb(0, 0, 0),
            bg,
            flags: CellFlag(CellFlag::NONE),
        }
    }

    /// Create a padding cell (space with PADDING flag).
    pub fn padding(bg: Rgb) -> Self {
        Self {
            ch: ' ',
            fg: Rgb(0, 0, 0),
            bg,
            flags: CellFlag(CellFlag::PADDING),
        }
    }
}

/// Render a slice of cells into an ANSI string with SGR color tracking.
pub fn render_cells_ansi(cells: &[Cell]) -> String {
    use std::fmt::Write;
    if cells.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(cells.len() * 4);
    let mut cur_fg = Rgb(255, 255, 255);
    let mut cur_bg = Rgb(0, 0, 0);
    let mut cur_bold = false;
    let mut cur_italic = false;
    out.push_str("\x1b[0m");

    for cell in cells {
        // Skip trailing half of wide characters — terminal already advanced
        if cell.flags.is_wide_pad() {
            continue;
        }
        let need_bold = cell.flags.is_bold();
        let need_italic = cell.flags.is_italic();
        if cell.bg != cur_bg {
            let _ = write!(out, "\x1b[48;2;{};{};{}m", cell.bg.0, cell.bg.1, cell.bg.2);
            cur_bg = cell.bg;
        }
        if cell.fg != cur_fg {
            let _ = write!(out, "\x1b[38;2;{};{};{}m", cell.fg.0, cell.fg.1, cell.fg.2);
            cur_fg = cell.fg;
        }
        if need_bold && !cur_bold {
            out.push_str("\x1b[1m");
            cur_bold = true;
        } else if !need_bold && cur_bold {
            out.push_str("\x1b[22m");
            cur_bold = false;
        }
        if need_italic && !cur_italic {
            out.push_str("\x1b[3m");
            cur_italic = true;
        } else if !need_italic && cur_italic {
            out.push_str("\x1b[23m");
            cur_italic = false;
        }
        out.push(cell.ch);
    }
    out.push_str("\x1b[0m");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::palette;

    #[test]
    fn cell_blank() {
        let c = Cell::blank(palette::BG);
        assert_eq!(c.ch, ' ');
        assert!(c.flags.is_content());
    }

    #[test]
    fn cell_padding_flag() {
        let c = Cell::padding(palette::BG);
        assert!(!c.flags.is_content());
    }
}
