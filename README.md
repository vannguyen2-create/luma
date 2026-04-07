# luma

> Lightweight coding agent built with Rust. Multi-provider AI support (Anthropic Claude, OpenAI Codex).

![demo](demo.gif)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/nghyane/luma/master/install.sh | sh
```

Update: `luma update`

## Features

**Three modes** — switch with `Tab`:

| Mode | Model | Use case |
|------|-------|----------|
| Rush | Haiku | Quick fixes, simple questions |
| Smart | Opus | Code review, complex problems |
| Deep | Codex | Advanced analysis, research |

**@file mention** — type `@` in prompt to autocomplete file paths. File content injected as context when sent. Multiple files supported.

**Image paste** — `Alt+V` to paste clipboard image. Sent as multimodal content to the model.

**Tools** — `Read`, `Write`, `Edit`, `Bash`, `Grep`, `Glob`, `apply_patch`. Web search via server-side (Claude) or client fallback.

**Skills** — compatible with Claude Code skill format:
- `.agents/skills/`, `.claude/skills/` (project-level)
- `~/.agents/skills/`, `~/.claude/skills/`, `~/.config/luma/skills/` (user-level)

**Sessions** — `/resume` to continue last session, `/sessions` to browse all, `/new` to start fresh.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Cycle mode (Rush → Smart → Deep) |
| `Enter` | Send message |
| `Alt+Enter` | Newline in prompt |
| `Alt+V` | Paste clipboard image |
| `Ctrl+T` | Cycle thinking level |
| `Escape` | Abort streaming (press twice) |
| `Ctrl+C` | Abort streaming / clear input / quit |
| `↑` `↓` | History / dropdown navigation |

## Auth

Zero-config — reuses credentials from [Claude Code](https://github.com/anthropics/claude-code) and [Codex CLI](https://github.com/openai/codex). OAuth tokens auto-refresh.

## Configuration

Preferences stored in `~/.config/luma/preferences.json` — mode, model, thinking level. Changed through TUI, no manual editing needed.

Debug: `LUMA_DEBUG=1 luma` → logs to `/tmp/luma.log`

## License

MIT — see [LICENSE](LICENSE).
