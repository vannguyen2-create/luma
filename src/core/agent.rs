/// Agent loop — actor that owns messages, provider, registry.
/// Receives commands from App, streams events back.
mod summary;
mod turn;

pub use summary::format_tool_summary;

use crate::core::registry::Registry;
use crate::core::session::Session;
use crate::core::types::{Message, Role, ThinkingLevel};
use crate::event::{AgentCommand, Event};
use tokio::sync::mpsc;

/// Configuration for spawning an agent loop.
pub struct AgentConfig {
    pub model_id: String,
    pub source: String,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
}

/// Spawn the agent loop task. Returns a command sender.
pub fn spawn(
    config: AgentConfig,
    registry: Registry,
    event_tx: mpsc::Sender<Event>,
) -> mpsc::Sender<AgentCommand> {
    let (cmd_tx, cmd_rx) = mpsc::channel(16);
    let tx = event_tx.clone();
    tokio::spawn(async move {
        let result = std::panic::AssertUnwindSafe(
            agent_loop(config, registry, cmd_rx, event_tx)
        );
        if futures::FutureExt::catch_unwind(result).await.is_err() {
            let _ = tx.send(Event::AgentError("agent task panicked".into())).await;
        }
    });
    cmd_tx
}

async fn agent_loop(
    mut config: AgentConfig,
    registry: Registry,
    mut cmd_rx: mpsc::Receiver<AgentCommand>,
    event_tx: mpsc::Sender<Event>,
) {
    let mut session = Session::new();

    if !config.system_prompt.is_empty() {
        session.messages.push(Message {
            role: Role::System,
            content: config.system_prompt.clone(),
            tool_call_id: None,
            tool_calls: None,
        });
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AgentCommand::Chat { text, cancel } => {
                session.messages.push(Message {
                    role: Role::User,
                    content: text,
                    tool_call_id: None,
                    tool_calls: None,
                });

                let turn_start = std::time::Instant::now();
                let result = turn::run_chat_turn(
                    &mut session.messages,
                    &config,
                    &registry,
                    &session.id,
                    &mut session.usage,
                    &event_tx,
                    cancel,
                )
                .await;

                // Fix orphaned tool_use blocks left by aborted/errored turns.
                // Claude API requires every tool_use to have a matching tool_result
                // immediately after. When a turn is cancelled mid-tool-execution,
                // assistant messages with tool_calls may lack their tool_results.
                fix_orphaned_tool_uses(&mut session.messages);

                session.turn_durations.push(turn_start.elapsed().as_secs_f64());
                session.save();
                crate::config::prefs::save_last_session(&session.id);

                match result {
                    Ok(_) => { let _ = event_tx.send(Event::AgentDone).await; }
                    Err(e) => { let _ = event_tx.send(Event::AgentError(e.to_string())).await; }
                }
            }
            AgentCommand::Reset => {
                session.save();
                session = Session::new();
                if !config.system_prompt.is_empty() {
                    session.messages.push(Message {
                        role: Role::System,
                        content: config.system_prompt.clone(),
                        tool_call_id: None,
                        tool_calls: None,
                    });
                }
            }
            AgentCommand::LoadSession { messages, usage } => {
                session.save();
                session = Session::new();
                session.messages = messages;
                session.usage = usage;
            }
            AgentCommand::SetModel { model_id, source } => {
                config.model_id = model_id;
                config.source = source;
            }
            AgentCommand::SetThinking(level) => {
                config.thinking = level;
            }
            AgentCommand::Shutdown => {
                session.save();
                break;
            }
        }
    }
}

/// Ensure every `tool_use` in assistant messages has a matching `tool_result`.
///
/// When a turn is aborted mid-execution, the assistant message with `tool_calls`
/// may already be in the history but the corresponding `Tool` result messages
/// may be missing (partially or fully). This violates the Claude API contract
/// which requires a `tool_result` for every `tool_use` in the immediately
/// following user/tool message(s).
///
/// This function scans from the end of the message list and fills in any
/// missing tool_result messages with an "[aborted]" placeholder.
fn fix_orphaned_tool_uses(messages: &mut Vec<Message>) {
    // Walk backwards to find the last assistant message with tool_calls.
    let Some(asst_idx) = messages.iter().rposition(|m| {
        m.role == Role::Assistant && m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
    }) else {
        return;
    };

    let expected_ids: Vec<String> = messages[asst_idx]
        .tool_calls
        .as_ref()
        .unwrap()
        .iter()
        .map(|tc| tc.id.clone())
        .collect();

    // Collect tool_result ids that already exist after this assistant message.
    let existing_ids: std::collections::HashSet<String> = messages[asst_idx + 1..]
        .iter()
        .filter(|m| m.role == Role::Tool)
        .filter_map(|m| m.tool_call_id.clone())
        .collect();

    // Add placeholder results for any missing tool_use ids.
    for id in &expected_ids {
        if !existing_ids.contains(id) {
            messages.push(Message {
                role: Role::Tool,
                content: "[aborted]".into(),
                tool_call_id: Some(id.clone()),
                tool_calls: None,
            });
        }
    }
}
