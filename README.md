# luma

> Lightweight coding agent built with Rust. Multi-provider AI support (Anthropic Claude, OpenAI Codex).

![demo](demo.gif)

## Install

macOS, Linux, WSL (aarch64, x86_64):

```bash
curl -fsSL https://raw.githubusercontent.com/nghyane/luma/master/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/nghyane/luma/master/install.ps1 | iex
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

**Inline attachments** — images and pasted text blocks insert at cursor position. Rendered as inline chips in both prompt and chat. Backspace removes them naturally.

**Drag & drop** — drop image files onto the terminal to attach. Supports `file://` paths, quoted paths, and common image formats.

**Tools** — `Read`, `Write`, `Edit`, `Bash`, `Grep`, `Glob`, `apply_patch`. Web search via server-side (Claude) or client fallback.

**Skills** — compatible with Claude Code skill format. Loaded from `.agents/skills/`, `.claude/skills/` (project) and `~/` equivalents.

**Sessions** — `/resume`, `/sessions`, `/new`. Attachments preserved across resume.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Cycle mode (Rush → Smart → Deep) |
| `Enter` | Send message |
| `Alt+Enter` | Newline in prompt |
| `Paste` | Text inline or block attachment (≥5 lines) |
| `Ctrl+T` | Cycle thinking level |
| `Esc` | Interrupt streaming (press twice to force) |
| `Ctrl+C` | Clear input / quit (when empty) |
| `↑` `↓` | History / dropdown navigation |

## Config

All data in `~/.config/luma/` — preferences, sessions, skills. Zero-config auth reuses [Claude Code](https://github.com/anthropics/claude-code) / [Codex CLI](https://github.com/openai/codex) credentials. OAuth auto-refresh.

Debug: `LUMA_DEBUG=1 luma` → `/tmp/luma.log`

## License

MIT — see [LICENSE](LICENSE).
