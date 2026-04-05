/// Trait for agent tools (read, write, bash, edit).
use crate::core::types::ToolSchema;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A tool the agent can invoke. Dyn-compatible via boxed future.
pub trait Tool: Send + Sync {
    /// Tool name as seen by the model.
    fn name(&self) -> &str;

    /// JSON schema for the model to call this tool.
    fn schema(&self) -> ToolSchema;

    /// Execute the tool with parsed arguments. Streams incremental output
    /// into `output_tx`. Returns the full result string.
    fn execute(
        &self,
        args: serde_json::Value,
        output_tx: mpsc::Sender<String>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>>;
}
