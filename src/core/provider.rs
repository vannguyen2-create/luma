/// Trait for LLM providers. Streams events into the shared channel.
use crate::core::types::{Message, ThinkingLevel, ToolSchema, Usage};
use crate::event::Event;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Response from a provider stream: assistant message + token usage.
pub type StreamResponse = (Message, Usage);

/// An LLM provider that streams responses as Events. Object-safe.
pub trait Provider: Send + Sync {
    /// Provider display name (e.g. "claude", "openai").
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Current thinking level.
    #[allow(dead_code)]
    fn thinking(&self) -> ThinkingLevel;

    /// Set thinking level.
    fn set_thinking(&mut self, level: ThinkingLevel);

    /// Build native schemas for server capabilities this provider supports.
    /// Each provider maps capability names to its own API format.
    /// Unknown capabilities are silently ignored.
    fn server_tool_schemas(&self, capabilities: &[String]) -> Vec<serde_json::Value> {
        let _ = capabilities;
        vec![]
    }

    /// Stream a chat completion. Sends Token/Thinking/ToolStart/ToolEnd/Usage
    /// events into `tx`. Returns the final assistant message + usage.
    fn stream<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSchema],
        server_tools: &'a [serde_json::Value],
        tx: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<StreamResponse>> + Send + 'a>>;
}
