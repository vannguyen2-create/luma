/// System prompt — composed from template files via `include_str!`.
///
/// Templates live in `src/config/prompt/`. Each has `{tools}` placeholder
/// filled at compose time.
///
/// | Mode  | Claude          | OpenAI / Codex              |
/// |-------|-----------------|-----------------------------|
/// | Rush  | rush.md (tool hint swapped)                   |
/// | Smart | smart + claude  | smart + openai/codex + extra |
/// | Deep  | deep + claude   | deep + openai/codex + extra  |
use crate::config::models::AgentMode;

// ── Templates (compile-time embedded) ──────────────────────────────────

const RUSH: &str = include_str!("prompt/rush.md");
const SMART: &str = include_str!("prompt/smart.md");
const DEEP: &str = include_str!("prompt/deep.md");

const TOOLS_CLAUDE: &str = include_str!("prompt/tools_claude.md");
const TOOLS_OPENAI: &str = include_str!("prompt/tools_openai.md");
const TOOLS_CODEX: &str = include_str!("prompt/tools_codex.md");

const OPENAI_EXTRA: &str = include_str!("prompt/openai_extra.md");
const OPENAI_EXAMPLES: &str = include_str!("prompt/openai_examples.md");

// ── Compose ────────────────────────────────────────────────────────────

/// Build the complete system prompt for a given model source and agent mode.
pub fn build(source: &str, mode: AgentMode) -> String {
    let (template, tools) = match (mode, source) {
        (AgentMode::Rush, _) => return rush(source),
        (AgentMode::Smart, "anthropic") => (SMART, TOOLS_CLAUDE),
        (AgentMode::Deep, "anthropic") => (DEEP, TOOLS_CLAUDE),
        (AgentMode::Smart, "codex") => (SMART, TOOLS_CODEX),
        (AgentMode::Deep, "codex") => (DEEP, TOOLS_CODEX),
        (AgentMode::Smart, _) => (SMART, TOOLS_OPENAI),
        (AgentMode::Deep, _) => (DEEP, TOOLS_OPENAI),
    };

    let base = template.replace("{tools}", tools);

    match (mode, source) {
        (AgentMode::Smart, "anthropic") | (AgentMode::Deep, "anthropic") => base,
        (AgentMode::Smart, _) => format!("{base}\n\n{OPENAI_EXTRA}\n\n{OPENAI_EXAMPLES}"),
        (AgentMode::Deep, _) => format!("{base}\n\n{OPENAI_EXTRA}"),
        _ => base,
    }
}

fn rush(source: &str) -> String {
    let tools = match source {
        "codex" => {
            "Use `exec_command` (with `rg` to search, `cat` to read) and `apply_patch` to edit."
        }
        _ => "Use `Read`, `Edit`, `Grep`, `Glob`, and `Bash`.",
    };
    RUSH.replace("{tools}", tools)
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rush_is_short() {
        let claude = build("anthropic", AgentMode::Rush);
        let codex = build("codex", AgentMode::Rush);
        assert!(claude.len() < 700, "Rush Claude too long: {}", claude.len());
        assert!(codex.len() < 700, "Rush Codex too long: {}", codex.len());
    }

    #[test]
    fn rush_claude_has_correct_tools() {
        let p = build("anthropic", AgentMode::Rush);
        assert!(p.contains("`Read`"));
        assert!(p.contains("`Edit`"));
        assert!(!p.contains("apply_patch"));
    }

    #[test]
    fn rush_codex_has_correct_tools() {
        let p = build("codex", AgentMode::Rush);
        assert!(p.contains("`apply_patch`"));
        assert!(p.contains("`rg`"));
        assert!(!p.contains("Use `Read`"));
    }

    #[test]
    fn claude_smart_structure() {
        let p = build("anthropic", AgentMode::Smart);
        assert!(p.contains("# Agency"));
        assert!(p.contains("dedicated tools"));
        assert!(p.contains("# Git Safety"));
        assert!(p.contains("# Pragmatism"));
        assert!(p.contains("# Handling Ambiguity"));
        assert!(!p.contains("Verification Gates"));
        assert!(!p.contains("Autonomy"));
    }

    #[test]
    fn claude_deep_structure() {
        let p = build("anthropic", AgentMode::Deep);
        assert!(p.contains("pragmatic, effective software engineer"));
        assert!(p.contains("# Autonomy"));
        assert!(p.contains("# Editing Constraints"));
        assert!(p.contains("# Review Mindset"));
        assert!(!p.contains("Verification Gates"));
        assert!(!p.contains("# Agency"));
    }

    #[test]
    fn openai_smart_structure() {
        let p = build("openai", AgentMode::Smart);
        assert!(p.contains("# Guardrails"));
        assert!(p.contains("dedicated tools"));
        assert!(p.contains("# Verification Gates"));
        assert!(p.contains("# Working Examples"));
        assert!(!p.contains("Autonomy"));
    }

    #[test]
    fn openai_deep_structure() {
        let p = build("openai", AgentMode::Deep);
        assert!(p.contains("# Autonomy"));
        assert!(p.contains("# Guardrails"));
        assert!(p.contains("# Verification Gates"));
        assert!(!p.contains("# Working Examples"));
    }

    #[test]
    fn codex_smart_has_correct_tools() {
        let p = build("codex", AgentMode::Smart);
        assert!(p.contains("`apply_patch`"));
        assert!(p.contains("`rg`"));
        assert!(p.contains("# Verification Gates"));
    }

    #[test]
    fn codex_deep_has_correct_tools() {
        let p = build("codex", AgentMode::Deep);
        assert!(p.contains("`apply_patch`"));
        assert!(p.contains("`rg`"));
        assert!(p.contains("# Autonomy"));
    }

    #[test]
    fn all_variants_have_git_safety() {
        for source in &["anthropic", "codex", "openai"] {
            for mode in &[AgentMode::Rush, AgentMode::Smart, AgentMode::Deep] {
                let p = build(source, *mode);
                assert!(
                    p.contains("reset --hard") || p.contains("destructive"),
                    "Missing git safety: {source} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn all_variants_have_emoji_rule() {
        for source in &["anthropic", "codex", "openai"] {
            for mode in &[AgentMode::Rush, AgentMode::Smart, AgentMode::Deep] {
                let p = build(source, *mode);
                assert!(p.contains("emoji"), "Missing emoji rule: {source} {mode:?}");
            }
        }
    }

    #[test]
    fn no_leftover_placeholders() {
        for source in &["anthropic", "codex", "openai"] {
            for mode in &[AgentMode::Rush, AgentMode::Smart, AgentMode::Deep] {
                let p = build(source, *mode);
                assert!(
                    !p.contains("{tools}"),
                    "Unresolved {{tools}}: {source} {mode:?}"
                );
                assert!(
                    !p.contains("{tool_list}"),
                    "Unresolved {{tool_list}}: {source} {mode:?}"
                );
            }
        }
    }
}
