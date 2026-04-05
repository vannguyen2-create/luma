/// Core types shared across agent, provider, and tool modules.
use serde::{Deserialize, Serialize};

/// A chat message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool invocation requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: ToolCallFunction,
}

/// The function name and arguments of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// JSON schema for tool parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Thinking budget level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Low,
    Medium,
    High,
}

impl ThinkingLevel {
    /// Budget in tokens for this thinking level.
    pub const fn budget(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 1024,
            Self::Medium => 4096,
            Self::High => 8192,
        }
    }

    /// Cycle to next level.
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Off,
        }
    }
}

/// Token usage from a provider response.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_cycle() {
        assert_eq!(ThinkingLevel::Off.next(), ThinkingLevel::Low);
        assert_eq!(ThinkingLevel::Low.next(), ThinkingLevel::Medium);
        assert_eq!(ThinkingLevel::Medium.next(), ThinkingLevel::High);
        assert_eq!(ThinkingLevel::High.next(), ThinkingLevel::Off);
    }

    #[test]
    fn thinking_level_budget() {
        assert_eq!(ThinkingLevel::Off.budget(), 0);
        assert_eq!(ThinkingLevel::High.budget(), 8192);
    }

    #[test]
    fn message_serializes() {
        let msg = Message {
            role: Role::User,
            content: "hello".into(),
            tool_call_id: None,
            tool_calls: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(!json.contains("tool_call_id"));
    }

    #[test]
    fn usage_default() {
        let u = Usage::default();
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.output_tokens, 0);
    }
}
