//! HTML-to-markdown conversion utilities for WebFetch.

use regex::Regex;

/// Convert raw response body to markdown based on content type.
pub fn convert_to_markdown(raw: &str, content_type: &str) -> String {
    if content_type.contains("text/html") || content_type.contains("application/xhtml") {
        html_to_markdown(raw)
    } else if content_type.contains("application/json") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
            let pretty = serde_json::to_string_pretty(&parsed).unwrap_or_default();
            format!("```json\n{pretty}\n```")
        } else {
            raw.to_owned()
        }
    } else {
        raw.to_owned()
    }
}

/// Extract primary content from HTML and convert to simple markdown.
fn html_to_markdown(html: &str) -> String {
    let primary = extract_primary_html(html);
    let cleaned = strip_tags(&primary);
    collapse_whitespace(&cleaned)
}

/// Extract main content region from HTML.
fn extract_primary_html(html: &str) -> String {
    if let Some(c) = extract_tag_content(html, "main") {
        return c;
    }
    if let Some(c) = extract_tag_content(html, "article") {
        return c;
    }
    if let Some(c) = extract_role_main(html) {
        return c;
    }
    strip_html_boilerplate(html)
}

/// Extract content from element with role="main".
fn extract_role_main(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let role_pos = lower
        .find("role=\"main\"")
        .or_else(|| lower.find("role='main'"))?;
    let tag_start = html[..role_pos].rfind('<')?;
    let after_lt = &html[tag_start + 1..];
    let tag_name: String = after_lt
        .chars()
        .take_while(|c| c.is_alphanumeric())
        .collect();
    if tag_name.is_empty() {
        return None;
    }
    let close_lower = format!("</{}>", tag_name.to_lowercase());
    let content_start = html[tag_start..].find('>')? + tag_start + 1;
    let end = lower[content_start..].find(&close_lower)?;
    Some(html[tag_start..content_start + end + close_lower.len()].to_owned())
}

/// Extract content of the first occurrence of a tag.
fn extract_tag_content(html: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let lower = html.to_lowercase();
    let start = lower.find(&open)?;
    let after_open = html[start..].find('>')? + start + 1;
    let end = lower[after_open..].find(&close)?;
    Some(html[after_open..after_open + end].to_owned())
}

/// Remove known boilerplate tags and class-based boilerplate divs.
fn strip_html_boilerplate(html: &str) -> String {
    // Lowercase once, remove tags from both in sync
    let lower = html.to_lowercase();
    let mut result = html.to_owned();
    let mut result_lower = lower;
    for tag in &["script", "style", "svg", "nav", "header", "footer", "aside"] {
        let (r, rl) = remove_tag_blocks_paired(&result, &result_lower, tag);
        result = r;
        result_lower = rl;
    }
    use std::sync::LazyLock;
    static DIV_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
        Regex::new(
            r#"(?is)<div\b[^>]*\b(?:class|id)=["'][^"']*(sidebar|toc|table-of-contents|navigation|menu|navbar|footer|header)[^"']*["'][^>]*>[\s\S]*?</div>"#,
        ).ok()
    });
    if let Some(re) = DIV_RE.as_ref() {
        result = re.replace_all(&result, "").into_owned();
    }
    result
}

/// Remove tag blocks using pre-computed lowercase for O(1) case comparison.
fn remove_tag_blocks_paired(html: &str, lower: &str, tag: &str) -> (String, String) {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut result = String::with_capacity(html.len());
    let mut result_lower = String::with_capacity(lower.len());
    let mut pos = 0;
    while let Some(start) = lower[pos..].find(&open) {
        let abs_start = pos + start;
        result.push_str(&html[pos..abs_start]);
        result_lower.push_str(&lower[pos..abs_start]);
        if let Some(end) = lower[abs_start..].find(&close) {
            pos = abs_start + end + close.len();
            if let Some(gt) = html[pos..].find('>') {
                pos += gt + 1;
            }
        } else {
            pos = html.len();
        }
    }
    result.push_str(&html[pos..]);
    result_lower.push_str(&lower[pos..]);
    (result, result_lower)
}

/// Strip HTML tags, converting to markdown equivalents.
fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag_buf = String::new();
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if ch == '>' && in_tag {
            in_tag = false;
            apply_tag_markdown(&tag_buf, &mut result);
        } else if in_tag {
            tag_buf.push(ch);
        } else {
            result.push(ch);
        }
    }
    decode_entities_in_place(&mut result);
    result
}

/// Emit markdown equivalent for an HTML tag.
fn apply_tag_markdown(tag_body: &str, out: &mut String) {
    let lower = tag_body.to_lowercase();
    let name = lower.split_whitespace().next().unwrap_or("");
    match name {
        "br" | "br/" => out.push('\n'),
        "p" | "div" | "tr" | "blockquote" => out.push('\n'),
        "/p" | "/div" | "/tr" | "/blockquote" => out.push('\n'),
        "li" => out.push_str("\n- "),
        "/li" => {}
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            out.push('\n');
            let level = name.as_bytes().get(1).map(|b| b - b'0').unwrap_or(1);
            for _ in 0..level {
                out.push('#');
            }
            out.push(' ');
        }
        "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => out.push('\n'),
        "strong" | "b" => out.push_str("**"),
        "/strong" | "/b" => out.push_str("**"),
        "em" | "i" => out.push('*'),
        "/em" | "/i" => out.push('*'),
        "code" => out.push('`'),
        "/code" => out.push('`'),
        "pre" => out.push_str("\n```\n"),
        "/pre" => out.push_str("\n```\n"),
        "hr" | "hr/" => out.push_str("\n---\n"),
        _ if name == "a" => {
            if let Some(href) = extract_attr(tag_body, "href") {
                out.push_str(&format!("[link]({href}) "));
            }
        }
        _ => {}
    }
}

/// Extract an attribute value from a tag body.
fn extract_attr(tag_body: &str, attr: &str) -> Option<String> {
    let lower = tag_body.to_lowercase();
    let pattern = format!("{attr}=\"");
    let start = lower.find(&pattern)? + pattern.len();
    let end = lower[start..].find('"')? + start;
    Some(tag_body[start..end].to_owned())
}

/// Decode common HTML entities in-place (single pass, no extra allocations).
fn decode_entities_in_place(text: &mut String) {
    // Only allocate a new string if we find any entity
    if !text.contains('&') {
        return;
    }
    let src = std::mem::take(text);
    text.reserve(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some((decoded, skip)) = try_decode_entity(&src[i..])
        {
            text.push_str(decoded);
            i += skip;
            continue;
        }
        text.push(bytes[i] as char);
        i += 1;
    }
}

/// Try to decode an HTML entity at the start of `s`. Returns (replacement, bytes consumed).
fn try_decode_entity(s: &str) -> Option<(&'static str, usize)> {
    const ENTITIES: &[(&str, &str)] = &[
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&nbsp;", " "),
    ];
    for &(entity, replacement) in ENTITIES {
        if s.starts_with(entity) {
            return Some((replacement, entity.len()));
        }
    }
    None
}

/// Collapse runs of blank lines to at most two.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut blank_count = 0u32;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(trimmed);
            result.push('\n');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_basic_html() {
        let md = strip_tags("<p>Hello <b>world</b></p>");
        assert!(md.contains("Hello"));
        assert!(md.contains("**world**"));
    }

    #[test]
    fn extract_main_content() {
        let html = "<html><nav>menu</nav><main><p>content</p></main></html>";
        let primary = extract_primary_html(html);
        assert!(primary.contains("<p>content</p>"));
        assert!(!primary.contains("menu"));
    }

    #[test]
    fn extract_role_main_content() {
        let html = r#"<div role="main"><p>main content</p></div>"#;
        assert!(extract_role_main(html).unwrap().contains("main content"));
    }

    #[test]
    fn decode_entities_works() {
        let mut s = "&amp; &lt;b&gt;".to_owned();
        decode_entities_in_place(&mut s);
        assert_eq!(s, "& <b>");
    }

    #[test]
    fn decode_no_entities_noop() {
        let mut s = "hello world".to_owned();
        let ptr = s.as_ptr();
        decode_entities_in_place(&mut s);
        // No reallocation when no entities
        assert_eq!(s.as_ptr(), ptr);
    }

    #[test]
    fn collapse_blank_lines() {
        let result = collapse_whitespace("a\n\n\n\n\nb\n");
        assert!(result.matches('\n').count() <= 4);
    }

    #[test]
    fn json_conversion() {
        let md = convert_to_markdown(r#"{"key":"value"}"#, "application/json");
        assert!(md.contains("```json"));
    }

    #[test]
    fn remove_script_tags() {
        let cleaned = strip_html_boilerplate("<div>hello<script>alert(1)</script>world</div>");
        assert!(cleaned.contains("hello"));
        assert!(!cleaned.contains("alert"));
    }

    #[test]
    fn heading_conversion() {
        assert!(strip_tags("<h2>Title</h2>").contains("## Title"));
    }

    #[test]
    fn bold_italic_list() {
        let md = strip_tags("<strong>b</strong><em>i</em><ul><li>x</li></ul>");
        assert!(md.contains("**b**"));
        assert!(md.contains("*i*"));
        assert!(md.contains("- x"));
    }

    #[test]
    fn strip_boilerplate_divs() {
        let cleaned = strip_html_boilerplate(r#"<div>keep</div><div class="sidebar">remove</div>"#);
        assert!(cleaned.contains("keep"));
        assert!(!cleaned.contains("remove"));
    }
}
