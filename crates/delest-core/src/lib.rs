//! Core library of Delest. The command, the hooks and the interface are thin front
//! ends over it.

pub mod acceptance;
pub mod activity;
pub mod advisor;
pub mod agents;
pub mod alerts;
pub mod answers;
pub mod arbiter;
pub mod capacities;
pub mod clean;
pub mod engine;
pub mod git;
pub mod history;
pub mod index;
pub mod install;
pub mod journal;
pub mod lang;
pub mod memory;
pub mod models;
pub mod paths;
pub mod processes;
pub mod quarantine;
pub mod report;
pub mod rules;
pub mod scan;
pub mod timer;
pub mod update;
pub mod verdict;
pub mod volume;

#[cfg(test)]
pub(crate) mod testutil;

use std::path::{Component, Path, PathBuf};

/// The user's home directory; the current directory if none can be determined.
pub fn home_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Replace a leading `~` with the home directory.
pub fn expand_home(path: &Path, home: &Path) -> PathBuf {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => home.join(components.as_path()),
        _ => path.to_path_buf(),
    }
}

/// The path with the home directory shown as `~`, for display.
pub fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
