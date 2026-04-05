

/// Debug log to /tmp/luma.log — enabled by LUMA_DEBUG=1.
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        if std::env::var("LUMA_DEBUG").is_ok() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true)
                .open("/tmp/luma.log")
            {
                let _ = writeln!(f, "[{:.3}] {}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default().as_secs_f64() % 100000.0,
                    format!($($arg)*)
                );
            }
        }
    };
}

mod core;
mod config;
mod event;
mod provider;
mod tool;
mod tui;

use std::process::Command;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str());

    match cmd {
        Some("sync") => {
            println!("syncing models...");
            match config::models::sync().await {
                Ok(count) => println!("synced {count} models"),
                Err(e) => { eprintln!("sync failed: {e}"); std::process::exit(1); }
            }
        }
        Some("auth") => {
            for provider in [config::auth::AuthProvider::Anthropic, config::auth::AuthProvider::OpenAI] {
                let name = match provider { config::auth::AuthProvider::Anthropic => "anthropic", config::auth::AuthProvider::OpenAI => "openai" };
                match config::auth::resolve(provider).await {
                    Ok(auth) => println!("{name}: {} (ok)", if auth.is_oauth { "oauth" } else { "apikey" }),
                    Err(e) => println!("{name}: {e}"),
                }
            }
        }
        Some("version") => println!("luma 0.1.0"),
        Some("help" | "--help" | "-h") => {
            println!("luma - lightweight coding agent\n\nusage:\n  luma              start TUI\n  luma sync         sync models\n  luma auth         show auth\n  luma version      version");
        }
        Some(unknown) => {
            eprintln!("unknown command: {unknown}\nrun 'luma help'");
            std::process::exit(1);
        }
        None => {
            if !config::models::has_synced() {
                println!("first run — syncing models...");
                if let Err(e) = config::models::sync().await {
                    eprintln!("sync failed: {e}");
                    std::process::exit(1);
                }
                println!("done");
            }

            let env_context = build_env_context();
            let app = tui::app::App::new(env_context);
            if let Err(e) = app.run().await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn build_env_context() -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());

    // Git info
    let is_git = cmd_ok(&cwd, "git", &["rev-parse", "--is-inside-work-tree"]);
    let git_branch = if is_git {
        cmd_stdout(&cwd, "git", &["rev-parse", "--abbrev-ref", "HEAD"])
    } else {
        None
    };
    let git_remote = if is_git {
        cmd_stdout(&cwd, "git", &["remote", "get-url", "origin"])
    } else {
        None
    };

    // Detect CLIs based on project files — only check tools relevant to this project.
    let mut cli_candidates: Vec<(&str, &str)> = vec![
        ("rg", "--version"),
        ("git", "--version"),
        ("gh", "--version"),
    ];

    let project_markers: &[(&str, &[(&str, &str)])] = &[
        ("Cargo.toml",      &[("cargo", "--version"), ("rustc", "--version")]),
        ("package.json",    &[("node", "--version"), ("npm", "--version"), ("pnpm", "--version"), ("yarn", "--version"), ("bun", "--version")]),
        ("Dockerfile",      &[("docker", "--version")]),
        ("docker-compose.yml", &[("docker", "--version")]),
        ("requirements.txt",&[("python3", "--version"), ("pip3", "--version")]),
        ("pyproject.toml",  &[("python3", "--version"), ("pip3", "--version")]),
        ("go.mod",          &[("go", "version")]),
        ("Makefile",        &[("make", "--version")]),
    ];

    let mut seen = std::collections::HashSet::new();
    for (marker, cmds) in project_markers {
        if cwd.join(marker).exists() {
            for &(cmd, flag) in *cmds {
                if seen.insert(cmd) {
                    cli_candidates.push((cmd, flag));
                }
            }
        }
    }

    let mut tools = Vec::new();
    for (cmd, flag) in &cli_candidates {
        if let Ok(out) = Command::new(cmd).arg(flag).output()
            && out.status.success()
        {
            let ver = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or(cmd).to_owned();
            tools.push(format!("{cmd} ({ver})"));
        }
    }

    // Build git line
    let git_info = if is_git {
        let mut parts = vec!["yes".to_owned()];
        if let Some(b) = &git_branch { parts.push(format!("branch={b}")); }
        if let Some(r) = &git_remote { parts.push(format!("remote={r}")); }
        parts.join(", ")
    } else {
        "no".into()
    };

    format!(
        "\nYou have tools: read (files/dirs), write (files), bash (shell commands).\n\n\
         <env>\n  OS: {} {}\n  Shell: {shell}\n  CWD: {}\n  Git: {git_info}\n  CLI: {}\n</env>",
        std::env::consts::OS, std::env::consts::ARCH,
        cwd.display(),
        tools.join(", "),
    )
}

/// Run a command and check success.
fn cmd_ok(cwd: &std::path::Path, cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).current_dir(cwd)
        .output().map(|o| o.status.success()).unwrap_or(false)
}

/// Run a command and return trimmed stdout on success.
fn cmd_stdout(cwd: &std::path::Path, cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd).args(args).current_dir(cwd).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
}
