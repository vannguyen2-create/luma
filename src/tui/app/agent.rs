/// Agent lifecycle — spawn, submit, done handling.
use super::state::RunState;
use super::Action;
use crate::event::AgentCommand;
use crate::tui::status::StatusState;
use tokio_util::sync::CancellationToken;

impl super::App {
    /// Handle submit from prompt — command or chat.
    pub(super) fn on_submit(&mut self, content: Vec<crate::core::types::ContentBlock>) -> Action {
        // Command: first text block starts with /
        if let Some(crate::core::types::ContentBlock::Text { text }) = content.first()
            && let Some(cmd) = text.strip_prefix('/')
        {
            return self.handle_command(cmd);
        }
        if self.agent.state != RunState::Idle {
            self.agent.pending_content = Some(content);
            self.agent.state = RunState::Aborting;
            if let Some(c) = &self.agent.cancel {
                c.cancel();
            }
            return Action::Render;
        }
        self.spawn_agent(content);
        Action::Render
    }

    /// Send user content to agent.
    pub(super) fn spawn_agent(&mut self, content: Vec<crate::core::types::ContentBlock>) {
        if self.config.model.is_none() {
            self.doc.error("no model — run 'luma sync'");
            return;
        }
        self.ensure_agent_loop();
        self.enter_chat();
        self.doc.user_message(&content);
        self.agent.state = RunState::Streaming;
        self.ui.status.set_state(StatusState::Thinking);
        self.agent.turn_start = Some(std::time::Instant::now());

        // Extract file refs from text blocks
        let text = crate::core::types::Message::content_text(&content);
        let files = read_file_refs(&text);

        // Extract image binary data from prompt attachments
        let images: Vec<crate::event::ImageAttach> = self
            .ui
            .prompt
            .take_images()
            .into_iter()
            .map(|(media_type, data)| crate::event::ImageAttach { media_type, data })
            .collect();

        let cancel = CancellationToken::new();
        self.agent.cancel = Some(cancel.clone());
        if let Some(agent_tx) = &self.agent.tx {
            let _ = agent_tx.try_send(AgentCommand::Chat {
                content,
                files,
                images,
                cancel,
            });
        }
    }

    pub(super) fn paste_clipboard_image(&mut self) {
        let Some(data) = read_clipboard_image() else {
            self.doc.info("no image in clipboard");
            return;
        };
        let (media_type, _) = detect_image_format(&data);
        self.ui.prompt.attach_image(media_type.to_owned(), data);
    }

    pub(super) fn paste_image_file(&mut self, path: &str) {
        let Ok(data) = std::fs::read(path) else {
            self.doc.info("cannot read image file");
            return;
        };
        let (media_type, _) = detect_image_format(&data);
        self.ui.prompt.attach_image(media_type.to_owned(), data);
    }

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
            registry.register(Box::new(crate::tool::bash::BashTool::codex()));
            registry.register(Box::new(crate::tool::apply_patch::ApplyPatchTool));
        } else {
            registry.register(Box::new(crate::tool::read::ReadTool));
            registry.register(Box::new(crate::tool::write::WriteTool));
            registry.register(Box::new(crate::tool::edit::EditTool));
            registry.register(Box::new(crate::tool::bash::BashTool::claude()));
            registry.register(Box::new(crate::tool::glob::GlobTool));
            registry.register(Box::new(crate::tool::grep::GrepTool));
        }
        registry.add_server_capability("web_search");
        if let Some(backend) = Self::search_backend() {
            registry.register(Box::new(crate::tool::web_search::WebSearchTool::new(
                backend,
            )));
        }

        self.agent.tx = Some(crate::core::agent::spawn(config, registry, tx));
    }

    fn search_backend() -> Option<crate::tool::web_search::SearchBackend> {
        use crate::tool::web_search::SearchBackend;
        if let Ok(key) = std::env::var("EXA_API_KEY") {
            return Some(SearchBackend::Exa { api_key: key });
        }
        if let Ok(key) = std::env::var("TAVILY_API_KEY") {
            return Some(SearchBackend::Tavily { api_key: key });
        }
        if let Ok(url) = std::env::var("SEARXNG_URL") {
            return Some(SearchBackend::SearXNG { base_url: url });
        }
        None
    }

    pub(super) fn on_agent_done(&mut self) {
        self.doc.newline();
        if let Some(start) = self.agent.turn_start.take() {
            let label = super::format_duration(start.elapsed());
            self.doc.divider_with_label(&label);
        } else {
            self.doc.divider();
        }
        self.agent.state = RunState::Idle;
        self.agent.cancel = None;
        self.ui.status.set_state(StatusState::Ready);

        if let Some(content) = self.agent.pending_content.take() {
            self.spawn_agent(content);
        }
    }

    pub(super) fn on_agent_error(&mut self, msg: &str) {
        if msg.contains("Aborted") {
            self.doc.warn("aborted");
        } else {
            self.doc.error(msg);
        }
        self.on_agent_done();
    }
}

#[cfg(target_os = "macos")]
fn read_clipboard_image() -> Option<Vec<u8>> {
    let tmp = std::env::temp_dir().join("luma_clipboard.png");
    let script = format!(
        r#"set theFile to POSIX file "{}"
try
    set theImage to the clipboard as «class PNGf»
    set fileRef to open for access theFile with write permission
    set eof of fileRef to 0
    write theImage to fileRef
    close access fileRef
on error
    try
        close access theFile
    end try
    error "no image"
end try"#,
        tmp.display()
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let data = std::fs::read(&tmp).ok()?;
    let _ = std::fs::remove_file(&tmp);
    if data.is_empty() {
        return None;
    }
    Some(data)
}

#[cfg(not(target_os = "macos"))]
fn read_clipboard_image() -> Option<Vec<u8>> {
    None
}

fn detect_image_format(data: &[u8]) -> (&'static str, &'static str) {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        ("image/png", "png")
    } else if data.starts_with(&[0xFF, 0xD8]) {
        ("image/jpeg", "jpg")
    } else if data.starts_with(b"GIF") {
        ("image/gif", "gif")
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        ("image/webp", "webp")
    } else {
        ("image/png", "png")
    }
}

fn read_file_refs(text: &str) -> Vec<crate::event::FileAttach> {
    parse_file_refs(text)
        .into_iter()
        .filter_map(|fref| {
            let content = std::fs::read_to_string(&fref.path).ok()?;
            Some(crate::event::FileAttach {
                path: fref.path,
                content,
            })
        })
        .collect()
}

struct FileRef {
    path: String,
}

fn parse_file_refs(text: &str) -> Vec<FileRef> {
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            if i > 0 && !bytes[i - 1].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            i += 1;
            let path_start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'@'
                && bytes[i] != b','
                && bytes[i] != b';'
            {
                i += 1;
            }
            let path_str = &text[path_start..i];
            if !path_str.is_empty() {
                let p = std::path::Path::new(path_str);
                if p.is_file() {
                    refs.push(FileRef {
                        path: path_str.to_owned(),
                    });
                }
            }
        } else {
            i += 1;
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file_refs_empty() {
        assert!(parse_file_refs("hello world").is_empty());
    }

    #[test]
    fn parse_file_refs_finds_existing() {
        let refs = parse_file_refs("look at @Cargo.toml please");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].path, "Cargo.toml");
    }

    #[test]
    fn email_not_treated_as_file_ref() {
        assert!(parse_file_refs("email user@example.com please").is_empty());
    }
}
