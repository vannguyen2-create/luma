/// Block types — content blocks for the conversation document.
/// Pure data. No render logic.
mod chrome;
pub mod diff;
mod render;
mod text;
mod tool;

pub use render::{RenderState, render_block};

use crate::core::types::ContentBlock;
use crate::tui::stream::StreamBuf;

/// A content block in the conversation document.
#[derive(Debug, Clone)]
pub enum Block {
    Gap,
    GapLabel(String),
    Info(String),
    Error(String),
    Warn(String),
    User(Vec<ContentBlock>),
    Thinking(StreamBuf),
    Text(TextBlock),
    Tool(ToolBlock),
    Skill(SkillBlock),
}

/// Content-group discriminant for auto_gap logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Chrome,
    Thinking,
    Text,
    Tool,
    Skill,
}

impl Block {
    /// Whether this block carries conversational content (for gap insertion).
    pub fn is_content(&self) -> bool {
        matches!(
            self.kind(),
            BlockKind::Thinking | BlockKind::Text | BlockKind::Tool | BlockKind::Skill
        )
    }

    /// Whether two blocks belong to the same content group (no gap needed between them).
    pub fn same_content_group(&self, other: &Block) -> bool {
        let a = self.kind();
        let b = other.kind();
        // Thinking → Text is one group (no gap).
        if a == BlockKind::Thinking && b == BlockKind::Text {
            return true;
        }
        if a == BlockKind::Text && b == BlockKind::Thinking {
            return true;
        }
        // Same kind stays grouped only for Tool sequences.
        a == b && a == BlockKind::Tool
    }

    /// Content-group discriminant.
    fn kind(&self) -> BlockKind {
        match self {
            Block::Thinking(_) => BlockKind::Thinking,
            Block::Text(_) => BlockKind::Text,
            Block::Tool(_) => BlockKind::Tool,
            Block::Skill(_) => BlockKind::Skill,
            _ => BlockKind::Chrome,
        }
    }
}

/// Assistant text block — StreamBuf only. No render cache.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub stream: StreamBuf,
}

impl TextBlock {
    /// Create a new empty text block.
    pub fn new() -> Self {
        Self {
            stream: StreamBuf::new(),
        }
    }

    /// Feed a streaming token.
    pub fn feed(&mut self, token: &str) {
        self.stream.feed(token);
    }

    /// Flush partial into committed.
    pub fn flush(&mut self) {
        self.stream.flush();
    }

    /// Whether there's any content.
    pub fn is_empty(&self) -> bool {
        self.stream.is_empty()
    }
}

/// Tool invocation block.
#[derive(Debug, Clone)]
pub struct ToolBlock {
    pub name: String,
    pub summary: String,
    pub output: Vec<String>,
    pub stream: Option<StreamBuf>,
    pub is_done: bool,
    pub end_summary: String,
    pub is_expanded: bool,
}

impl ToolBlock {
    /// Create a streaming tool block (active tool call).
    pub fn streaming(name: &str, summary: &str) -> Self {
        Self {
            name: name.to_owned(),
            summary: summary.to_owned(),
            output: Vec::new(),
            stream: Some(StreamBuf::new()),
            is_done: false,
            end_summary: String::new(),
            is_expanded: false,
        }
    }

    /// Create a completed tool block (history replay).
    pub fn history(name: &str, summary: &str) -> Self {
        Self {
            name: name.to_owned(),
            summary: summary.to_owned(),
            output: Vec::new(),
            stream: None,
            is_done: true,
            end_summary: String::new(),
            is_expanded: false,
        }
    }
}

/// Skill activation block.
#[derive(Debug, Clone)]
pub struct SkillBlock {
    pub name: String,
    pub is_done: bool,
    pub end_summary: String,
}

/// Fingerprint for pull-based dirty detection in Layout.
/// Two equal snapshots mean no re-render needed.
#[derive(Clone)]
pub enum Snapshot {
    /// Always re-render (active spinner, etc.).
    Volatile,
    /// Never changes after creation.
    Immutable,
    /// Streaming content — track committed + partial lengths.
    Stream { committed: usize, partial: usize },
    /// Tool with output — track completion and expand state.
    Tool { output_len: usize, expanded: bool },
    /// Skill — track completion.
    Skill { is_done: bool },
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Volatile, _) | (_, Self::Volatile) => false,
            (Self::Immutable, Self::Immutable) => true,
            (
                Self::Stream {
                    committed: a,
                    partial: b,
                },
                Self::Stream {
                    committed: c,
                    partial: d,
                },
            ) => a == c && b == d,
            (
                Self::Tool {
                    output_len: a,
                    expanded: b,
                },
                Self::Tool {
                    output_len: c,
                    expanded: d,
                },
            ) => a == c && b == d,
            (Self::Skill { is_done: a }, Self::Skill { is_done: b }) => a == b,
            _ => false,
        }
    }
}

impl Block {
    /// Snapshot for dirty detection. Layout compares old vs new.
    pub fn snapshot(&self) -> Snapshot {
        match self {
            Block::Gap
            | Block::GapLabel(_)
            | Block::Info(_)
            | Block::Error(_)
            | Block::Warn(_)
            | Block::User(_) => Snapshot::Immutable,
            Block::Thinking(s) => Snapshot::Stream {
                committed: s.committed.len(),
                partial: s.partial().len(),
            },
            Block::Text(tb) => Snapshot::Stream {
                committed: tb.stream.committed.len(),
                partial: tb.stream.partial().len(),
            },
            Block::Tool(tb) if tb.is_done => Snapshot::Tool {
                output_len: tb.output.len(),
                expanded: tb.is_expanded,
            },
            Block::Tool(_) => Snapshot::Volatile,
            Block::Skill(sb) => Snapshot::Skill {
                is_done: sb.is_done,
            },
        }
    }
}

#[cfg(test)]
mod tests;
