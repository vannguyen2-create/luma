/// Agent lifecycle — spawn, submit, done handling.
use super::state::RunState;
use super::Action;
use crate::event::AgentCommand;
use crate::tui::status::StatusState;
use tokio_util::sync::CancellationToken;

impl super::App {
    /// Handle user submit — dispatch command or start agent turn.
    pub(super) fn on_submit(&mut self, text: String) -> Action {
        if let Some(cmd) = text.strip_prefix('/') {
            return self.handle_command(cmd);
        }
        if self.agent.state != RunState::Idle {
            self.agent.pending_input = Some(text);
            self.agent.state = RunState::Aborting;
            if let Some(c) = &self.agent.cancel {
                c.cancel();
            }
            return Action::Render;
        }
        crate::dbg_log!("submit: {}", text.chars().take(40).collect::<String>());
        self.spawn_agent(text);
        Action::Render
    }

    /// Send user message to agent and start streaming.
    pub(super) fn spawn_agent(&mut self, text: String) {
        if self.config.model.is_none() {
            self.ui.output.error("no model — run 'luma sync'");
            return;
        }
        self.ensure_agent_loop();
        self.ui.output.user_message(&text);
        self.agent.state = RunState::Streaming;
        self.ui.status.set_state(StatusState::Thinking);
        self.agent.turn_start = Some(std::time::Instant::now());

        let cancel = CancellationToken::new();
        self.agent.cancel = Some(cancel.clone());
        if let Some(agent_tx) = &self.agent.tx {
            let _ = agent_tx.try_send(AgentCommand::Chat { text, cancel });
        }
    }

    /// Start agent loop if not running.
    pub(super) fn ensure_agent_loop(&mut self) {
        if self.agent.tx.is_some() {
            return;
        }
        let Some(model) = &self.config.model else {
            return;
        };
        let tx = self.tx.clone().expect("tx set in run()");

        let skills = crate::config::skills::discover();
        let skill_catalog = crate::config::skills::build_catalog(&skills);
        let project_instructions = crate::config::instructions::discover();
        let instructions_block =
            crate::config::instructions::build_instructions(&project_instructions);
        let base_prompt = crate::config::prompt::build(&model.source, self.config.mode);
        let system_prompt = format!(
            "{base_prompt}\n{}{skill_catalog}{instructions_block}",
            self.config.env_context
        );

        let config = crate::core::agent::AgentConfig {
            model_id: model.id.clone(),
            source: model.source.clone(),
            system_prompt,
            thinking: self.config.thinking,
        };

        let mut registry = crate::core::registry::Registry::new();
        if model.source == "codex" {
            registry.register(Box::new(crate::tool::bash::BashTool));
            registry.register(Box::new(crate::tool::apply_patch::ApplyPatchTool));
        } else {
            registry.register(Box::new(crate::tool::read::ReadTool));
            registry.register(Box::new(crate::tool::write::WriteTool));
            registry.register(Box::new(crate::tool::edit::EditTool));
            registry.register(Box::new(crate::tool::bash::BashTool));
            registry.register(Box::new(crate::tool::glob::GlobTool));
            registry.register(Box::new(crate::tool::grep::GrepTool));
        }
        registry.set_wire_names(&model.source);
        self.agent.tx = Some(crate::core::agent::spawn(config, registry, tx));
    }

    /// Handle agent completion — show duration, reset state, process pending.
    pub(super) fn on_agent_done(&mut self) {
        self.ui.output.newline();
        if let Some(start) = self.agent.turn_start.take() {
            let label = super::format_duration(start.elapsed());
            self.ui.output.divider_with_label(&label);
        } else {
            self.ui.output.divider();
        }
        self.agent.state = RunState::Idle;
        self.agent.cancel = None;
        self.ui.status.set_state(StatusState::Ready);

        if let Some(next) = self.agent.pending_input.take() {
            self.spawn_agent(next);
        }
    }
}
