/// Glob tool — fast file pattern matching, native Rust.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::Result;
use std::path::Path;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_RESULTS: usize = 200;

/// Native glob tool using globset + directory walking.
pub struct GlobTool;

impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_owned(),
            description: concat!(
                "Find files by name pattern. Returns paths sorted by modification time (newest first).\n",
                "- Patterns: **/*.rs, src/**/*.{ts,tsx}, *test*, src/[a-z]*/*.ts\n",
                "- Not for searching file contents — use `grep` instead.",
            ).to_owned(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern to match files (e.g. \"**/*.rs\", \"src/**/*.{ts,tsx}\")"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in. Defaults to current working directory."
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    fn execute(
        &self,
        args: serde_json::Value,
        output_tx: mpsc::Sender<String>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let pattern = args["pattern"].as_str().unwrap_or("**/*");
            let path = args["path"].as_str().unwrap_or(".");
            let root = if Path::new(path).is_absolute() {
                path.to_owned()
            } else {
                std::env::current_dir()?.join(path).to_string_lossy().into_owned()
            };

            let matcher = globset::GlobBuilder::new(pattern)
                .literal_separator(false)
                .build()
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern: {e}"))?
                .compile_matcher();

            let mut matches: Vec<(String, std::time::SystemTime)> = Vec::new();

            walk_dir(&root, &matcher, &mut matches, MAX_RESULTS)?;

            // Sort by mtime descending (newest first)
            matches.sort_by(|a, b| b.1.cmp(&a.1));

            let truncated = matches.len() >= MAX_RESULTS;
            let files: Vec<String> = matches.into_iter().map(|(p, _)| p).collect();

            for f in &files {
                let _ = output_tx.send(f.clone()).await;
            }

            let mut result = files.join("\n");
            if truncated {
                result.push_str("\n(Results truncated. Use a more specific pattern.)");
            }
            if files.is_empty() {
                result = "No files found".to_owned();
            }
            Ok(result)
        })
    }
}

fn walk_dir(
    dir: &str,
    matcher: &globset::GlobMatcher,
    results: &mut Vec<(String, std::time::SystemTime)>,
    limit: usize,
) -> Result<()> {
    let root = Path::new(dir);
    walk_recursive(root, root, matcher, results, limit)
}

fn walk_recursive(
    root: &Path,
    dir: &Path,
    matcher: &globset::GlobMatcher,
    results: &mut Vec<(String, std::time::SystemTime)>,
    limit: usize,
) -> Result<()> {
    if results.len() >= limit {
        return Ok(());
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        if results.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        // Skip hidden dirs and common noise
        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        if path.is_dir() {
            walk_recursive(root, &path, matcher, results, limit)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            if matcher.is_match(rel) {
                let mtime = path.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                let display = rel.to_string_lossy().into_owned();
                results.push((display, mtime));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn glob_finds_rust_files() {
        let (tx, _rx) = mpsc::channel(64);
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src"});
        let result = tool.execute(args, tx, CancellationToken::new()).await.unwrap();
        assert!(result.contains(".rs"));
    }

    #[tokio::test]
    async fn glob_no_match() {
        let (tx, _rx) = mpsc::channel(64);
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "**/*.nonexistent_ext"});
        let result = tool.execute(args, tx, CancellationToken::new()).await.unwrap();
        assert_eq!(result, "No files found");
    }
}
