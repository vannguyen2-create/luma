# luma

> Lightweight coding agent built with Rust. Uses Anthropic Claude and OpenAI Codex as backends.

![demo](demo.gif)

## Features

**Three operation modes** — switch with `Tab`:

| Mode | Default model | Fallback | Use case |
|------|---------------|----------|----------|
| **Rush** | Claude Haiku | Sonnet | Quick fixes, simple questions |
| **Smart** | Claude Opus | Sonnet | Code review, complex problems |
| **Deep** | Codex | Opus | Advanced analysis, research |

**Built-in tools**: `read`, `write`, `edit`, `bash`, `grep`, `glob`, `apply_patch`

**Skills** — compatible with Claude Code skill format. Scanned from:
- `.agents/skills/` and `.claude/skills/` (project-level, higher priority)
- `~/.agents/skills/`, `~/.claude/skills/`, `~/.config/luma/skills/` (user-level)

**Zero-config auth** — reuses credentials from Claude Code (macOS Keychain or `~/.claude/.credentials.json`) and Codex CLI (`~/.codex/auth.json`). OAuth tokens auto-refresh.

## Install

Requires **Rust 1.85+** (edition 2024). macOS only for now.

```bash
git clone https://github.com/nghyane/luma.git
cd luma
cargo build --release
cp target/release/luma ~/.local/bin/
```

## Usage

```bash
luma              # start TUI
luma sync         # sync models from Anthropic & OpenAI
luma auth         # check credential status
luma version      # show version
luma help         # show help
```

First run calls `luma sync` automatically.

### TUI commands

| Command | Description |
|---------|-------------|
| `/new` | New conversation thread |
| `/mode` | Pick mode (Rush / Smart / Deep) |
| `/model` | Pick specific model |
| `/sessions` | Resume a saved session |
| `/exit` | Quit |

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Cycle mode (Rush → Smart → Deep) |
| `Ctrl+T` | Cycle thinking level (Off → Low → Medium → High) |
| `Esc` | Cancel / close picker |
| `Alt+Enter` | Newline in input |
| `Ctrl+C` | Exit |

## Configuration

Preferences are stored in `~/.config/luma/preferences.json` (mode, model per mode, thinking level). Changed automatically through TUI — no manual editing needed.

### Debug logging

```bash
export LUMA_DEBUG=1
luma
# logs written to /tmp/luma.log
```

## Authentication

LUMA does not have its own login. It reads existing credentials:

| Provider | Source | Priority |
|----------|--------|----------|
| Anthropic | macOS Keychain (`Claude Code-credentials`) | 1st |
| Anthropic | `~/.claude/.credentials.json` | 2nd |
| OpenAI | `~/.codex/auth.json` | — |

If credentials are missing, install and log in through the official tools:
- [Claude Code](https://github.com/anthropics/claude-code)
- [Codex CLI](https://github.com/openai/codex)

Cached tokens are stored in `~/.config/luma/auth.json` and refreshed automatically when expired.

## Development

```bash
git clone https://github.com/nghyane/luma.git
cd luma
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

See [SECURITY.md](SECURITY.md).

## License

MIT — see [LICENSE](LICENSE).

Copyright (c) 2025 Nghyane
