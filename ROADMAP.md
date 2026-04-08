# Roadmap

Current: ~12K lines Rust, 290 tests, 3 providers, 9 tools. Cross-platform (macOS, Linux, Windows).

## v0.2 — Done

- [x] Tool diff colors + syntax highlighting
- [x] Wire name removal — tools carry API-native names
- [x] Prompt templates (`src/config/prompt/`)
- [x] Tool improvements: BufReader (Read), deadline timeout (Bash), skip unchanged (Write), curly-quote (Edit), ignore crate (Glob/Grep)
- [x] Web search — server tools + client fallback
- [x] Non-blocking input, lazy block rendering
- [x] Session resume, save on error/abort, auto-resume

## v0.3 — Architecture + Multimodal — Done

- [x] **TUI architecture redesign** — Document (model) + Layout (render cache) + ViewState (orchestrator). Unidirectional data flow, pull-based dirty detection via `Block::snapshot()`
- [x] **Block render pipeline** — `block/` module: mod, render, text, tool, chrome, diff. TextCache owned by Layout via `RenderState`. Read-only `&[Block]` rendering
- [x] **Screen architecture** — `Screen::Welcome { lines }` / `Screen::Chat`. Data on variant, Rust drops on transition. No `has_logo` flag
- [x] **Renderer FloatingLayer** — dropdown/picker as overlay layer in flush pipeline. No content cloning. Works on all screens
- [x] **Segment prompt buffer** — `PromptBuffer` with `Vec<Seg>`. Single source of truth for editing. Image/paste insert at cursor, backspace removes naturally
- [x] **ContentBlock pipeline** — `Vec<ContentBlock>` from prompt → app → doc → agent → session → resume → render. No XML serialization. `ContentBlock::Paste` variant
- [x] **Shared content rendering** — `content_lines()` for both prompt input and user chat bubble
- [x] **Abort handling** — tool results always pushed, `[user interrupted]` system message for LLM context, active tools marked "aborted"
- [x] **Input normalization** — `\r\n` / `\r` → `\n`. Image drag-drop with `file://` and quote stripping
- [x] **Esc/Ctrl+C separation** — Esc = interrupt streaming, Ctrl+C = clear buffer / quit
- [x] **Cross-platform** — Windows install (`install.ps1`), Git Bash support, CI paths-ignore for docs
- [x] **@file mention** — fuzzy autocomplete, file content injection, `@path` highlighting
- [x] **Image attach** — clipboard paste, drag-drop, multimodal provider serialization
- [x] Legacy cleanup — removed custom deserializer, OutputLog, viewport, composite_overlay

## v0.4 — Performance & Polish

- [ ] Diff algorithm: Myers O(n+m) replacing LCS O(n*m)
- [ ] Diff line count in tool output ("Updated file.rs +5 -3")
- [ ] URL highlighting in markdown (bare https:// links)
- [ ] Terminal resize during streaming — verify no corruption
- [ ] Streaming freeze profiling for large sessions (65K+ tokens)
- [ ] Session management: `/delete`, `/rename`, `/export`

## v0.5 — UX & Extensibility

- [ ] MCP server support — connect to external tool servers
- [ ] Custom tools via config — shell commands + schema
- [ ] File watcher — detect external changes, warn before overwrite
- [ ] Copy selection to clipboard
- [ ] Search within output (Ctrl+F)
- [ ] Themes — user-configurable color schemes
- [ ] Keybind customization

## v1.0 — Production

- [ ] Comprehensive error messages with actionable guidance
- [ ] Rate limit handling — backoff + retry
- [ ] Audit log — record all tool executions
- [ ] Config validation on startup
- [ ] man page and shell completions (bash, zsh, fish)
- [ ] Benchmark suite — render, throughput, memory

## Non-goals

- GUI — terminal tool
- Built-in editor — use your editor
- Multi-user — single user, single session
