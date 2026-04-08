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

/// Resolves image id → base64 data. Passed to providers so they don't touch filesystem.
pub type ImageResolver = dyn Fn(&str) -> String + Send + Sync;

/// An LLM provider that streams responses as Events. Object-safe.
#[allow(dead_code)] // Trait methods implemented by all providers but not called via dyn dispatch
pub trait Provider: Send + Sync {
    /// Provider display name (e.g. "claude", "openai").
    fn name(&self) -> &str;

    /// Current thinking level.
    fn thinking(&self) -> ThinkingLevel;

    /// Set thinking level.
    fn set_thinking(&mut self, level: ThinkingLevel);

    /// Build native schemas for server capabilities this provider supports.
    fn server_tool_schemas(&self, capabilities: &[String]) -> Vec<serde_json::Value> {
        let _ = capabilities;
        vec![]
    }

    /// Stream a chat completion. `resolve_image` maps image id → base64 string.
    fn stream<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSchema],
        server_tools: &'a [serde_json::Value],
        resolve_image: &'a ImageResolver,
        tx: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<StreamResponse>> + Send + 'a>>;
}
