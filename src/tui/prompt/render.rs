/// Prompt rendering — input lines, dropdown, inline indicators.
use super::completion::{dropdown_line, highlight_at_refs};
use crate::tui::text::{Line, Span};
use crate::tui::theme::palette;
use smallvec::smallvec;

impl super::PromptState {
    /// Render the prompt lines.
    pub fn lines(&self) -> Vec<Line> {
        let mut spans = smallvec![];

        // Inline image indicators: [Image 1] [Image 2]
        for (i, _img) in self.images.iter().enumerate() {
            spans.push(Span::with_bg(
                format!(" Image {} ", i + 1),
                palette::BG,
                palette::FILE_REF,
            ));
            spans.push(Span::new(" ".to_owned(), palette::FG));
        }

        // Paste preview mode
        if let Some(pasted) = &self.paste {
            let n = pasted.lines().count();
            spans.push(Span::with_bg(
                format!(" Pasted ~{n} lines "),
                palette::BG,
                palette::WARN,
            ));
            return vec![
                Line::new(spans),
                Line::new(smallvec![
                    Span::new("enter", palette::ACCENT),
                    Span::new(" send  ", palette::DIM),
                    Span::new("esc", palette::ACCENT),
                    Span::new(" cancel", palette::DIM),
                ]),
            ];
        }

        // Multiline buffer
        let ghost = self.ghost();
        let line_count = self.buffer.lines().count();
        if line_count > 1 {
            let last_line = self.buffer.lines().last().unwrap_or("");
            spans.push(Span::new(last_line.to_owned(), palette::FG));
            return vec![
                Line::new(spans),
                Line::new(smallvec![
                    Span::new(format!("{line_count} lines "), palette::DIM),
                    Span::new("enter", palette::ACCENT),
                    Span::new(" send  ", palette::DIM),
                    Span::new("esc", palette::ACCENT),
                    Span::new(" clear", palette::DIM),
                ]),
            ];
        }

        // Normal single-line with @path highlighting
        let text_spans = highlight_at_refs(&self.buffer);
        spans.extend(text_spans);
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
