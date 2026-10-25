//! Location of Delest's own data (journal, list of quarantines), following each
//! platform's conventions.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{expand_home, home_dir};

/// Overrides the data directory, for tests and unusual setups.
pub const ENV_HOME: &str = "DELEST_HOME";

/// The absolute, resolved path, in the form people and programs use. On Windows,
/// `fs::canonicalize` answers `\\?\C:\…`, which would not start with the home folder
/// `C:\Users\…` and which many programs refuse: the prefix is taken off.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(plain)
}

/// `\\?\C:\dir` → `C:\dir`, `\\?\UNC\server\share` → `\\server\share`; others unchanged.
pub fn plain(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    /// Linux and the other Unix systems, which follow the XDG conventions.
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Other
        }
    }
}

/// The directory that holds Delest's journal and state.
pub fn data_dir() -> PathBuf {
    data_dir_for(Os::current(), |name| env::var_os(name), &home_dir())
}

/// `DELEST_HOME` overrides the default, which is:
/// - Windows: `%LOCALAPPDATA%\delest`
/// - macOS: `~/Library/Application Support/delest`
/// - other systems: `$XDG_DATA_HOME/delest`, or `~/.local/share/delest`
pub fn data_dir_for(os: Os, var: impl Fn(&str) -> Option<OsString>, home: &Path) -> PathBuf {
    let set = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = set(ENV_HOME) {
        return expand_home(&dir, home);
    }
    match os {
        Os::Windows => set("LOCALAPPDATA")
            .unwrap_or_else(|| home.join("AppData").join("Local"))
            .join("delest"),
        Os::MacOs => home
            .join("Library")
            .join("Application Support")
            .join("delest"),
        Os::Other => set("XDG_DATA_HOME")
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("delest"),
    }
}

/// The folders where developer tools install their commands (Claude Code, Cargo,
/// Homebrew, npm, Bun, Scoop), which a program opened from the Finder, the Dock or
/// the Start menu does not have on its `PATH`.
fn tool_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".local").join("bin"),
        home.join(".claude").join("local"),
        home.join(".cargo").join("bin"),
        home.join(".npm-global").join("bin"),
        home.join(".bun").join("bin"),
    ];
    if cfg!(windows) {
        dirs.push(home.join("AppData").join("Roaming").join("npm"));
        dirs.push(home.join("scoop").join("shims"));
    } else {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// A program's file name: `delest.exe` on Windows.
pub fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// The file names a command can have: on Windows, npm installs `claude.cmd`.
fn command_names(name: &str) -> Vec<String> {
    if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|extension| format!("{name}.{extension}"))
            .collect()
    } else {
        vec![name.to_string()]
    }
}

/// The command `name` on `path` (a `PATH` value), else in the usual tool folders.
pub fn find_program_in(name: &str, path: Option<OsString>, home: &Path) -> Option<PathBuf> {
    let names = command_names(name);
    path.iter()
        .flat_map(env::split_paths)
        .chain(tool_dirs(home))
        .flat_map(|dir| names.iter().map(move |file| dir.join(file)))
        .find(|candidate| candidate.is_file())
}

/// The command `name`: on the `PATH`, else in the usual tool folders, else where
/// the developer's login shell finds it. The answer is kept for the process's life,
/// since asking the shell can take a second.
pub fn find_program(name: &str) -> Option<PathBuf> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static FOUND: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);
    let mut found = FOUND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let found = found.get_or_insert_with(HashMap::new);
    if let Some(known) = found.get(name) {
        return known.clone();
    }
    let program =
        find_program_in(name, env::var_os("PATH"), &home_dir()).or_else(|| from_login_shell(name));
    found.insert(name.to_string(), program.clone());
    program
}

/// `command -v <name>` in the developer's login shell, which reads their profile.
#[cfg(unix)]
fn from_login_shell(name: &str) -> Option<PathBuf> {
    use std::process::{Command, Stdio};
    // The name goes into a shell command: only plain command names are asked.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return None;
    }
    let shell = env::var_os("SHELL").filter(|shell| !shell.is_empty())?;
    let output = Command::new(shell)
        .args(["-l", "-i", "-c", &format!("command -v {name}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with('/'))
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

#[cfg(not(unix))]
fn from_login_shell(_: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_verbatim_paths_lose_their_prefix() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\Users\a\proj")),
            PathBuf::from(r"C:\Users\a\proj")
        );
        assert_eq!(
            plain(PathBuf::from(r"\\?\UNC\server\share\x")),
            PathBuf::from(r"\\server\share\x")
        );
        assert_eq!(
            plain(PathBuf::from(r"\\?\Volume{1}\x")),
            PathBuf::from(r"\\?\Volume{1}\x")
        );
        assert_eq!(plain(PathBuf::from("/home/a")), PathBuf::from("/home/a"));
    }

    #[test]
    fn a_tool_folder_is_searched_when_the_path_lacks_it() {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        fs_write(&bin.join(executable_name("claude")));
        // The PATH a program opened from the Finder gets.
        let path = Some(OsString::from("/usr/bin:/bin:/usr/sbin:/sbin"));
        assert_eq!(
            find_program_in("claude", path.clone(), home.path()),
            Some(bin.join(executable_name("claude")))
        );
        assert_eq!(
            find_program_in("nothing-like-this", path, home.path()),
            None
        );
    }

    #[test]
    fn the_path_comes_first() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join("first");
        fs_write(&first.join(executable_name("vario")));
        fs_write(
            &home
                .path()
                .join(".cargo/bin")
                .join(executable_name("vario")),
        );
        assert_eq!(
            find_program_in("vario", Some(first.clone().into_os_string()), home.path()),
            Some(first.join(executable_name("vario")))
        );
    }

    fn fs_write(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, OsString)> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), OsString::from(value)))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn defaults_per_platform() {
        let home = Path::new("/home/dev");
        assert_eq!(
            data_dir_for(Os::MacOs, env_of(&[]), home),
            home.join("Library/Application Support/delest")
        );
        assert_eq!(
            data_dir_for(Os::Other, env_of(&[]), home),
            home.join(".local/share/delest")
        );
        assert_eq!(
            data_dir_for(Os::Other, env_of(&[("XDG_DATA_HOME", "/data")]), home),
            PathBuf::from("/data/delest")
        );
    }

    #[test]
    fn the_override_wins_and_expands_the_home() {
        let home = Path::new("/home/dev");
        assert_eq!(
            data_dir_for(Os::MacOs, env_of(&[(ENV_HOME, "~/delest-test")]), home),
            home.join("delest-test")
        );
    }
}
