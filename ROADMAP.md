# Roadmap

Current state: 15K lines Rust, 287 tests, 3 providers, 9 tools.

## v0.2 — Done

- [x] Tool diff colors + syntax highlighting (language-aware from file path)
- [x] Wire name removal — tools carry API-native names directly
- [x] Prompt templates extracted to files (`src/config/prompt/`)
- [x] Tool improvements: BufReader + binary detection (Read), deadline timeout + head/tail truncation (Bash), skip unchanged (Write), curly-quote normalization (Edit), `ignore` crate (Glob/Grep)
- [x] Web search — capability-based server tools + client fallback (Exa/Tavily/SearXNG)
- [x] Non-blocking input thread (try_send)
- [x] Lazy block rendering (defer to visible window)
- [x] Session resume preserves original ID
- [x] Save on error/abort for crash recovery
- [x] Auto-resume last session on app start
- [x] apply_patch @@ context hints for fuzzy scope matching

## v0.3 — Context & Multimodal

- [x] **Multimodal Message type** — `content: String` → `content: Vec<ContentBlock>` with `Text(String)` and `Image { media_type, id }`. Backward-compatible session deserialization via custom serde.
- [x] **@file mention** — type `@` in prompt → fuzzy autocomplete file paths (ignore-aware) → inject file content into message context. Highlight `@path` in peach. Tab ghost completion.
- [x] **Image attach** — clipboard paste (Alt+V) → save to session assets → multimodal message. Provider serialization per format (Anthropic base64 source, OpenAI image_url data URI).
- [ ] URL highlighting in markdown text (bare https:// links colored)
- [ ] Session management: `/delete`, `/rename`, `/export` commands
- [ ] `Block::Success` usage — show success messages for session save, compact, reset

## v0.4 — Performance & Polish

- [ ] Incremental markdown rendering — parse only new tokens, not full block re-render
- [ ] Diff algorithm: replace LCS O(n*m) memory with Myers O(n+m) for large files
- [ ] Diff line count in tool output for Write/Edit ("Updated file.rs (+5 -3)")
- [ ] Terminal resize during streaming — verify no layout corruption
- [ ] Streaming freeze profiling — large sessions (65K+ tokens)

## v0.5 — UX

- [ ] `/command` quick invocation — user types `/commit`, `/simplify` → inject predefined prompt
- [ ] File watcher — detect external file changes during session, warn before overwrite
- [ ] Copy selection to clipboard (mouse or keyboard)
- [ ] Search within output (Ctrl+F)
- [ ] Multi-line prompt input (shift+enter)
- [ ] Prompt history (up/down arrow cycles previous inputs)

## v0.5 — Extensibility

- [ ] MCP server support — connect to external tool servers
- [ ] Custom tools via config — define tools with shell commands + schema
- [ ] Plugin system — load .wasm or .so tools at runtime
- [ ] Themes — user-configurable color schemes via JSON
- [ ] Keybind customization via config file

## v1.0 — Production

- [ ] Comprehensive error messages — every error has actionable guidance
- [ ] Offline mode — queue messages when API unreachable, retry on reconnect
- [ ] Rate limit handling — backoff + retry with user notification
- [ ] Audit log — record all tool executions for compliance
- [ ] Config validation — check config files on startup, report issues
- [ ] man page and shell completions (bash, zsh, fish)
- [ ] Automated release pipeline (GitHub Actions + cross-compile)
- [ ] Benchmark suite — render performance, token throughput, memory usage

## Non-goals

- GUI — this is a terminal tool
- Built-in editor — use your editor, we handle the AI
- Multi-user — single user, single session at a time
- Backward compatibility with Claude Code session format — clean break
