//! Helpers shared by the tests.

use std::path::Path;
use std::process::{Command, Stdio};

pub fn run(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// An empty git repository in a temporary folder.
pub fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["init", "-q"]);
    run(dir.path(), &["config", "user.email", "test@example.com"]);
    run(dir.path(), &["config", "user.name", "Test"]);
    dir
}

/// Commits everything git does not ignore.
pub fn commit_all(dir: &Path) {
    run(dir, &["add", "-A"]);
    run(dir, &["commit", "-q", "-m", "test"]);
}

/// A relative path written with "/" as the system writes it: "app\.env" on Windows.
pub fn native(path: &str) -> String {
    Path::new(path)
        .components()
        .collect::<std::path::PathBuf>()
        .display()
        .to_string()
}

/// Sets the modification time of a file or a folder. Windows opens a folder only
/// with the backup-semantics flag.
pub fn set_modified(path: &Path, time: std::time::SystemTime) {
    let mut options = std::fs::File::options();
    options.write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    options
        .open(path)
        .or_else(|_| std::fs::File::open(path))
        .unwrap()
        .set_modified(time)
        .unwrap();
}
