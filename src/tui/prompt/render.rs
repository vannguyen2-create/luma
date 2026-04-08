/// Prompt rendering — input line with inline segment chips, dropdown.
use super::buffer::Seg;
use super::completion::{dropdown_line, highlight_at_refs};
use crate::tui::text::{Line, Span};
use crate::tui::theme::palette;
use smallvec::smallvec;

impl super::PromptState {
    /// Render the prompt input line.
    pub fn lines(&self) -> Vec<Line> {
        let line_count = self.buf.line_count();
        if line_count > 1 {
            let mut spans = smallvec![
                Span::with_bg(format!(" ~{line_count} lines "), palette::BG, palette::DIM),
                Span::new(" ".to_owned(), palette::FG),
            ];
            spans.extend(render_segs_inline(
                &self.buf.segs,
                Some(&self.buf.last_line()),
            ));
            return vec![Line::new(spans)];
        }

        let mut spans = render_segs_inline(&self.buf.segs, None);
        let ghost = self.ghost();
        if !ghost.is_empty() {
            spans.push(Span::new(ghost, palette::MUTED));
        }
        vec![Line::new(spans)]
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
}

/// Render segments as inline spans. If `last_line_only` is set, only that text.
fn render_segs_inline(segs: &[Seg], last_line_only: Option<&str>) -> smallvec::SmallVec<[Span; 4]> {
    let mut spans = smallvec![];
    let mut img_n = 0;

    for seg in segs {
        match seg {
            Seg::Text(t) => {
                let text = if let Some(ll) = last_line_only {
                    ll
                } else {
                    t.as_str()
                };
                if !text.is_empty() {
                    spans.extend(highlight_at_refs(text));
                }
                if last_line_only.is_some() {
                    // Only render last line text, skip rest
                    continue;
                }
            }
            Seg::Image { .. } => {
                img_n += 1;
                spans.push(Span::with_bg(
                    format!(" Image {img_n} "),
                    palette::BG,
                    palette::FILE_REF,
                ));
                spans.push(Span::new(" ".to_owned(), palette::FG));
            }
            Seg::Paste(text) => {
                let n = text.lines().count();
                spans.push(Span::with_bg(
                    format!(" Pasted ~{n} lines "),
                    palette::BG,
                    palette::WARN,
                ));
                spans.push(Span::new(" ".to_owned(), palette::FG));
            }
        }
    }
    if spans.is_empty() {
        spans.push(Span::new(String::new(), palette::FG));
    }
    spans
}
