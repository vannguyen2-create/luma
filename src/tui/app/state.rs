/// App state — enums and sub-structs for App decomposition.
use crate::config::models::{AgentMode, ModelEntry};
use crate::core::types::ThinkingLevel;
use crate::event::AgentCommand;
use crate::tui::output::OutputLog;
use crate::tui::picker::Picker;
use crate::tui::prompt::PromptState;
use crate::tui::selection::Selection;
use crate::tui::status::StatusBar;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Main state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Streaming,
    PendingAbort,
    Aborting,
}

/// Dragging interaction state.
pub enum DragState {
    Scrollbar { start_row: u16, start_offset: usize },
    Selecting,
}

/// Picker mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    Model,
    Session,
}

/// Agent-facing configuration: mode, model, thinking, env context.
pub struct AppConfig {
    pub mode: AgentMode,
    pub model: Option<ModelEntry>,
    pub env_context: String,
    pub thinking: ThinkingLevel,
    pub picker_mode: PickerMode,
}

/// Agent loop handle: channel, cancellation, turn tracking.
pub struct AgentHandle {
    pub tx: Option<mpsc::Sender<AgentCommand>>,
    pub cancel: Option<CancellationToken>,
    pub turn_start: Option<std::time::Instant>,
    pub state: RunState,
    pub pending_input: Option<String>,
    pub abort_countdown: u8,
}

impl AgentHandle {
    /// Create idle agent handle.
    pub fn new() -> Self {
        Self {
            tx: None,
            cancel: None,
            turn_start: None,
            state: RunState::Idle,
            pending_input: None,
            abort_countdown: 0,
        }
    }
}

/// UI component state: output, prompt, picker, status, selection.
pub struct UiState {
    pub output: OutputLog,
    pub prompt: PromptState,
    pub picker: Picker,
    pub status: StatusBar,
    pub selection: Selection,
    pub drag: Option<DragState>,
    pub last_output_width: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_handle_new() {
        let h = AgentHandle::new();
        assert_eq!(h.state, RunState::Idle);
        assert!(h.tx.is_none());
    }

    #[test]
    fn run_state_values() {
        assert_ne!(RunState::Idle, RunState::Streaming);
        assert_ne!(RunState::Streaming, RunState::PendingAbort);
        assert_ne!(RunState::PendingAbort, RunState::Aborting);
    }
}
