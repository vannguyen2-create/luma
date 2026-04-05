/// Event dispatch — central event handler and keyboard input routing.
use super::state::{PickerMode, RunState};
use super::{Action, ABORT_HINT_TICKS};
use crate::config::models;
use crate::event::{Event, KeyEvent};
use crate::tui::picker::PickerAction;
use crate::tui::prompt::PromptAction;

impl super::App {
    /// Route an event to the appropriate handler.
    pub(super) fn handle(&mut self, event: Event) -> Action {
        if self.agent.state == RunState::Aborting {
            return match event {
                Event::Key(k) => self.on_key(k),
                Event::Mouse(m) => self.on_mouse(m),
                Event::Resize { w, h } => {
                    self.handle_resize(w, h);
                    Action::Render
                }
                Event::Tick => {
                    self.ui.status.tick();
                    Action::Render
                }
                Event::AgentDone => {
                    self.on_agent_done();
                    Action::Render
                }
                Event::AgentError(msg) => {
                    if !msg.contains("Aborted") {
                        self.ui.output.error(&msg);
                    }
                    self.on_agent_done();
                    Action::Render
                }
                _ => Action::Continue,
            };
        }

        match event {
            Event::Key(k) => self.on_key(k),
            Event::Mouse(m) => self.on_mouse(m),
            Event::Resize { w, h } => {
                self.handle_resize(w, h);
                Action::Render
            }
            Event::Tick => {
                let active = matches!(
                    self.agent.state,
                    RunState::Streaming | RunState::PendingAbort
                );
                self.ui.output.tick();
                self.ui.status.tick();
                if self.agent.state == RunState::PendingAbort {
                    self.agent.abort_countdown = self.agent.abort_countdown.saturating_sub(1);
                    if self.agent.abort_countdown == 0 {
                        self.agent.state = RunState::Streaming;
                    }
                    return Action::Render;
                }
                if active {
                    Action::Render
                } else {
                    Action::Continue
                }
            }
            Event::Token(t) => {
                crate::dbg_log!("token: {}B", t.len());
                self.ui.output.append_token(&t);
                Action::Render
            }
            Event::Thinking(t) => {
                crate::dbg_log!("thinking: {}B", t.len());
                self.ui.output.append_thinking(&t);
                Action::Render
            }
            Event::ToolStart { name, summary } => {
                crate::dbg_log!("tool_start {name} {summary}");
                self.ui.output.tool_start(&name, &summary);
                Action::Render
            }
            Event::ToolInput { name, chunk } => {
                crate::dbg_log!("tool_input {name}: {}B", chunk.len());
                self.ui.output.tool_input(&name, &chunk);
                Action::Render
            }
            Event::ToolOutput { name, chunk } => {
                crate::dbg_log!(
                    "tool_output {name}: {:?}",
                    chunk.chars().take(60).collect::<String>()
                );
                self.ui.output.tool_output(&name, &chunk);
                Action::Render
            }
            Event::ToolEnd { name, summary } => {
                crate::dbg_log!("tool_end {name} {summary}");
                self.ui.output.tool_end(&name, &summary);
                Action::Render
            }
            Event::SkillStart(name) => {
                self.ui.output.skill_start(&name);
                Action::Render
            }
            Event::SkillEnd(summary) => {
                self.ui.output.skill_end(&summary);
                Action::Render
            }
            Event::Usage(usage) => {
                self.ui.status.add_cache(
                    usage.cache_read.unwrap_or(0),
                    usage.cache_write.unwrap_or(0),
                );
                // Total context = all input (uncached + cached) + output
                let input_total = usage.input_tokens
                    + usage.cache_read.unwrap_or(0)
                    + usage.cache_write.unwrap_or(0);
                let total = input_total + usage.output_tokens;
                let ctx_window = self
                    .config
                    .model
                    .as_ref()
                    .map(|m| models::context_window(&m.id))
                    .unwrap_or(200_000);
                let pct = ((total as f64 / ctx_window as f64) * 100.0).min(100.0) as u8;
                self.ui.status.set_context(total, pct);
                Action::Render
            }
            Event::AgentDone => {
                crate::dbg_log!("agent done");
                self.on_agent_done();
                Action::Render
            }
            Event::AgentError(msg) => {
                crate::dbg_log!("agent error: {msg}");
                if !msg.contains("Aborted") {
                    self.ui.output.error(&msg);
                }
                self.on_agent_done();
                Action::Render
            }
        }
    }

    /// Handle keyboard input — escape, picker, tab, prompt keys.
    pub(super) fn on_key(&mut self, key: KeyEvent) -> Action {
        crate::dbg_log!("key {:?} state={:?}", key, self.agent.state);
        if key == KeyEvent::Escape {
            if self.agent.state == RunState::PendingAbort {
                self.agent.state = RunState::Aborting;
                self.ui.output.abort();
                if let Some(c) = &self.agent.cancel {
                    c.cancel();
                }
                return Action::Render;
            }
            if self.agent.state == RunState::Streaming {
                self.agent.state = RunState::PendingAbort;
                self.agent.abort_countdown = ABORT_HINT_TICKS;
                return Action::Render;
            }
            // While aborting or any non-idle agent state, swallow Escape
            if self.agent.state != RunState::Idle {
                return Action::Continue;
            }
        }

        if self.ui.picker.is_active {
            match self.ui.picker.handle_key(key) {
                PickerAction::Select(id) => {
                    match self.config.picker_mode {
                        PickerMode::Model => self.select_model(&id),
                        PickerMode::Session => self.resume_session(&id),
                    }
                    return Action::Render;
                }
                PickerAction::Cancel => return Action::Render,
                PickerAction::Redraw => return Action::Render,
                PickerAction::None => return Action::Continue,
            }
        }

        if key == KeyEvent::Tab && self.agent.state == RunState::Idle {
            self.quick_cycle_mode();
            return Action::Render;
        }

        match self.ui.prompt.handle_key(key) {
            PromptAction::None => Action::Continue,
            PromptAction::Redraw => Action::Render,
            PromptAction::Submit(text) => self.on_submit(text),
            PromptAction::Interrupt => Action::Quit,
            PromptAction::ToggleThinking => {
                self.cycle_thinking();
                Action::Render
            }
        }
    }
}
