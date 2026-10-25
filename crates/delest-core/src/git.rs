//! What git says about a path, asked through the `git` command and never by reading
//! its internals.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No repository contains the path.
    Outside,
    /// A repository contains the path, but git could not answer.
    Unreadable,
    /// The path holds files tracked by git.
    Tracked {
        files: usize,
    },
    /// Neither tracked nor ignored: git shows it as new work.
    Untracked,
    Ignored,
}

#[derive(Clone, Debug)]
pub struct GitState {
    /// The working tree that contains the path.
    pub root: Option<PathBuf>,
    pub status: Status,
    pub last_commit_days: Option<u64>,
}

/// The working tree above `path`: the nearest parent that holds a `.git` entry.
pub fn working_tree(path: &Path) -> Option<PathBuf> {
    path.parent()?
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

pub fn state(path: &Path) -> GitState {
    let Some(root) = working_tree(path) else {
        return GitState {
            root: None,
            status: Status::Outside,
            last_commit_days: None,
        };
    };
    let status = status_in(&root, path);
    let last_commit_days = last_commit_days(&root);
    GitState {
        root: Some(root),
        status,
        last_commit_days,
    }
}

fn status_in(root: &Path, path: &Path) -> Status {
    let Some(listed) = git(
        root,
        [
            OsStr::new("ls-files"),
            "-z".as_ref(),
            "--".as_ref(),
            path.as_os_str(),
        ],
    ) else {
        return Status::Unreadable;
    };
    if !listed.status.success() {
        return Status::Unreadable;
    }
    let files = listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .count();
    if files > 0 {
        return Status::Tracked { files };
    }
    let Some(ignored) = git(
        root,
        [
            OsStr::new("check-ignore"),
            "-q".as_ref(),
            "--".as_ref(),
            path.as_os_str(),
        ],
    ) else {
        return Status::Unreadable;
    };
    match ignored.status.code() {
        Some(0) => Status::Ignored,
        Some(1) => Status::Untracked,
        _ => Status::Unreadable,
    }
}

fn last_commit_days(root: &Path) -> Option<u64> {
    let output = git(root, ["log", "-1", "--format=%ct"])?;
    let seconds: u64 = String::from_utf8(output.stdout).ok()?.trim().parse().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(now.saturating_sub(seconds) / 86_400)
}

fn git<I, S>(dir: &Path, args: I) -> Option<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{repository, run};

    #[test]
    fn tracked_ignored_untracked_and_outside() {
        let repo = repository();
        let root = repo.path();
        fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(root.join("ignored")).unwrap();
        fs::write(root.join("ignored/a.o"), "x").unwrap();
        fs::create_dir_all(root.join("tracked")).unwrap();
        fs::write(root.join("tracked/icon.png"), "x").unwrap();
        fs::create_dir_all(root.join("new")).unwrap();
        fs::write(root.join("new/work.rs"), "x").unwrap();
        run(root, &["add", ".gitignore", "tracked"]);
        run(root, &["commit", "-q", "-m", "init"]);

        assert_eq!(state(&root.join("ignored")).status, Status::Ignored);
        assert_eq!(
            state(&root.join("tracked")).status,
            Status::Tracked { files: 1 }
        );
        assert_eq!(state(&root.join("new")).status, Status::Untracked);
        assert_eq!(state(&root.join("ignored")).last_commit_days, Some(0));

        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("build")).unwrap();
        assert_eq!(state(&outside.path().join("build")).status, Status::Outside);
    }
}
