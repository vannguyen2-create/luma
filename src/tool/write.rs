/// Write tool — write content to a file, creating parent dirs if needed.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::{bail, Result};
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Writes content to a file.
pub struct WriteTool;

impl Tool for WriteTool {
    fn name(&self) -> &str { "write" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "write".into(),
            description: concat!(
                "Write content to a file. Creates parent directories if needed.\n",
                "Usage:\n",
                "- This tool will overwrite the existing file\n",
                "- If this is an existing file, you MUST read it first with the read tool\n",
                "- Prefer the edit tool for modifying existing files — it only sends the diff\n",
                "- Only use write to create new files or for complete rewrites\n",
                "- NEVER create documentation files (*.md) or README files unless explicitly requested",
            ).into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to write" },
                    "content": { "type": "string", "description": "Content to write" }
                },
                "required": ["path", "content"]
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
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if path_str.is_empty() { bail!("missing path argument"); }

            let path = PathBuf::from(path_str);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Read old content for diff (if file exists)
            let old = std::fs::read_to_string(&path).unwrap_or_default();
            std::fs::write(&path, content)?;

            // Send diff lines to UI
            let diff = crate::tool::diff::make_diff(&old, content);
            for line in &diff {
                let _ = output_tx.try_send(format!("{line}\n"));
            }

            let total_lines = content.lines().count();
            Ok(format!("Wrote {} ({total_lines} lines)", path.display()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out.txt");

        let tool = WriteTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "content": "hello"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("Wrote"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello");
    }

    #[tokio::test]
    async fn write_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a/b/c.txt");

        let tool = WriteTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "content": "deep"}),
            tx, cancel,
        ).await.unwrap();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), "deep");
    }


}
