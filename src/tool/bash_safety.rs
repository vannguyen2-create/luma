//! Command safety checks for the bash tool.

/// Dangerous command patterns — checked as substrings.
const DANGEROUS_SUBSTR: &[&str] = &["rm --no-preserve-root", "git reset --hard"];

/// Dangerous targets for recursive-force `rm`.
const DANGEROUS_RM_TARGETS: &[&str] = &[
    "/", "/*", "~", "$HOME", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib64", "/proc",
    "/root", "/sbin", "/sys", "/usr", "/var", "/opt", "/srv", "/tmp",
];

/// Dangerous base commands — matched only at command position.
const DANGEROUS_CMDS: &[&str] = &["mkfs", "dd"];

/// Commands that wrap another command — skipped when finding the real base command.
const CMD_WRAPPERS: &[&str] = &["sudo", "env", "nice", "nohup", "strace", "time"];

/// Check if a command contains any dangerous substring pattern.
pub fn contains_dangerous_substr(command: &str) -> bool {
    DANGEROUS_SUBSTR.iter().any(|p| command.contains(p))
}

/// Check if any segment of a piped/chained command is dangerous.
pub fn is_dangerous_cmd(command: &str) -> bool {
    for segment in command.split(&['|', ';'][..]) {
        for part in segment.split("&&").flat_map(|s| s.split("||")) {
            let tokens: Vec<&str> = part.split_whitespace().collect();
            let cmd_start = skip_wrappers(&tokens);
            let base = tokens.get(cmd_start).copied().unwrap_or("");
            if DANGEROUS_CMDS
                .iter()
                .any(|&cmd| base == cmd || base.starts_with(&format!("{cmd}.")))
            {
                return true;
            }
            if is_dangerous_rm(&tokens, cmd_start) {
                return true;
            }
            if is_dangerous_git_push(&tokens, cmd_start) {
                return true;
            }
            if is_shell_wrapper_bypass(&tokens, cmd_start) {
                return true;
            }
            if is_dangerous_chmod_chown(&tokens, cmd_start) {
                return true;
            }
        }
    }
    false
}

/// Skip leading wrapper commands, their flags, and env assignments.
fn skip_wrappers(tokens: &[&str]) -> usize {
    let mut i = 0;
    while i < tokens.len() {
        // Skip env assignments like FOO=bar
        if tokens[i].contains('=') && !tokens[i].starts_with('-') {
            i += 1;
            continue;
        }
        // Skip wrapper commands and their flags
        if CMD_WRAPPERS.iter().any(|&w| tokens[i] == w) {
            i += 1;
            // Skip flags belonging to the wrapper (e.g. sudo -u root)
            while i < tokens.len() && tokens[i].starts_with('-') {
                i += 1;
                // Skip flag value if it's not another flag (e.g. -u root)
                if i < tokens.len()
                    && !tokens[i].starts_with('-')
                    && !tokens[i].contains('=')
                    && !is_known_command(tokens[i])
                {
                    i += 1;
                }
            }
            continue;
        }
        break;
    }
    i
}

/// Heuristic: is this token likely a command name (not a flag value).
fn is_known_command(token: &str) -> bool {
    matches!(
        token,
        "rm" | "dd" | "mkfs" | "chmod" | "chown" | "git" | "bash" | "sh" | "zsh" | "mv" | "cp"
    )
}

/// Detect `rm` with both recursive and force flags targeting dangerous paths.
fn is_dangerous_rm(tokens: &[&str], cmd_start: usize) -> bool {
    if tokens.get(cmd_start).copied() != Some("rm") {
        return false;
    }

    let args = &tokens[cmd_start + 1..];
    let mut has_recursive = false;
    let mut has_force = false;

    for arg in args {
        if arg.starts_with('-') && !arg.starts_with("--") {
            if arg.contains('r') || arg.contains('R') {
                has_recursive = true;
            }
            if arg.contains('f') {
                has_force = true;
            }
        } else if *arg == "--recursive" {
            has_recursive = true;
        } else if *arg == "--force" {
            has_force = true;
        }
    }

    if !(has_recursive && has_force) {
        return false;
    }

    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        if is_dangerous_path(arg) {
            return true;
        }
    }
    false
}

/// Check if a path targets a dangerous location.
fn is_dangerous_path(arg: &str) -> bool {
    let normalized = arg.trim_end_matches('/');
    let check = if normalized.is_empty() {
        "/"
    } else {
        normalized
    };
    DANGEROUS_RM_TARGETS.iter().any(|&t| {
        let t_norm = t.trim_end_matches('/');
        let t_check = if t_norm.is_empty() { "/" } else { t_norm };
        check == t_check || *arg == format!("{t}*") || *arg == format!("{t}/*")
    })
}

/// Detect `git push --force` / `git push -f` but allow `--force-with-lease`.
fn is_dangerous_git_push(tokens: &[&str], cmd_start: usize) -> bool {
    if tokens.get(cmd_start).copied() != Some("git") {
        return false;
    }
    if tokens.get(cmd_start + 1).copied() != Some("push") {
        return false;
    }

    let args = &tokens[cmd_start + 2..];
    for arg in args {
        if *arg == "--force" || *arg == "-f" {
            return true;
        }
        // --force-with-lease is safer, don't block it
    }
    false
}

/// Detect `bash -c "rm -rf /"` style wrappers that embed dangerous commands.
fn is_shell_wrapper_bypass(tokens: &[&str], cmd_start: usize) -> bool {
    let base = tokens.get(cmd_start).copied().unwrap_or("");
    if !matches!(base, "bash" | "sh" | "zsh") {
        return false;
    }

    let args = &tokens[cmd_start + 1..];
    for (i, arg) in args.iter().enumerate() {
        if *arg == "-c" {
            // Everything after -c is the embedded command string
            let embedded: String = args[i + 1..].join(" ");
            // Remove surrounding quotes
            let inner = embedded.trim_matches(|c| c == '"' || c == '\'');
            if contains_dangerous_substr(inner) || is_dangerous_cmd(inner) {
                return true;
            }
            break;
        }
    }
    false
}

/// Detect `chmod -R 000 /` or `chown -R nobody:nogroup /` on dangerous paths.
fn is_dangerous_chmod_chown(tokens: &[&str], cmd_start: usize) -> bool {
    let base = tokens.get(cmd_start).copied().unwrap_or("");
    if !matches!(base, "chmod" | "chown") {
        return false;
    }

    let args = &tokens[cmd_start + 1..];
    let has_recursive = args.iter().any(|a| {
        *a == "-R"
            || *a == "--recursive"
            || (a.starts_with('-') && !a.starts_with("--") && a.contains('R'))
    });
    if !has_recursive {
        return false;
    }

    // Check non-flag args for dangerous paths (skip the mode/owner arg)
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        if is_dangerous_path(arg) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_cmd_detection() {
        assert!(is_dangerous_cmd("dd if=/dev/zero of=/dev/sda"));
        assert!(is_dangerous_cmd("mkfs.ext4 /dev/sda"));
        assert!(is_dangerous_cmd("echo x && dd if=a of=b"));
        assert!(is_dangerous_cmd("echo x | dd of=/dev/null"));

        assert!(!is_dangerous_cmd("git add ."));
        assert!(!is_dangerous_cmd("git add -A && git commit -m 'msg'"));
        assert!(!is_dangerous_cmd("git commit -m 'added feature'"));
        assert!(!is_dangerous_cmd("echo add something"));
        assert!(!is_dangerous_cmd("addr2line -e binary"));
    }

    #[test]
    fn dangerous_rm_variants() {
        // All bypass variants from issue #1
        assert!(is_dangerous_cmd("rm -rf /*"));
        assert!(is_dangerous_cmd("rm -r -f /"));
        assert!(is_dangerous_cmd("rm -rf $HOME"));
        assert!(is_dangerous_cmd("rm -rf ~"));
        assert!(is_dangerous_cmd("rm -rf /etc"));
        assert!(is_dangerous_cmd("rm -rf /usr"));
        assert!(is_dangerous_cmd("rm -rf /var"));
        assert!(is_dangerous_cmd("rm -rf /home"));
        assert!(is_dangerous_cmd("sudo rm -rf /"));
        assert!(is_dangerous_cmd("sudo rm -r -f /"));
        assert!(is_dangerous_cmd("rm -Rf /"));
        assert!(is_dangerous_cmd("rm --recursive --force /"));

        // Chained dangerous rm
        assert!(is_dangerous_cmd("echo hi && rm -rf /"));
        assert!(is_dangerous_cmd("ls | rm -rf /etc"));

        // Safe rm — should NOT block
        assert!(!is_dangerous_cmd("rm -rf ./build"));
        assert!(!is_dangerous_cmd("rm -rf target/"));
        assert!(!is_dangerous_cmd("rm -f file.txt"));
        assert!(!is_dangerous_cmd("rm -r ./tmp"));
        assert!(!is_dangerous_cmd("rm -rf /usr/local/bin/myapp"));
    }

    #[test]
    fn wrapper_bypass() {
        // sudo with flags
        assert!(is_dangerous_cmd("sudo -u root rm -rf /"));
        assert!(is_dangerous_cmd("sudo -E rm -rf /etc"));
        // env wrapper
        assert!(is_dangerous_cmd("env rm -rf /"));
        assert!(is_dangerous_cmd("env FOO=bar rm -rf /"));
        // nice/nohup wrappers
        assert!(is_dangerous_cmd("nice rm -rf /"));
        assert!(is_dangerous_cmd("nohup rm -rf /"));
        // env assignment prefix
        assert!(is_dangerous_cmd("FOO=bar rm -rf /"));
        // stacked wrappers
        assert!(is_dangerous_cmd("sudo env rm -rf /"));
    }

    #[test]
    fn shell_wrapper_bypass() {
        assert!(is_dangerous_cmd("bash -c \"rm -rf /\""));
        assert!(is_dangerous_cmd("sh -c 'rm -rf /etc'"));
        assert!(is_dangerous_cmd("bash -c \"sudo rm -rf /\""));
        // Safe embedded command
        assert!(!is_dangerous_cmd("bash -c \"echo hello\""));
        assert!(!is_dangerous_cmd("bash -c 'ls -la'"));
    }

    #[test]
    fn git_push_force() {
        // Should block
        assert!(is_dangerous_cmd("git push --force"));
        assert!(is_dangerous_cmd("git push -f"));
        assert!(is_dangerous_cmd("git push -f origin main"));
        assert!(is_dangerous_cmd("git push --force origin main"));
        // --force-with-lease is safer, allow it
        assert!(!is_dangerous_cmd("git push --force-with-lease"));
        assert!(!is_dangerous_cmd("git push --force-with-lease origin main"));
        // Normal push is fine
        assert!(!is_dangerous_cmd("git push"));
        assert!(!is_dangerous_cmd("git push origin main"));
    }

    #[test]
    fn chmod_chown_dangerous() {
        assert!(is_dangerous_cmd("chmod -R 000 /"));
        assert!(is_dangerous_cmd("chmod -R 777 /etc"));
        assert!(is_dangerous_cmd("chown -R nobody /"));
        assert!(is_dangerous_cmd("chown -R nobody:nogroup /usr"));
        // Without -R is less dangerous, allow
        assert!(!is_dangerous_cmd("chmod 644 /etc/hosts"));
        // Targeting safe path
        assert!(!is_dangerous_cmd("chmod -R 755 ./build"));
    }

    #[test]
    fn dangerous_substr_detection() {
        assert!(contains_dangerous_substr("rm --no-preserve-root /"));
        assert!(contains_dangerous_substr("git reset --hard HEAD~5"));
        assert!(!contains_dangerous_substr("echo hello"));
        // git push --force is now handled by git_push detection
        assert!(!contains_dangerous_substr("git push --force origin main"));
    }
}
