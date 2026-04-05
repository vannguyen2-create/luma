// Tool call summaries — one-line descriptions for UI display.

/// Build a one-line summary for a tool call (used in ToolStart and history replay).
pub fn format_tool_summary(name: &str, args: &serde_json::Value) -> String {
    match name.to_lowercase().as_str() {
        "read" | "read_file" => args
            .get("path")
            .or_else(|| args.get("filePath"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        "write" | "write_file" => {
            let path = args
                .get("path")
                .or_else(|| args.get("filePath"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let lines = args
                .get("content")
                .and_then(|v| v.as_str())
                .map(|c| c.lines().count())
                .unwrap_or(0);
            format!("{path} ({lines} lines)")
        }
        "edit" | "apply_patch" => args
            .get("path")
            .or_else(|| args.get("filePath"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        "glob" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("**/*")
            .to_owned(),
        "grep" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            match args.get("include").and_then(|v| v.as_str()) {
                Some(inc) => format!("{pattern} ({inc})"),
                None => pattern.to_owned(),
            }
        }
        "bash" | "exec_command" | "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if cmd.chars().count() > 60 {
                let truncated: String = cmd.chars().take(57).collect();
                format!("$ {truncated}...")
            } else {
                format!("$ {cmd}")
            }
        }
        _ => args
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        let vs = v.to_string();
                        let s = v.as_str().unwrap_or(&vs);
                        let s = if s.chars().count() > 40 {
                            let t: String = s.chars().take(40).collect();
                            format!("{t}…")
                        } else {
                            s.to_owned()
                        };
                        format!("{k}={s}")
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default(),
    }
}

/// Build a result summary for tool completion (shown after ToolEnd).
pub fn format_tool_result(name: &str, result: &str) -> String {
    let lower = name.to_lowercase();
    if matches!(lower.as_str(), "bash" | "exec_command" | "shell") {
        if let Some(cap) = result.rfind("[exit code: ") {
            let code = &result[cap + 12..result.len().min(cap + 16)].trim_end_matches(']');
            if *code != "0" {
                return format!("exit {code}");
            }
        }
        return String::new();
    }
    let lines = result.lines().count();
    // Read/glob/grep: parenthesized line count for inline display
    if matches!(lower.as_str(), "read" | "read_file" | "glob" | "grep") {
        if lines > 1 {
            return format!("({lines} lines)");
        }
        return String::new();
    }
    // Write/edit: empty (diff shown in block body)
    if matches!(
        lower.as_str(),
        "write" | "write_file" | "edit" | "create_file" | "apply_patch"
    ) {
        return String::new();
    }
    if lines <= 1 {
        result.chars().take(80).collect()
    } else {
        format!("{lines} lines")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_bash() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(format_tool_summary("bash", &args), "$ echo hello");
    }
    #[test]
    fn summary_read() {
        let args = serde_json::json!({"path": "/tmp/test.rs"});
        assert_eq!(format_tool_summary("read", &args), "/tmp/test.rs");
    }
    #[test]
    fn result_bash_error() {
        assert_eq!(
            format_tool_result("bash", "error\n[exit code: 1]"),
            "exit 1"
        );
    }
    #[test]
    fn summary_wire_names() {
        let args = serde_json::json!({"path": "/tmp/test.rs"});
        assert_eq!(format_tool_summary("Read", &args), "/tmp/test.rs");
        assert_eq!(format_tool_summary("read_file", &args), "/tmp/test.rs");
        let args = serde_json::json!({"command": "ls"});
        assert_eq!(format_tool_summary("exec_command", &args), "$ ls");
    }
    #[test]
    fn summary_glob_grep() {
        let args = serde_json::json!({"pattern": "**/*.rs"});
        assert_eq!(format_tool_summary("glob", &args), "**/*.rs");
        let args = serde_json::json!({"pattern": "fn main", "include": "*.rs"});
        assert_eq!(format_tool_summary("grep", &args), "fn main (*.rs)");
    }
    #[test]
    fn result_multiline() {
        assert_eq!(
            format_tool_result("read", "line1\nline2\nline3"),
            "(3 lines)"
        );
    }
    #[test]
    fn result_read_single() {
        assert_eq!(format_tool_result("read", "only one line"), "");
    }
    #[test]
    fn result_write_empty() {
        assert_eq!(format_tool_result("write", "file written"), "");
    }
    #[test]
    fn result_glob() {
        assert_eq!(format_tool_result("glob", "a\nb\nc"), "(3 lines)");
    }
}
