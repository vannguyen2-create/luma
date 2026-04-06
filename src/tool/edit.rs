/// Edit tool — find-and-replace in files with safety checks.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;



/// Edits files by exact string replacement.
pub struct EditTool;

impl Tool for EditTool {
    fn name(&self) -> &str { "Edit" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "Edit".into(),
            description: concat!(
                "Performs exact string replacement in an existing file.\n",
                "- You MUST read the file before editing. Never edit code you haven't seen.\n",
                "- old_string must match exactly one location, or set replace_all=true.\n",
                "- old_string and new_string must be different.\n",
                "- Preserve exact indentation from the file when specifying old_string.\n",
                "- Use replace_all for renaming variables/strings across the file.\n",
                "- To create new files, use the `write` tool instead.",
            ).into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the file. ALWAYS generate this argument first." },
                    "old_string": { "type": "string", "description": "Exact string to find" },
                    "new_string": { "type": "string", "description": "Replacement string" },
                    "replace_all": { "type": "boolean", "description": "Replace all occurrences" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    fn execute(
        &self,
        args: serde_json::Value,
        output_tx: mpsc::Sender<String>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            let replace_all = args.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);

            if path_str.is_empty() { bail!("missing path argument"); }
            if old == new { bail!("old_string and new_string are identical"); }

            let path = PathBuf::from(path_str);

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) if old.is_empty() => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&path, new)?;
                    let diff = crate::tool::diff::make_diff("", new);
                    for line in &diff {
                        let _ = output_tx.try_send(format!("{line}\n"));
                    }
                    return Ok(format!("Created {}", path.display()));
                }
                Err(_) => bail!("file not found — {}", path.display()),
            };

            if !content.contains(old) {
                bail!("old_string not found in file");
            }

            let count = content.matches(old).count();
            if count > 1 && !replace_all {
                bail!("found {count} matches. Set replace_all=true or provide more context.");
            }

            let updated = if replace_all {
                content.replace(old, new)
            } else {
                content.replacen(old, new, 1)
            };

            std::fs::write(&path, &updated)?;

            // Send context-based diff to UI (no LCS needed — we know exact position)
            let diff = crate::tool::diff::make_edit_diff(&content, old, new, replace_all);
            for line in &diff {
                let _ = output_tx.try_send(format!("{line}\n"));
            }

            let old_lines = old.lines().count() as isize;
            let new_lines = new.lines().count() as isize;
            let delta = new_lines - old_lines;
            let sign = if delta >= 0 { "+" } else { "" };

            Ok(format!(
                "Edited {} ({} replacement{}, {sign}{delta} lines)",
                path.display(), count, if count > 1 { "s" } else { "" }
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn edit_single_replace() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let tool = EditTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "old_string": "hello", "new_string": "hi"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("Edited"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hi world");
    }

    #[tokio::test]
    async fn edit_multiple_without_flag_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "aaa bbb aaa").unwrap();

        let tool = EditTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "old_string": "aaa", "new_string": "x"}),
            tx, cancel,
        ).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn edit_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "aaa bbb aaa").unwrap();

        let tool = EditTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "old_string": "aaa", "new_string": "x", "replace_all": true}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("2 replacements"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x bbb x");
    }

    #[tokio::test]
    async fn edit_create_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.txt");

        let tool = EditTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "old_string": "", "new_string": "content"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("Created"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "content");
    }


}
