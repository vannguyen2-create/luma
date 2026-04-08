/// Block render dispatch — single entry point for all block types.
use super::chrome::{render_skill, render_thinking, render_user, ThinkingCache};
use super::text::TextCache;
use super::tool::render_tool;
use super::Block;
use crate::tui::text::{Line, Span};
use crate::tui::theme::palette;
use smallvec::smallvec;

/// Per-block render state owned by Layout.
#[derive(Debug, Clone)]
pub enum RenderState {
    None,
    Text(TextCache),
    Thinking(ThinkingCache),
}

impl RenderState {
    /// Create default state (no cache).
    pub fn new() -> Self {
        Self::None
    }
}

/// Render a block into Lines. Uses `state` for incremental caching.
pub fn render_block(
    block: &Block,
    state: &mut RenderState,
    width: usize,
    spinner_frame: usize,
) -> Vec<Line> {
    match block {
        Block::Gap => vec![Line::empty()],
        Block::GapLabel(label) => {
            vec![Line::new(smallvec![Span::new(
                label.clone(),
                palette::MUTED,
            )])]
        }
        Block::Info(t) => wrap_icon(super::chrome::ICON_INFO, palette::DIM, t, width),
        Block::Success(t) => wrap_icon(super::chrome::ICON_SUCCESS, palette::SUCCESS, t, width),
        Block::Error(t) => wrap_icon(super::chrome::ICON_ERROR, palette::ERROR, t, width),
        Block::Warn(t) => wrap_icon(super::chrome::ICON_WARN, palette::WARN, t, width),
        Block::User(lines) => render_user(lines, width),
        Block::Thinking(stream) => render_thinking(stream, state, width),
        Block::Text(tb) => super::text::render_text(tb, state, width),
        Block::Tool(tb) => render_tool(tb, width, spinner_frame),
        Block::Skill(sb) => render_skill(sb, width),
    }
}

/// Wrap an icon + text into lines.
pub fn wrap_icon(ic: &str, color: crate::tui::theme::Rgb, text: &str, w: usize) -> Vec<Line> {
    let line = Line::new(smallvec![
        Span::new(format!("{ic} "), color),
        Span::new(text.to_owned(), color),
    ]);
    crate::tui::text::wrap_line(&line, w, None)
}
