//! Platform shell — spawn the appropriate shell for the current OS.

/// Spawn a shell process to execute a command.
pub fn spawn(command: &str) -> std::io::Result<tokio::process::Child> {
    let mut cmd = platform::command(command);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.spawn()
}

#[cfg(unix)]
mod platform {
    /// Build a shell command using bash.
    pub fn command(command: &str) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("bash");
        cmd.arg("-c").arg(command);
        cmd
    }
}

#[cfg(windows)]
mod platform {
    /// Build a shell command using cmd.exe.
    pub fn command(command: &str) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo() {
        // "echo hello" works on both bash and cmd.
        let output = spawn("echo hello")
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn exit_code() {
        let output = spawn("exit 42").unwrap().wait_with_output().await.unwrap();
        assert_eq!(output.status.code(), Some(42));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn exit_code() {
        let output = spawn("exit /b 42")
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(output.status.code(), Some(42));
    }
}
