/// Read tool — read files or list directories with line numbers.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::{bail, Result};
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use std::future::Future;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const DEFAULT_LIMIT: usize = 2000;

/// Reads files with line numbers or lists directory contents.
pub struct ReadTool;

impl Tool for ReadTool {
    fn name(&self) -> &str { "read" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "read".into(),
            description: concat!(
                "Read a file or list a directory. Returns content with line numbers (e.g. '1: content').\n",
                "- Path must be absolute.\n",
                "- Default reads up to 2000 lines. Use offset/limit for large files.\n",
                "- Avoid tiny repeated slices (e.g. 30-line chunks). Read a larger window instead.\n",
                "- Call in parallel for multiple files you need to read.\n",
                "- For directories, returns entries with trailing / for subdirectories.\n",
                "- Not for searching — use `grep` for content search, `glob` for file search.",
            ).into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to read" },
                    "offset": { "type": "number", "description": "Start line (1-indexed)" },
                    "limit": { "type": "number", "description": "Max lines (default 2000)" }
                },
                "required": ["path"]
            }),
        }
    }

    fn execute(
        &self,
        args: serde_json::Value,
        _output_tx: mpsc::Sender<String>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path_str.is_empty() { bail!("missing path argument"); }

            let path = PathBuf::from(path_str).canonicalize().unwrap_or_else(|_| PathBuf::from(path_str));
            let meta = fs::metadata(&path)?;

            if meta.is_dir() {
                let entries: Vec<String> = fs::read_dir(&path)?
                    .filter_map(|e| e.ok())
                    .map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            format!("{name}/")
                        } else { name }
                    })
                    .collect();
                return Ok(entries.join("\n"));
            }

            let content = fs::read_to_string(&path)?;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_LIMIT as u64) as usize;

            let mut result = String::new();
            let mut count = 0;

            for (i, line) in content.lines().enumerate() {
                let line_num = i + 1;
                if line_num < offset { continue; }
                if count >= limit {
                    result.push_str(&format!("[{line_num}+ more lines]\n"));
                    break;
                }
                result.push_str(&format!("{line_num}: {line}\n"));
                count += 1;
            }

            if result.is_empty() { Ok("(empty file)".into()) } else { Ok(result) }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn read_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\n").unwrap();

        let tool = ReadTool;
        let (tx, _rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap()}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("1: line1"));
        assert!(result.contains("3: line3"));
    }

    #[tokio::test]
    async fn read_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let tool = ReadTool;
        let (tx, _rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": dir.path().to_str().unwrap()}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("a.txt"));
        assert!(result.contains("sub/"));
    }

    #[tokio::test]
    async fn read_with_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.txt");
        let mut f = std::fs::File::create(&file).unwrap();
        for i in 1..=100 { writeln!(f, "line {i}").unwrap(); }

        let tool = ReadTool;
        let (tx, _rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"path": file.to_str().unwrap(), "offset": 50, "limit": 5}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("50: line 50"));
        assert!(result.contains("54: line 54"));
        assert!(result.contains("more lines"));
    }
}
