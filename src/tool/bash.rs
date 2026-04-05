/// Bash tool — execute shell commands with streaming output and timeout.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::{bail, Result};
use std::pin::Pin;
use std::future::Future;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_OUTPUT: usize = 32_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Dangerous command patterns — checked as substrings.
const DANGEROUS_SUBSTR: &[&str] = &[
    "rm -rf /", "rm --no-preserve-root",
    "git push --force", "git reset --hard",
];

/// Dangerous base commands — matched only at command position
/// (start of string or after `&&`, `||`, `;`, `|`).
const DANGEROUS_CMDS: &[&str] = &["mkfs", "dd"];

/// Execute shell commands with streaming output.
pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "bash".into(),
            description: concat!(
                "Execute a shell command. Returns stdout + stderr.\n",
                "Usage:\n",
                "- Use absolute paths and avoid cd when possible\n",
                "- Always quote file paths that contain spaces\n",
                "- For reading files: use the read tool instead of cat/head/tail\n",
                "- For editing files: use the edit tool instead of sed/awk\n",
                "- For writing files: use the write tool instead of echo/cat\n",
                "- For file search: use bash with find/fd\n",
                "- For content search: use bash with grep/rg\n",
                "- Multiple independent commands: use separate tool calls in parallel\n",
                "- Dependent commands: chain with && in a single call\n",
                "- Optional timeout in ms (default 30000, max 120000)",
            ).into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout": { "type": "number", "description": "Timeout in ms (default 30000)" }
                },
                "required": ["command"]
            }),
        }
    }

    fn execute(
        &self,
        args: serde_json::Value,
        output_tx: mpsc::Sender<String>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if command.is_empty() { bail!("missing command argument"); }

            let timeout_ms = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_TIMEOUT_MS);

            for pattern in DANGEROUS_SUBSTR {
                if command.contains(pattern) {
                    bail!("blocked dangerous command — {command}");
                }
            }
            if is_dangerous_cmd(command) {
                bail!("blocked dangerous command — {command}");
            }

            let mut child = tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let mut stdout = child.stdout.take().expect("stdout piped");
            let mut stderr = child.stderr.take().expect("stderr piped");

            // Read stdout + stderr with cancel/timeout support
            let (output, exit_code) = {
                let mut out = String::new();
                let mut err = String::new();
                let mut stdout_buf = [0u8; 4096];
                let mut stderr_buf = [0u8; 4096];
                let mut aborted = false;
                let mut timed_out = false;
                let mut stdout_done = false;
                let mut stderr_done = false;

                loop {
                    if stdout_done && stderr_done {
                        break;
                    }
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => { aborted = true; break; }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)) => { timed_out = true; break; }
                        n = stdout.read(&mut stdout_buf), if !stdout_done => {
                            let n = n?;
                            if n == 0 { stdout_done = true; continue; }
                            let chunk = String::from_utf8_lossy(&stdout_buf[..n]).to_string();
                            out.push_str(&chunk);
                            let _ = output_tx.send(chunk).await;
                            if out.len() > MAX_OUTPUT {
                                out.truncate(MAX_OUTPUT);
                                out.push_str("\n[truncated]");
                                break;
                            }
                        }
                        n = stderr.read(&mut stderr_buf), if !stderr_done => {
                            let n = n?;
                            if n == 0 { stderr_done = true; continue; }
                            let chunk = String::from_utf8_lossy(&stderr_buf[..n]).to_string();
                            err.push_str(&chunk);
                            let _ = output_tx.send(chunk).await;
                        }
                    }
                }

                if aborted || timed_out {
                    child.kill().await.ok();
                    if aborted { out.push_str("\n[aborted]"); }
                    if timed_out { out.push_str("\n[timeout]"); }
                    (out, if aborted { 130 } else { 124 })
                } else {
                    let status = child.wait().await?;
                    if !err.is_empty() {
                        out.push_str(&err);
                    }
                    (out, status.code().unwrap_or(1))
                }
            };

            let mut result_str = output;
            if exit_code != 0 && !result_str.contains("[exit code:") {
                result_str.push_str(&format!("\n[exit code: {exit_code}]"));
            }

            if result_str.trim().is_empty() { Ok("(no output)".into()) } else { Ok(result_str) }
        })
    }
}

/// Check if any segment of a piped/chained command starts with a dangerous command.
fn is_dangerous_cmd(command: &str) -> bool {
    for segment in command.split(&['|', ';'][..]) {
        // Split on && and || too
        for part in segment.split("&&").flat_map(|s| s.split("||")) {
            let base = part.split_whitespace().next().unwrap_or("");
            if DANGEROUS_CMDS.iter().any(|&cmd| base == cmd || base.starts_with(&format!("{cmd}."))) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bash_echo() {
        let tool = BashTool;
        let (tx, mut rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"command": "echo hello"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("hello"));
        // Should have received streaming output
        let chunk = rx.try_recv();
        assert!(chunk.is_ok());
    }

    #[tokio::test]
    async fn bash_exit_code() {
        let tool = BashTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"command": "exit 42"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("[exit code: 42]"));
    }

    #[tokio::test]
    async fn bash_dangerous_blocked() {
        let tool = BashTool;
        let (tx, _rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let result = tool.execute(
            serde_json::json!({"command": "rm -rf /"}),
            tx, cancel,
        ).await;

        assert!(result.is_err());
    }

    #[test]
    fn dangerous_cmd_detection() {
        // Should block
        assert!(is_dangerous_cmd("dd if=/dev/zero of=/dev/sda"));
        assert!(is_dangerous_cmd("mkfs.ext4 /dev/sda"));
        assert!(is_dangerous_cmd("echo x && dd if=a of=b"));
        assert!(is_dangerous_cmd("echo x | dd of=/dev/null"));

        // Should NOT block
        assert!(!is_dangerous_cmd("git add ."));
        assert!(!is_dangerous_cmd("git add -A && git commit -m 'msg'"));
        assert!(!is_dangerous_cmd("git commit -m 'added feature'"));
        assert!(!is_dangerous_cmd("echo add something"));
        assert!(!is_dangerous_cmd("addr2line -e binary"));
    }

    #[tokio::test]
    async fn bash_cancel() {
        let tool = BashTool;
        let (tx, _rx) = mpsc::channel(32);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            cancel_clone.cancel();
        });

        let result = tool.execute(
            serde_json::json!({"command": "sleep 10"}),
            tx, cancel,
        ).await.unwrap();

        assert!(result.contains("[aborted]"));
    }
}
