/// Central event type. All input (keyboard, mouse, resize) and agent output
/// flow through a single `mpsc::channel<Event>`. The app loop matches exhaustively.
use crate::core::types::Usage;

/// A keyboard or control key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    CtrlC,
    CtrlT,
    CtrlA,
    CtrlE,
    CtrlU,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Paste(String),
    AltEnter,
    AltV,
}

/// Mouse button identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    None,
}

/// A parsed mouse event from SGR mode 1002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseEvent {
    Press {
        button: MouseButton,
        row: u16,
        col: u16,
    },
    Release {
        button: MouseButton,
        row: u16,
        col: u16,
    },
    Drag {
        button: MouseButton,
        row: u16,
        col: u16,
    },
    ScrollUp {
        row: u16,
        col: u16,
    },
    ScrollDown {
        row: u16,
        col: u16,
    },
}

/// A single web search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Every event the app loop handles.
#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize {
        w: u16,
        h: u16,
    },

    Token(String),
    Thinking(String),
    ToolStart {
        name: String,
        summary: String,
    },
    /// Streaming tool input args (e.g. file content being written).
    ToolInput {
        name: String,
        chunk: String,
    },
    ToolOutput {
        name: String,
        chunk: String,
    },
    ToolEnd {
        name: String,
        summary: String,
    },
    /// Server-side web search started.
    WebSearchStart {
        query: String,
    },
    /// Server-side web search completed.
    WebSearchDone {
        query: String,
        results: Vec<SearchHit>,
    },
    SkillStart(String),
    SkillEnd(String),
    Usage(Usage),
    AgentDone,
    AgentError(String),

    Tick,
}

/// An image attachment — raw bytes, saved by agent to session dir.
pub struct ImageAttach {
    pub media_type: String,
    pub data: Vec<u8>,
}

/// A file reference attached to a message (content read at send time).
pub struct FileAttach {
    pub path: String,
    pub content: String,
}

/// Commands sent from App to the agent loop task.
pub enum AgentCommand {
    /// Run a user turn. Agent pushes user message, calls provider, runs tools.
    Chat {
        text: String,
        images: Vec<ImageAttach>,
        files: Vec<FileAttach>,
        cancel: tokio_util::sync::CancellationToken,
    },
    /// Reset conversation (new thread). Clears all non-system messages.
    Reset,
    /// Switch model (agent rebuilds provider with auth on next turn).
    SetModel { model_id: String, source: String },
    /// Update thinking level on current provider.
    SetThinking(crate::core::types::ThinkingLevel),
    /// Load a saved session (replace current messages, usage, and session ID).
    LoadSession {
        session: crate::core::session::Session,
    },
    /// Shut down the agent loop.
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Event>();
    }

    #[test]
    fn key_event_eq() {
        assert_eq!(KeyEvent::Char('a'), KeyEvent::Char('a'));
        assert_ne!(KeyEvent::Char('a'), KeyEvent::Char('b'));
    }

    #[test]
    fn mouse_event_variants() {
        let press = MouseEvent::Press {
            button: MouseButton::Left,
            row: 1,
            col: 1,
        };
        let release = MouseEvent::Release {
            button: MouseButton::Left,
            row: 1,
            col: 1,
        };
        assert_ne!(press, release);
    }
}
