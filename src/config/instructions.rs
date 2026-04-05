/// Project instructions — discover and load AGENTS.md / CLAUDE.md / RULES.md / COPILOT.md.
///
/// Priority (first found wins):
///   1. AGENTS.md          (emerging standard)
///   2. CLAUDE.md           (Claude Code convention)
///   3. .claude/settings.json → "instructions" field
///   4. RULES.md
///   5. COPILOT.md / .github/copilot-instructions.md
///
/// Also walks parent directories up to the git root (or filesystem root)
/// to collect inherited instructions, like Claude Code does.
use std::fs;
use std::path::{Path, PathBuf};

/// A discovered instruction file with its source path and content.
#[derive(Debug, Clone)]
pub struct Instruction {
    pub path: PathBuf,
    pub content: String,
}

/// Discover and load all project instructions from CWD up to the repo root.
/// Returns instructions ordered from root → CWD (outermost first, so CWD overrides).
pub fn discover() -> Vec<Instruction> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = find_git_root(&cwd).unwrap_or_else(|| cwd.clone());

    let mut dirs = Vec::new();
    let mut dir = cwd.as_path();
    loop {
        dirs.push(dir.to_owned());
        if dir == root {
            break;
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p,
            _ => break,
        }
    }
    // Root first, CWD last — outermost instructions come first.
    dirs.reverse();

    let mut instructions = Vec::new();
    for d in &dirs {
        if let Some(instr) = load_from_dir(d) {
            instructions.push(instr);
        }
    }
    instructions
}

/// Build the instructions block for system prompt injection.
/// Returns empty string if no instructions found.
pub fn build_instructions(instructions: &[Instruction]) -> String {
    if instructions.is_empty() {
        return String::new();
    }

    let mut out = String::from("\n<project_instructions>\n");
    for instr in instructions {
        out.push_str(&format!(
            "# From: {}\n{}\n\n",
            instr.path.display(),
            instr.content.trim()
        ));
    }
    out.push_str("</project_instructions>\n");
    out
}

/// Try to load an instruction file from a single directory, in priority order.
fn load_from_dir(dir: &Path) -> Option<Instruction> {
    // Priority order — first found wins for each directory.
    let candidates: &[&str] = &[
        "AGENTS.md",
        "CLAUDE.md",
        "RULES.md",
        "COPILOT.md",
        ".github/copilot-instructions.md",
    ];

    for &name in candidates {
        let path = dir.join(name);
        if let Ok(content) = fs::read_to_string(&path)
            && !content.trim().is_empty()
        {
            return Some(Instruction { path, content });
        }
    }

    // Fallback: .claude/settings.json → "instructions" field
    let settings_path = dir.join(".claude").join("settings.json");
    if let Ok(raw) = fs::read_to_string(&settings_path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(instr) = v.get("instructions").and_then(|v| v.as_str())
        && !instr.trim().is_empty()
    {
        return Some(Instruction {
            path: settings_path,
            content: instr.to_owned(),
        });
    }

    None
}

/// Walk up from `start` to find the git repository root.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_owned());
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p,
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn agents_md_takes_priority() {
        let dir = std::env::temp_dir().join("luma_instr_test_priority");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("AGENTS.md"), "Use Rust idioms.").unwrap();
        fs::write(dir.join("CLAUDE.md"), "Use Claude style.").unwrap();
        fs::write(dir.join("RULES.md"), "Follow rules.").unwrap();

        let instr = load_from_dir(&dir).unwrap();
        assert!(instr.path.ends_with("AGENTS.md"));
        assert!(instr.content.contains("Rust idioms"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_claude_md() {
        let dir = std::env::temp_dir().join("luma_instr_test_fallback");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("CLAUDE.md"), "Claude instructions").unwrap();

        let instr = load_from_dir(&dir).unwrap();
        assert!(instr.path.ends_with("CLAUDE.md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_settings_json() {
        let dir = std::env::temp_dir().join("luma_instr_test_settings");
        let _ = fs::remove_dir_all(&dir);
        let claude_dir = dir.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        fs::write(
            claude_dir.join("settings.json"),
            r#"{"instructions": "Always use snake_case."}"#,
        )
        .unwrap();

        let instr = load_from_dir(&dir).unwrap();
        assert!(instr.content.contains("snake_case"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_skipped() {
        let dir = std::env::temp_dir().join("luma_instr_test_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("AGENTS.md"), "  \n  ").unwrap();
        fs::write(dir.join("CLAUDE.md"), "Actual instructions").unwrap();

        let instr = load_from_dir(&dir).unwrap();
        assert!(instr.path.ends_with("CLAUDE.md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_instructions_returns_none() {
        let dir = std::env::temp_dir().join("luma_instr_test_none");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert!(load_from_dir(&dir).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_instructions_empty() {
        assert!(build_instructions(&[]).is_empty());
    }

    #[test]
    fn build_instructions_format() {
        let instrs = vec![Instruction {
            path: PathBuf::from("/project/AGENTS.md"),
            content: "Be concise.".into(),
        }];
        let out = build_instructions(&instrs);
        assert!(out.contains("<project_instructions>"));
        assert!(out.contains("# From: /project/AGENTS.md"));
        assert!(out.contains("Be concise."));
        assert!(out.contains("</project_instructions>"));
    }

    #[test]
    fn find_git_root_works() {
        // This test runs inside the luma repo itself
        let cwd = std::env::current_dir().unwrap();
        if cwd.join(".git").exists() {
            assert_eq!(find_git_root(&cwd), Some(cwd));
        }
    }
}
