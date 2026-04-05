# luma

> Lightweight coding agent built with Rust. Uses Anthropic Claude and OpenAI Codex as backends.

![demo](demo.gif?v=2)

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

**Zero-config auth** — reuses credentials from Claude Code and Codex CLI.

## Memory Usage

Measured at idle state on macOS (Apple Silicon):

| App | RAM (RSS) | Stack |
|-----|-----------|-------|
| **luma** | **12.8 MB** | 1 process, Rust |
| codex | 64.5 MB | 4 processes, Node + native |
| claude code | 207.6 MB | 2 processes, Node.js |

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

## Configuration

Preferences are stored in `~/.config/luma/preferences.json` (mode, model per mode, thinking level). Changed automatically through TUI — no manual editing needed.

### Debug logging

```bash
export LUMA_DEBUG=1
luma
# logs written to /tmp/luma.log
```

## Authentication

Reuses credentials from [Claude Code](https://github.com/anthropics/claude-code) and [Codex CLI](https://github.com/openai/codex). No separate login needed.

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
