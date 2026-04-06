/// Prompt rendering — input lines, dropdown, image indicators.
use super::completion::{dropdown_line, highlight_at_refs};
use crate::tui::text::{Line, Span};
use crate::tui::theme::palette;
use smallvec::smallvec;

impl super::PromptState {
    /// Render the prompt lines.
    pub fn lines(&self) -> Vec<Line> {
        if let Some(pasted) = &self.paste {
            return self.render_paste(pasted);
        }
        let ghost = self.ghost();
        let line_count = self.buffer.lines().count();
        if line_count > 1 {
            return self.render_multiline(line_count);
        }
        let mut spans = highlight_at_refs(&self.buffer);
        if !ghost.is_empty() {
            spans.push(Span::new(ghost, palette::MUTED));
        }
        let mut lines = vec![Line::new(spans)];
        if !self.images.is_empty() {
            lines.push(self.render_image_indicators());
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

    fn render_paste(&self, pasted: &str) -> Vec<Line> {
        let n = pasted.lines().count();
        let first = pasted.lines().next().unwrap_or("");
        let preview = if first.len() > 30 {
            format!("{}...", &first[..30])
        } else {
            first.to_owned()
        };
        vec![
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
        ]
    }

    fn render_multiline(&self, line_count: usize) -> Vec<Line> {
        let last_line = self.buffer.lines().last().unwrap_or("");
        vec![
            Line::new(smallvec![Span::new(last_line.to_owned(), palette::FG)]),
            Line::new(smallvec![
                Span::new(format!("{line_count} lines "), palette::DIM),
                Span::new("enter", palette::ACCENT),
                Span::new(" send  ", palette::DIM),
                Span::new("esc", palette::ACCENT),
                Span::new(" clear", palette::DIM),
            ]),
        ]
    }

    fn render_image_indicators(&self) -> Line {
        let labels: Vec<String> = self
            .images
            .iter()
            .map(|img| format!("[{}]", img.label))
            .collect();
        Line::new(smallvec![
            Span::new(labels.join(" "), palette::DIM),
            Span::new("  alt+v", palette::ACCENT),
            Span::new(" add image", palette::DIM),
        ])
    }
}
