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
        let files = read_file_refs(&text);
        crate::dbg_log!(
            "submit: {}, {} files",
            text.chars().take(40).collect::<String>(),
            files.len()
        );
        self.spawn_agent(text, files);
        Action::Render
    }

    /// Send user message to agent and start streaming.
    pub(super) fn spawn_agent(&mut self, text: String, files: Vec<crate::event::FileAttach>) {
        if self.config.model.is_none() {
            self.ui.output.error("no model — run 'luma sync'");
            return;
        }
        self.ensure_agent_loop();
        self.ui.output.user_message(&text);
        self.agent.state = RunState::Streaming;
        self.ui.status.set_state(StatusState::Thinking);
        self.agent.turn_start = Some(std::time::Instant::now());

        let attached = self.ui.prompt.take_images();
        let images: Vec<crate::event::ImageAttach> = attached
            .into_iter()
            .map(|img| crate::event::ImageAttach {
                media_type: img.media_type,
                data: img.data,
            })
            .collect();

        let cancel = CancellationToken::new();
        self.agent.cancel = Some(cancel.clone());
        if let Some(agent_tx) = &self.agent.tx {
            let _ = agent_tx.try_send(AgentCommand::Chat {
                text,
                files,
                images,
                cancel,
            });
        }
    }

    /// Read clipboard image and attach to prompt.
    pub(super) fn paste_clipboard_image(&mut self) {
        let Some(data) = read_clipboard_image() else {
            self.ui.output.info("no image in clipboard");
            return;
        };
        let (media_type, _) = detect_image_format(&data);
        self.ui.prompt.attach_image(media_type.to_owned(), data);
    }

    /// Read image from file path (drag-drop) and attach to prompt.
    pub(super) fn paste_image_file(&mut self, path: &str) {
        let Ok(data) = std::fs::read(path) else {
            self.ui.output.info("cannot read image file");
            return;
        };
        let (media_type, _) = detect_image_format(&data);
        self.ui.prompt.attach_image(media_type.to_owned(), data);
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

        // Declare server capabilities — each provider maps these to its own format.
        registry.add_server_capability("web_search");

        // Fallback: client-side web search when provider doesn't support built-in.
        if let Some(backend) = Self::search_backend() {
            registry.register(Box::new(crate::tool::web_search::WebSearchTool::new(
                backend,
            )));
        }

        self.agent.tx = Some(crate::core::agent::spawn(config, registry, tx));
    }

    /// Detect search backend from environment variables.
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
            let files = read_file_refs(&next);
            self.spawn_agent(next, files);
        }
    }
}

/// Read image data from system clipboard via osascript.
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

/// Clipboard image not yet supported on this platform.
#[cfg(not(target_os = "macos"))]
fn read_clipboard_image() -> Option<Vec<u8>> {
    None
}

/// Detect image format from magic bytes.
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

/// Read `@path` references from text, returning file contents for the agent.
/// Text stays unchanged — files are sent as separate content blocks.
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

/// Parse `@path` patterns from text. Returns sorted by start position.
fn parse_file_refs(text: &str) -> Vec<FileRef> {
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            // Must be at start of text or preceded by whitespace
            if i > 0 && !bytes[i - 1].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            i += 1; // skip @
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
    fn parse_file_refs_no_exist() {
        assert!(parse_file_refs("check @nonexistent.xyz").is_empty());
    }

    #[test]
    fn parse_file_refs_finds_existing() {
        let refs = parse_file_refs("look at @Cargo.toml please");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].path, "Cargo.toml");
    }

    #[test]
    fn read_file_refs_existing() {
        let files = read_file_refs("check @Cargo.toml");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "Cargo.toml");
        assert!(files[0].content.contains("[package]"));
    }

    #[test]
    fn read_file_refs_none() {
        let files = read_file_refs("hello world");
        assert!(files.is_empty());
    }

    #[test]
    fn email_not_treated_as_file_ref() {
        let files = read_file_refs("email user@example.com please");
        assert!(files.is_empty());
    }
}
