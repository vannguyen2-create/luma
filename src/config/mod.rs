pub mod auth;
pub mod instructions;
pub mod models;
pub mod prefs;
pub mod prompt;
pub mod skills;

use std::path::PathBuf;

/// Cross-platform home directory. Uses `HOME` on Unix, `USERPROFILE` on Windows.
pub fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
    }
}
