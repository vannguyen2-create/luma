mod agent;
mod commands;
mod dispatch;
mod input;
mod render;
/// App — async event loop, state machine, owns all TUI state.
mod state;

use state::{AgentHandle, AppConfig, PickerMode, UiState};

use crate::config::models;
use crate::core::types::ThinkingLevel;
use crate::event::Event;
use crate::tui::output::OutputLog;
use crate::tui::picker::Picker;
use crate::tui::prompt::PromptState;
use crate::tui::renderer::{Region, Renderer};
use crate::tui::selection::Selection;
use crate::tui::status::StatusBar;
use crate::tui::term;
use crate::tui::text::{Line, Padding};
use crate::tui::theme::{CONTENT_PAD, OUTER_MARGIN, palette};
use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc;

const TICK_INTERVAL: Duration = Duration::from_millis(80);
const SCROLL_STEP: usize = 3;
const ABORT_HINT_TICKS: u8 = 25; // ~2s at 80ms tick

const LOGO: &[&str] = &[
    "                                      ⢰⡇⠀⠀⣸⠃      ⣴⠟⠁⠈⢻⣦",
    "                                      ⣿⠀⠀⢠⡟    ⢠⡾⠃⠀⠀⣰⠟⠁",
    "                                      ⠉⠛⠓⠾⠁⠀⠀⣰⠟⠀⠀⢀⡾⠋     ⢀⣴⣆",
    "                        ⢀⣀⣀⣀⣠⣤⣤⣤⣄⣀⣀⡀        ⠙⠳⣦⣴⠟⠁   ⣠⡴⠋⠀⠀⠈⢷⣄",
    "                ⣀⣤⣴⣶⣿⣿⣿⣿⡿⠿⠿⠿⠿⠿⠿⣿⣿⣿⣿⣷⣦⣤⣀         ⠀⣠⡾⠋⠀⠀⢀⣴⠟⠁",
    "            ⢀⣠⣶⣿⣿⡿⠟⠋⠉⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠉⠙⠻⢿⣿⣿⣶⣄⡀    ⠺⣏⠀⠀⣀⡴⠟⠁⢀⣀",
    "          ⣠⣶⣿⣿⠿⠋⠁⠀⢀⣴⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢶⣬⡙⠿⣿⣿⣶⣄   ⠙⢷⡾⠋⢀⣤⠾⠋⠙⢷⡀",
    "        ⣠⣾⣿⡿⠋⠁⠀⠀⠀⢠⣾⡟⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣰⣦⣠⣤⠽⣿⣦⠈⠙⢿⣿⣷⣄   ⠺⣏⠁⠀⠀⣀⣼⠿",
    "      ⢠⣾⣿⡿⠋⠀⠀⠀⠀⠀⣰⣿⠟⠀⠀⠀⢠⣤⠀⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⣿⣧⠀⠀⠈⢿⣷⣄⠀⠙⢿⣿⣷⣄  ⠙⣧⡴⠟⠋",
    "     ⣴⣿⣿⠏⠀⠀⠀⠀⠀⠀⢷⣿⡟⠀⣰⡆⠀⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀⣀⡀⠀⣿⣿⡀⠀⠀⠈⢿⣿⣦⠀⠀⠙⢿⣿⣦",
    "    ⣼⣿⡿⠁⠀⠦⣤⣀⠀⠀⢀⣿⣿⡇⢰⣿⠇⠀⢸⣿⡆⠀⠀⠀⠀⠀⠀⠀⣿⡇⠀⢸⣿⣿⣆⠀⠀⠈⣿⣿⣧⣠⣤⠾⢿⣿⣧",
    "   ⣸⣿⣿⣵⣿⠀⠀⠀⠉⠀⠀⣼⣿⢿⡇⣾⣿⠀⠀⣾⣿⡇⢸⠀⠀⠀⠀⠀⠀⣿⡇⠀⣼⣿⢻⣿⣦⠴⠶⢿⣿⣿⣇⠀⠀⠀⢻⣿⣧⣀",
    "  ⢀⣿⣿⣿⣿⠇⠀⠀⠀⠀⠀⢠⣿⡟⡌⣼⣿⣿⠉⢁⣿⣿⣷⣿⡗⠒⠚⠛⠛⢛⣿⣯⣯⣿⣿⠀⢻⣿⣧⠀⢸⣿⣿⣿⡄⠀⠀⠀⠙⢿⣿⣷⣤⣀",
    "  ⢸⣿⣿⣿⠏⠀⠀⠀⠀⠀⠀⢸⣿⡇⣼⣿⣿⣿⣶⣾⣿⣿⢿⣿⡇⠀⠀⠀⠀⢸⣿⠟⢻⣿⣿⣿⣶⣿⣿⣧⢸⣿⣿⣿⣧⠀⠀⠀⢰⣷⡈⠛⢿⣿⣿⣶⣦⣤⣤⣀",
    "   ⢀⣤⣾⣿⣿⢫⡄⠀⠀⠀⠀⠀⠀⣿⣿⣹⣿⠏⢹⣿⣿⣿⣿⣿⣼⣿⠃⠀⠀⠀⢀⣿⡿⢀⣿⣿⠟⠀⠀⠀⠹⣿⣿⣿⠇⢿⣿⡄⠀⠀⠈⢿⣿⣷⣶⣶⣿⣿⣿⣿⣿⡿",
    "⣴⣶⣶⣿⣿⣿⣿⣋⣴⣿⣇⠀⠀⠀⠀⢀⣿⣿⣿⣟⣴⠟⢿⣿⠟⣿⣿⣿⣿⣶⣶⣶⣶⣾⣿⣿⣿⠿⣫⣤⣶⡆⠀⠀⣻⣿⣿⣶⣸⣿⣷⡀⠀⠀⠸⣿⣿⣿⡟⠛⠛⠛⠉⠁",
    "⠻⣿⣿⣿⣿⣿⣿⡿⢿⣿⠋⠀⢠⠀⠀⢸⣿⣿⣿⣿⣁⣀⣀⣁⠀⠀⠉⠉⠉⠉⠉⠉⠉⠁⠀⠀⠀⠸⢟⣫⣥⣶⣿⣿⣿⠿⠟⠋⢻⣿⡟⣇⣠⡤⠀⣿⣿⣿⣿⡀",
    "   ⠉⠉⢹⣿⡇⣾⣿⠀⠀⢸⡆⠀⢸⣿⣿⡟⠿⠿⠿⠿⣿⣿⣿⣿⣷⣦⡄⠀⠀⠀⠀⠀⠀⢠⣾⣿⣿⣿⣿⣯⣥⣤⣄⣀⡀⢸⣿⠇⢿⢸⡇⠀⢹⣿⣿⣿⡇",
    "     ⣾⣿⡇⣿⣿⠀⠀⠸⣧⠀⢸⣿⣿⠀⢀⣀⣤⣤⣶⣾⣿⠿⠟⠛⠁⠀⠀⠀⠀⠀⠀⠀⠉⠉⠉⠙⠛⢛⣛⠛⠛⠛⠃⠸⣿⣆⢸⣿⣇⠀⢸⣿⣿⣿⣷",
    "     ⢻⣿⡇⢻⣿⡄⠀⠀⣿⡄⢸⣿⡷⢾⣿⠿⠟⠛⠉⠉⠀⠀⠀⢠⣶⣾⣿⣿⣿⣿⣿⣶⣶⠀⠀⢀⡾⠋⠁⢠⡄⠀⣤⠀⢹⣿⣦⣿⡇⠀⢸⣿⣿⣿⣿",
    "     ⢸⣿⣇⢸⣿⡇⠀⠀⣿⣧⠈⣿⣷⠀⠀⢀⣀⠀⢙⣧⠀⠀⠀⢸⣿⡇⠀⠀⠀⠀⢀⣿⡏⠀⠀⠸⣇⠀⠀⠘⠛⠘⠛⠀⢀⣿⣿⣿⡇⠀⣼⣿⢻⣿⡿",
    "     ⠸⣿⣿⣸⣿⣿⠀⠀⣿⣿⣆⢿⣿⡀⠀⠸⠟⠀⠛⣿⠃⠀⠀⢸⣿⡇⠀⠀⠀⠀⢸⣿⡇⠀⠀⠀⠙⠷⣦⣄⡀⠀⢀⣴⣿⡿⣱⣾⠁⠀⣿⣿⣾⣿⡇",
    "      ⢻⣿⣿⣿⣿⣇⠀⢿⢹⣿⣆⢸⣿⣧⣀⠀⠀⠴⠞⠁⠀⠀⠸⣿⡇⠀⠀⠀⠀⣿⣿⠀⠀⠀⠀⠀⠀⢀⣨⣽⣾⣿⣿⡏⢀⣿⣿⠀⣸⣿⣿⣿⡿",
    "      ⠈⢻⣿⣿⣿⣿⣆⢸⡏⠻⣿⣦⣿⣿⣿⣿⣶⣦⣤⣀⣀⣀⣀⣿⣷⠀⠀⠀⣸⣿⣏⣀⣤⣤⣶⣾⣿⣿⣿⠿⠛⢹⣿⣧⣼⣿⣿⣰⣿⣿⠛⠛",
    "        ⠉⠛⠙⣿⣿⣦⣷⠀⢻⣿⣿⣿⣿⡝⠛⠻⠿⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠿⠟⠛⠛⠉⠁⠀⠀⠀⣼⣿⣿⣿⣿⣿⣿⣿⠃",
    "           ⠈⢻⣿⣿⣄⢸⣿⣿⣿⣿⣷⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠿⠟⠻⣿⡿⠋⠁",
    "             ⠙⢿⣿⣿⣿⣿⡌⠙⠛⠁",
];

struct Layout {
    output: Region,
    status: Region,
    input: Region,
}

fn compute_layout(w: u16, h: u16) -> Layout {
    let mx = OUTER_MARGIN;
    let sh = 2u16;
    let ih = 5u16;
    let oh = h.saturating_sub(sh + ih).max(1);
    let inner_w = w.saturating_sub(mx * 2);
    Layout {
        output: Region {
            row: 1,
            col: 1 + mx,
            width: inner_w,
            height: oh,
            bg: palette::BG,
            padding: Padding {
                left: CONTENT_PAD,
                right: CONTENT_PAD,
                top: 0,
                bottom: 1,
            },
        },
        status: Region {
            row: 1 + oh + ih,
            col: 1,
            width: w,
            height: sh,
            bg: palette::BG,
            padding: Padding {
                left: OUTER_MARGIN + CONTENT_PAD,
                right: OUTER_MARGIN + CONTENT_PAD,
                top: 0,
                bottom: 1,
            },
        },
        input: Region {
            row: 1 + oh,
            col: 1 + mx,
            width: inner_w,
            height: ih,
            bg: palette::SURFACE,
            padding: Padding::zero(),
        },
    }
}

/// The TUI application.
pub struct App {
    ui: UiState,
    renderer: Renderer,
    layout: Layout,
    agent: AgentHandle,
    config: AppConfig,
    tx: Option<mpsc::Sender<Event>>,
}

impl App {
    /// Create the app.
    pub fn new(env_context: String) -> Self {
        let (w, h) = term::size();
        let layout = compute_layout(w, h);
        let mut renderer = Renderer::new(w, h);
        renderer.define("output", layout.output.clone());
        renderer.define("status", layout.status.clone());
        renderer.define("input", layout.input.clone());

        let output = OutputLog::new(
            layout.output.content_width() as usize,
            layout.output.content_height() as usize,
        );
        let mut prompt = PromptState::new();
        prompt.add_command("new", "new thread");
        prompt.add_command("model", "switch model");
        prompt.add_command("sessions", "browse sessions");
        prompt.add_command("exit", "quit luma");

        let mode = crate::config::prefs::load_mode();
        let model = models::resolve_default(mode);
        let thinking = crate::config::prefs::load_thinking();

        let ui = UiState {
            output,
            prompt,
            picker: Picker::new(),
            status: StatusBar::new(),
            selection: Selection::new(),
            drag: None,
            last_output_width: 0,
        };
        let config = AppConfig {
            mode,
            model,
            env_context,
            thinking,
            picker_mode: PickerMode::Model,
        };

        let mut app = Self {
            ui,
            renderer,
            layout,
            agent: AgentHandle::new(),
            config,
            tx: None,
        };
        app.update_status();
        if thinking != ThinkingLevel::Off {
            let label = match thinking {
                ThinkingLevel::Off => "off",
                ThinkingLevel::Low => "low",
                ThinkingLevel::Medium => "medium",
                ThinkingLevel::High => "high",
            };
            app.ui.status.set_thinking_level(label);
        }
        app
    }

    /// Run the event loop.
    pub async fn run(mut self) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel::<Event>(256);
        self.tx = Some(tx.clone());

        let orig = term::enter_raw()?;
        let mut out = term::buffered_stdout();
        write!(
            out,
            "{}{}{}{}",
            term::ALT_ON,
            term::CURSOR_HIDE,
            term::MOUSE_ON,
            term::PASTE_ON,
        )?;
        out.flush()?;
        self.renderer.clear_screen();

        if self.config.model.is_none() {
            self.ui.output.warn("no model — run 'luma sync'");
        }
        // Vertical centering: pad so logo sits in the middle of the output area
        let logo_height = LOGO.len() + 2; // +2 for dividers
        let output_h = self.layout.output.content_height() as usize;
        let top_pad = output_h.saturating_sub(logo_height) / 2;
        for _ in 0..top_pad {
            self.ui.output.divider();
        }
        self.ui.output.logo(LOGO);
        self.ui.output.divider();

        self.render();

        let tx_input = tx.clone();
        tokio::task::spawn_blocking(move || input::read_stdin_loop(tx_input));

        let tx_tick = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TICK_INTERVAL);
            loop {
                interval.tick().await;
                if tx_tick.send(Event::Tick).await.is_err() {
                    break;
                }
            }
        });

        let tx_resize = tx.clone();
        tokio::spawn(async move {
            if let Ok(mut sig) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
            {
                loop {
                    sig.recv().await;
                    let (w, h) = term::size();
                    if tx_resize.send(Event::Resize { w, h }).await.is_err() {
                        break;
                    }
                }
            }
        });

        loop {
            let Some(event) = rx.recv().await else { break };
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle(event)));
            match result {
                Ok(Action::Continue) => {}
                Ok(Action::Render) => self.render(),
                Ok(Action::Quit) => break,
                Err(panic) => {
                    let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_owned()
                    };
                    crate::dbg_log!("PANIC caught: {msg}");
                    self.ui.output.error(&format!("internal error: {msg}"));
                    self.render();
                }
            }
        }

        let mut out = term::buffered_stdout();
        write!(
            out,
            "{}{}{}{}{}",
            term::PASTE_OFF,
            term::MOUSE_OFF,
            term::CURSOR_SHOW,
            term::RESET,
            term::ALT_OFF,
        )?;
        out.flush()?;
        term::exit_raw(&orig);
        std::process::exit(0);
    }
}

fn composite_overlay(content: &[Line], overlay: &[Line], height: usize) -> Vec<Line> {
    let overlay_count = overlay.len().min(height);
    let content_space = height.saturating_sub(overlay_count);
    let mut result: Vec<Line> = content.iter().take(content_space).cloned().collect();
    let pad_count = content_space.saturating_sub(result.len());
    result.extend(std::iter::repeat_n(Line::empty(), pad_count));
    result.extend(overlay.iter().take(overlay_count).cloned());
    result
}

/// Format a duration compactly: "1.2s", "45.0s", "1m 23s".
fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let m = d.as_secs() / 60;
        let s = d.as_secs() % 60;
        format!("{m}m {s}s")
    }
}

enum Action {
    Continue,
    Render,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_layout_basic() {
        let l = compute_layout(80, 24);
        assert_eq!(l.output.height, 17);
        assert_eq!(l.input.height, 5);
        assert_eq!(l.status.height, 2);
        assert_eq!(l.output.width, 80 - OUTER_MARGIN * 2);
        assert_eq!(
            l.output.content_width(),
            80 - (OUTER_MARGIN + CONTENT_PAD) * 2
        );
    }

    #[test]
    fn format_duration_short() {
        let d = std::time::Duration::from_secs_f64(3.456);
        assert_eq!(format_duration(d), "3.5s");
    }

    #[test]
    fn format_duration_long() {
        let d = std::time::Duration::from_secs(95);
        assert_eq!(format_duration(d), "1m 35s");
    }
}
