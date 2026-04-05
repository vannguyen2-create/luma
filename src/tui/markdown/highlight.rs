/// Syntax highlighting for code blocks — keyword-based coloring.
use crate::tui::text::Span;
use crate::tui::theme::{palette, Rgb};
use smallvec::SmallVec;

const KEYWORDS: &[&str] = &[
    "import",
    "export",
    "from",
    "const",
    "let",
    "var",
    "function",
    "class",
    "return",
    "if",
    "else",
    "for",
    "while",
    "async",
    "await",
    "fn",
    "pub",
    "mut",
    "impl",
    "struct",
    "trait",
    "use",
    "mod",
    "type",
    "interface",
    "true",
    "false",
    "null",
    "undefined",
    "None",
    "self",
    "def",
];

/// Highlight a code line into styled spans.
pub fn highlight_code(code: &str) -> SmallVec<[Span; 4]> {
    let mut spans: SmallVec<[Span; 4]> = SmallVec::new();
    if let Some(idx) = find_comment_start(code) {
        if idx > 0 {
            highlight_tokens(&code[..idx], &mut spans);
        }
        spans.push(Span::italic(code[idx..].to_owned(), palette::DIM));
        return spans;
    }
    highlight_tokens(code, &mut spans);
    spans
}

fn find_comment_start(code: &str) -> Option<usize> {
    if let Some((i, _)) = code.match_indices("//").next() {
        return Some(i);
    }
    code.match_indices('#')
        .map(|(i, _)| i)
        .find(|&i| i == 0 || code.as_bytes()[i - 1] == b' ')
}

fn highlight_tokens(code: &str, spans: &mut SmallVec<[Span; 4]>) {
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < bytes.len() {
        // String literals
        if bytes[pos] == b'"' || bytes[pos] == b'\'' {
            let quote = bytes[pos];
            let start = pos;
            pos += 1;
            while pos < bytes.len() && bytes[pos] != quote {
                if bytes[pos] == b'\\' {
                    pos += 1;
                }
                pos += 1;
            }
            if pos < bytes.len() {
                pos += 1;
            }
            spans.push(Span::new(code[start..pos].to_owned(), Rgb(166, 227, 161)));
            continue;
        }

        // Numbers
        if bytes[pos].is_ascii_digit() {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_digit() || bytes[pos] == b'.') {
                pos += 1;
            }
            spans.push(Span::new(code[start..pos].to_owned(), Rgb(250, 179, 135)));
            continue;
        }

        // Identifiers / keywords
        if bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_' {
            let start = pos;
            while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                pos += 1;
            }
            let word = &code[start..pos];
            let color = if KEYWORDS.contains(&word) {
                Rgb(198, 160, 246)
            } else {
                palette::FG
            };
            spans.push(Span::new(word.to_owned(), color));
            continue;
        }

        // Whitespace
        if bytes[pos] == b' ' || bytes[pos] == b'\t' {
            let start = pos;
            while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
                pos += 1;
            }
            spans.push(Span::new(code[start..pos].to_owned(), palette::FG));
            continue;
        }

        // Multi-byte UTF-8
        if bytes[pos] >= 0x80 {
            let start = pos;
            while pos < bytes.len() && bytes[pos] >= 0x80 {
                pos += 1;
            }
            while pos < bytes.len() && bytes[pos].is_ascii_alphanumeric() {
                pos += 1;
            }
            spans.push(Span::new(code[start..pos].to_owned(), palette::FG));
            continue;
        }

        // Punctuation/operators
        let ch = bytes[pos] as char;
        let color = match ch {
            '{' | '}' | '(' | ')' | '[' | ']' | ';' | ',' | '.' => palette::DIM,
            '=' | '+' | '-' | '*' | '/' | '<' | '>' | '!' | '&' | '|' | ':' => Rgb(137, 180, 250),
            _ => palette::FG,
        };
        spans.push(Span::new(String::from(ch), color));
        pos += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_basic() {
        let spans = highlight_code("let x = 42;");
        assert!(spans.len() > 1);
    }
}
