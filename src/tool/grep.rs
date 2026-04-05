/// Grep tool — fast content search with regex, native Rust.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::Result;
use std::path::Path;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_RESULTS: usize = 250;
const MAX_LINE_LEN: usize = 500;

/// Native grep tool using regex + directory walking.
pub struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_owned(),
            description: concat!(
                "Search file contents using regex. Returns file paths with line numbers and matching lines.\n",
                "- Regex syntax: \"log.*Error\", \"fn\\s+\\w+\", \"TODO|FIXME\"\n",
                "- Filter by file type with include param: \"*.rs\", \"*.{ts,tsx}\"\n",
                "- Results sorted by modification time (newest first).\n",
                "- Not for finding files by name — use `glob` instead.",
            ).to_owned(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for in file contents"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in. Defaults to current working directory."
                    },
                    "include": {
                        "type": "string",
                        "description": "File glob pattern to filter (e.g. \"*.rs\", \"*.{ts,tsx}\")"
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
            let pattern_str = args["pattern"].as_str().unwrap_or("");
            let path = args["path"].as_str().unwrap_or(".");
            let include = args["include"].as_str();

            let root = if Path::new(path).is_absolute() {
                path.to_owned()
            } else {
                std::env::current_dir()?.join(path).to_string_lossy().into_owned()
            };

            let re = regex::Regex::new(pattern_str)
                .map_err(|e| anyhow::anyhow!("Invalid regex: {e}"))?;

            let file_filter = include.and_then(|g| {
                globset::GlobBuilder::new(g)
                    .literal_separator(false)
                    .build()
                    .ok()
                    .map(|g| g.compile_matcher())
            });

            let mut matches: Vec<FileMatch> = Vec::new();
            search_dir(&root, &root, &re, file_filter.as_ref(), &mut matches, MAX_RESULTS)?;

            // Sort by mtime descending
            matches.sort_by(|a, b| b.mtime.cmp(&a.mtime));

            let mut lines = Vec::new();
            for m in &matches {
                let _ = output_tx.send(format!("{}:{}", m.path, m.line_num)).await;
                let line = if m.line.len() > MAX_LINE_LEN {
                    format!("{}:{}:{}...", m.path, m.line_num, &m.line[..MAX_LINE_LEN])
                } else {
                    format!("{}:{}:{}", m.path, m.line_num, m.line)
                };
                lines.push(line);
            }

            if lines.is_empty() {
                return Ok("No matches found".to_owned());
            }

            let truncated = lines.len() >= MAX_RESULTS;
            let mut result = lines.join("\n");
            if truncated {
                result.push_str("\n(Results truncated. Use a more specific pattern or path.)");
            }
            Ok(result)
        })
    }
}

struct FileMatch {
    path: String,
    line_num: usize,
    line: String,
    mtime: std::time::SystemTime,
}

fn search_dir(
    root: &str,
    dir: &str,
    re: &regex::Regex,
    file_filter: Option<&globset::GlobMatcher>,
    results: &mut Vec<FileMatch>,
    limit: usize,
) -> Result<()> {
    let root_path = Path::new(root);
    search_recursive(root_path, Path::new(dir), re, file_filter, results, limit)
}

fn search_recursive(
    root: &Path,
    dir: &Path,
    re: &regex::Regex,
    file_filter: Option<&globset::GlobMatcher>,
    results: &mut Vec<FileMatch>,
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

        if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
            continue;
        }

        if path.is_dir() {
            search_recursive(root, &path, re, file_filter, results, limit)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);

            // Check file filter
            if let Some(filter) = file_filter
                && !filter.is_match(rel)
            {
                continue;
            }

            // Skip binary/large files
            let meta = match path.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.len() > 1_000_000 {
                continue;
            }

            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue, // skip binary/unreadable
            };

            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let rel_str = rel.to_string_lossy().into_owned();

            for (i, line) in content.lines().enumerate() {
                if results.len() >= limit {
                    break;
                }
                if re.is_match(line) {
                    results.push(FileMatch {
                        path: rel_str.clone(),
                        line_num: i + 1,
                        line: line.to_owned(),
                        mtime,
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grep_finds_pattern() {
        let (tx, _rx) = mpsc::channel(64);
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "fn main", "path": "src", "include": "*.rs"});
        let result = tool.execute(args, tx, CancellationToken::new()).await.unwrap();
        assert!(result.contains("fn main"));
    }

    #[tokio::test]
    async fn grep_no_match() {
        let dir = std::env::temp_dir().join("luma_grep_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        let (tx, _rx) = mpsc::channel(64);
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "ZZZZZ_NONEXISTENT", "path": dir.to_str().unwrap()});
        let result = tool.execute(args, tx, CancellationToken::new()).await.unwrap();
        assert_eq!(result, "No matches found");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn grep_with_include() {
        let (tx, _rx) = mpsc::channel(64);
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "pub struct", "include": "*.rs"});
        let result = tool.execute(args, tx, CancellationToken::new()).await.unwrap();
        assert!(result.contains("pub struct"));
    }
}
