# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-05

### Added
- Initial release of LUMA
- Multi-provider support (Anthropic Claude, OpenAI Codex)
- Terminal User Interface (TUI) with interactive agent
- Three operation modes:
  - **Rush**: Fast responses with Claude Haiku (fallback Sonnet)
  - **Smart**: Balanced responses with Claude Opus (fallback Sonnet)
  - **Deep**: Advanced analysis with Codex (fallback Opus)
- Token usage tracking per session
- Skill system compatible with Claude Code format
- Session persistence and resumption
- Built-in tools: `read`, `write`, `edit`, `bash`, `grep`, `glob`, `apply_patch`
- Zero-config authentication (reuses existing Claude Code and Codex credentials)
- Automatic OAuth token refresh
- Syntax highlighting for code blocks
- Keyboard shortcuts and slash command system
