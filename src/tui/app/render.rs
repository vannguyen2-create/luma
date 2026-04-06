/// App rendering — mouse handling, screen composition, scrollbar, selection.
use super::state::{DragState, RunState};
use super::{Action, SCROLL_STEP};
use crate::event::{MouseButton, MouseEvent};
use crate::tui::renderer::{CursorState, Overlay};
use crate::tui::selection;
use crate::tui::text::{Line, Span};
use crate::tui::theme::{icon, palette};
use smallvec::smallvec;

impl super::App {
    /// Handle mouse input — scroll, drag, selection, click-to-expand.
    pub(super) fn on_mouse(&mut self, ev: MouseEvent) -> Action {
        let r_row = self.layout.output.row;
        let r_height = self.layout.output.height;
        let r_width = self.layout.output.width;
        let in_output = |row: u16| row >= r_row && row < r_row + r_height;
        let i_row = self.layout.input.row;
        let i_height = self.layout.input.height;
        let in_input = |row: u16| row >= i_row && row < i_row + i_height;
        let (total, visible, _) = self.ui.output.scroll_info();
        let has_sb = total > visible;
        let sb_col = self.layout.output.col + r_width - 1;

        match ev {
            MouseEvent::ScrollUp { row, .. } if in_output(row) => {
                self.ui.output.scroll_up(SCROLL_STEP);
                Action::Render
            }
            MouseEvent::ScrollDown { row, .. } if in_output(row) => {
                self.ui.output.scroll_down(SCROLL_STEP);
                Action::Render
            }
            MouseEvent::Press {
                button: MouseButton::Left, row, col,
            } if in_output(row) || in_input(row) => {
                if in_output(row) && has_sb && col >= sb_col {
                    let (_, _, offset) = self.ui.output.scroll_info();
                    self.ui.drag = Some(DragState::Scrollbar {
                        start_row: row,
                        start_offset: offset,
                    });
                } else {
                    self.ui.selection.begin(row, col);
                    self.ui.drag = Some(DragState::Selecting);
                }
                Action::Continue
            }
            MouseEvent::Drag {
                button: MouseButton::Left, row, col,
            } => {
                match &self.ui.drag {
                    Some(DragState::Scrollbar {
                        start_row, start_offset,
                    }) if has_sb => {
                        let start_row = *start_row;
                        let start_offset = *start_offset;
                        let delta = row as i32 - start_row as i32;
                        let max_off = total.saturating_sub(visible);
                        let thumb_h = (visible * visible / total).max(1);
                        let track_h = visible.saturating_sub(thumb_h);
                        if track_h > 0 {
                            let sd = (delta as f64 / track_h as f64
                                * max_off as f64)
                                .round() as isize;
                            self.ui.output.scroll_to(
                                (start_offset as isize + sd).max(0) as usize,
                            );
                        }
                        Action::Render
                    }
                    Some(DragState::Selecting) => {
                        self.ui.selection.update(row, col);
                        self.ui.selection.edge_scroll(
                            &mut self.ui.output, r_row, r_height,
                        );
                        Action::Render
                    }
                    _ => Action::Continue,
                }
            }
            MouseEvent::Release {
                button: MouseButton::Left, row, ..
            } => {
                let was_selecting =
                    matches!(self.ui.drag, Some(DragState::Selecting));
                self.ui.drag = None;

                if was_selecting {
                    if let Some((r0, c0, r1, c1)) = self.ui.selection.finish()
                    {
                        selection::copy_from_buffer(
                            self.renderer.buffer(), r0, c0, r1, c1,
                        );
                        return Action::Render;
                    } else if in_output(row) {
                        let rr = self.layout.output.row as usize;
                        if let Some(idx) =
                            self.ui.output.hit_test_block(row as usize, rr)
                            && self.ui.output.toggle_expand(idx)
                        {
                            return Action::Render;
                        }
                    }
                }
                Action::Continue
            }
            _ => Action::Continue,
        }
    }

    /// Handle terminal resize.
    pub(super) fn handle_resize(&mut self, w: u16, h: u16) {
        self.layout = super::compute_layout(w, h);
        self.renderer.set_term_size(w, h);
        self.renderer.update_region("output", self.layout.output.clone());
        self.renderer.update_region("status", self.layout.status.clone());
        self.renderer.update_region("input", self.layout.input.clone());
        self.ui.output.set_size(
            self.layout.output.content_width() as usize,
            self.layout.output.content_height() as usize,
        );
        self.renderer.clear_screen();
    }

    /// Compose all regions and flush to terminal.
    pub(super) fn render(&mut self) {
        // Two-phase: reconcile all dirty state once before composing.
        self.ui.output.prepare_frame();

        let content_w = self.layout.output.content_width();
        let content_h = self.layout.output.content_height();
        let (total, visible, _) = self.ui.output.scroll_info();
        let has_sb = total > visible;
        let ow = if has_sb { content_w - 1 } else { content_w };
        if ow != self.ui.last_output_width {
            self.ui.output.set_size(ow as usize, content_h as usize);
            self.ui.last_output_width = ow;
            self.ui.output.prepare_frame();
        }

        let dropdown = self.ui.prompt.dropdown();
        let picker_lines = self.ui.picker.lines(content_h as usize);
        let needs_overlay = !picker_lines.is_empty() || !dropdown.is_empty();

        if needs_overlay {
            let vis = self.ui.output.visible_lines().to_vec();
            let full_h = self.layout.output.height as usize;
            let composited = if !picker_lines.is_empty() {
                super::composite_overlay(&vis, &picker_lines, full_h)
            } else {
                super::composite_overlay(&vis, &dropdown, full_h)
            };
            // Temporarily remove bottom padding so overlay fills the full region
            self.renderer.set_bottom_padding("output", 0);
            self.renderer.set_lines("output", &composited);
        } else {
            self.renderer.set_bottom_padding("output", self.layout.output.padding.bottom);
            // From viewport iterator — no intermediate Vec
            let iter = self.ui.output.visible_iter();
            self.renderer.set_lines_iter("output", iter);
        }
        let hint_w = self.layout.status.content_width() as usize;
        let status_line = if self.agent.state == RunState::PendingAbort {
            Line::new(smallvec![
                Span::new("esc", palette::WARN),
                Span::new(" again to interrupt", palette::DIM),
            ])
        } else {
            self.ui.status.hint_line(hint_w)
        };
        self.renderer.set_lines("status", &[status_line]);

        let prompt_lines = self.ui.prompt.lines();
        let mode_line = self.ui.status.mode_line();

        let bar = icon::PROMPT;
        let bar_empty =
            Line::new(smallvec![Span::deco(bar.to_owned(), palette::ACCENT)]);
        let total_h = self.layout.input.height as usize;
        let mut input_lines = Vec::with_capacity(total_h);

        input_lines.push(bar_empty.clone());
        for pl in &prompt_lines {
            let mut spans =
                smallvec![Span::deco(format!("{bar}  "), palette::ACCENT)];
            spans.extend(pl.spans.iter().cloned());
            input_lines.push(Line::new(spans));
        }

        let mut mode_spans =
            smallvec![Span::deco(format!("{bar}  "), palette::ACCENT)];
        mode_spans.extend(mode_line.spans.iter().cloned());
        let mode = Line::new(mode_spans);

        let used = input_lines.len();
        for _ in used..total_h.saturating_sub(2) {
            input_lines.push(bar_empty.clone());
        }
        input_lines.push(mode);

        let transition = Line::new(smallvec![
            Span::deco_colored("╹".to_owned(), palette::ACCENT, palette::BG),
            Span::deco_colored(
                "▀".repeat(
                    (self.layout.input.width as usize).saturating_sub(1),
                ),
                palette::SURFACE,
                palette::BG,
            ),
        ]);
        input_lines.push(transition);

        self.renderer.set_lines("input", &input_lines);
        self.update_scrollbar();
        self.update_selection_highlight();

        let ir = &self.layout.input;
        let cursor_col = ir.col + 3 + self.ui.prompt.cursor_column() as u16;
        let cursor_row = ir.row + 1;
        if self.ui.prompt.has_paste()
            || self.agent.state == RunState::PendingAbort
        {
            self.renderer.set_cursor(CursorState::Hidden);
        } else {
            self.renderer.set_cursor(CursorState::Visible {
                row: cursor_row,
                col: cursor_col,
            });
        }
        let _ = self.renderer.flush();
    }

    fn update_selection_highlight(&mut self) {
        use crate::tui::renderer::SelectionRange;
        if self.ui.selection.is_active && self.ui.selection.has_range() {
            let (mut r0, mut c0, mut r1, mut c1) = (
                self.ui.selection.start_row,
                self.ui.selection.start_col,
                self.ui.selection.end_row,
                self.ui.selection.end_col,
            );
            if r0 > r1 || (r0 == r1 && c0 > c1) {
                std::mem::swap(&mut r0, &mut r1);
                std::mem::swap(&mut c0, &mut c1);
            }
            self.renderer
                .set_selection(Some(SelectionRange { r0, c0, r1, c1 }));
        } else {
            self.renderer.set_selection(None);
        }
    }

    fn update_scrollbar(&mut self) {
        use crate::tui::renderer::ScrollCell;

        let (total, visible, offset) = self.ui.output.scroll_info();
        if total <= visible {
            self.renderer.set_overlay(None);
            return;
        }
        let r = &self.layout.output;
        let track = visible; // cells

        // Compute thumb in 1/8th cell units (subcells) for smooth positioning.
        const SUB: usize = 8;
        let track_sub = track * SUB;
        let thumb_sub = (track_sub * visible / total).max(SUB); // min 1 cell
        let max_off = total.saturating_sub(visible);
        let scroll_sub = track_sub.saturating_sub(thumb_sub);
        let start_sub = if max_off > 0 {
            offset * scroll_sub / max_off
        } else {
            0
        };
        let end_sub = start_sub + thumb_sub;

        let mut cells = Vec::with_capacity(track);
        for i in 0..track {
            let cell_start = i * SUB;
            let cell_end = cell_start + SUB;

            if cell_end <= start_sub || cell_start >= end_sub {
                // Entirely outside thumb
                cells.push(ScrollCell::Track);
            } else if cell_start >= start_sub && cell_end <= end_sub {
                // Entirely inside thumb
                cells.push(ScrollCell::Thumb);
            } else if cell_start < start_sub && cell_end > start_sub {
                // Top edge: thumb starts partway through this cell
                let frac = (start_sub - cell_start) as u8;
                if frac == 0 {
                    cells.push(ScrollCell::Thumb);
                } else {
                    cells.push(ScrollCell::TopEdge(frac));
                }
            } else {
                // Bottom edge: thumb ends partway through this cell
                let frac = (end_sub - cell_start) as u8;
                if frac >= SUB as u8 {
                    cells.push(ScrollCell::Thumb);
                } else {
                    cells.push(ScrollCell::BottomEdge(frac));
                }
            }
        }

        self.renderer.set_overlay(Some(Overlay {
            row: r.row,
            col: r.col + r.width - 1,
            fg_thumb: palette::DIM,
            fg_track: palette::BORDER,
            cells,
        }));
    }
}
