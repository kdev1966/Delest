//! The optional system timer, off by default: every four hours it runs the same
//! check as the agents' hook, for the work done without an agent. A launchd agent
//! on macOS, a systemd user timer on Linux, a task of the Task Scheduler on Windows.
//! The first check comes four hours after the timer is turned on, never at login.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const LABEL: &str = "delest.check";
pub const INTERVAL_SECONDS: u64 = 4 * 3600;
/// The systemd units' name, without the suffix.
const UNIT: &str = "delest-check";
/// The Task Scheduler's task.
const TASK: &str = "Delest check";

/// Whether this system has a timer Delest can use.
pub fn supported() -> bool {
    if cfg!(target_os = "macos") || cfg!(windows) {
        true
    } else if cfg!(target_os = "linux") {
        crate::paths::find_program("systemctl").is_some()
    } else {
        false
    }
}

/// Where the timer lives, for the developer: a file, or the task's name.
pub fn place(home: &Path, lang: crate::lang::Lang) -> String {
    if cfg!(windows) {
        crate::tr!(
            lang,
            "Task Scheduler › {TASK}",
            "Planificateur de tâches › {TASK}"
        )
    } else if cfg!(target_os = "macos") {
        crate::tilde(&plist_path(home), home)
    } else {
        crate::tilde(&units_dir(home).join(format!("{UNIT}.timer")), home)
    }
}

/// Turns the timer on, replacing an older one.
pub fn enable(home: &Path, exe: &Path) -> io::Result<()> {
    if cfg!(target_os = "macos") {
        launchd::enable(home, exe)
    } else if cfg!(windows) {
        scheduler::enable(exe)
    } else {
        systemd::enable(home, exe)
    }
}

/// Turns the timer off; nothing to do when it is not there.
pub fn disable(home: &Path) -> io::Result<()> {
    if cfg!(target_os = "macos") {
        launchd::disable(home)
    } else if cfg!(windows) {
        scheduler::disable()
    } else {
        systemd::disable(home)
    }
}

/// Whether the timer is installed and active.
pub fn enabled(home: &Path) -> bool {
    if cfg!(target_os = "macos") {
        launchd::enabled(home)
    } else if cfg!(windows) {
        scheduler::enabled()
    } else {
        systemd::enabled(home)
    }
}

fn quiet(program: &str, args: &[&str]) -> io::Result<bool> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

// ------------------------------------------------------------------ macOS

pub fn plist_path(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The launchd agent: `delest hook timer` every four hours, not at login.
pub fn plist(exe: &Path) -> String {
    let exe = escape(&exe.display().to_string());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>hook</string>
        <string>timer</string>
    </array>
    <key>StartInterval</key>
    <integer>{INTERVAL_SECONDS}</integer>
    <key>RunAtLoad</key>
    <false/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>LowPriorityIO</key>
    <true/>
</dict>
</plist>
"#
    )
}

mod launchd {
    use super::*;

    fn domain() -> io::Result<String> {
        let output = Command::new("id").arg("-u").output()?;
        let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if uid.is_empty() {
            return Err(io::Error::other("id -u gave no user id"));
        }
        Ok(format!("gui/{uid}"))
    }

    pub fn enable(home: &Path, exe: &Path) -> io::Result<()> {
        let path = plist_path(home);
        let domain = domain()?;
        let _ = quiet("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, plist(exe))?;
        if !quiet(
            "launchctl",
            &["bootstrap", &domain, &path.display().to_string()],
        )? {
            return Err(io::Error::other("launchctl bootstrap refused the timer"));
        }
        Ok(())
    }

    pub fn disable(home: &Path) -> io::Result<()> {
        let domain = domain()?;
        let _ = quiet("launchctl", &["bootout", &format!("{domain}/{LABEL}")]);
        remove(&plist_path(home))
    }

    pub fn enabled(home: &Path) -> bool {
        plist_path(home).is_file()
            && domain()
                .and_then(|domain| quiet("launchctl", &["print", &format!("{domain}/{LABEL}")]))
                .unwrap_or(false)
    }
}

// ------------------------------------------------------------------ Linux

/// The user's systemd units, in `$XDG_CONFIG_HOME/systemd/user` (`~/.config` by default).
fn units_dir(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("systemd/user")
}

/// A path for `ExecStart`: quoted, with systemd's specifiers (`%`) doubled.
fn exec_path(exe: &Path) -> String {
    let text = exe
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{text}\"")
}

/// The service the timer starts: one check, at the lowest priority.
pub fn service(exe: &Path) -> String {
    format!(
        "[Unit]\nDescription=Delest: disk check\n\n\
         [Service]\nType=oneshot\nExecStart={} hook timer\nNice=19\nIOSchedulingClass=idle\n",
        exec_path(exe)
    )
}

/// The timer: the first check four hours after it is turned on, then every four hours.
pub fn timer_unit() -> String {
    let hours = INTERVAL_SECONDS / 3600;
    format!(
        "[Unit]\nDescription=Delest: a disk check every {hours} hours\n\n\
         [Timer]\nOnActiveSec={hours}h\nOnUnitActiveSec={hours}h\nPersistent=false\n\n\
         [Install]\nWantedBy=timers.target\n"
    )
}

mod systemd {
    use super::*;

    fn systemctl(args: &[&str]) -> io::Result<bool> {
        let mut all = vec!["--user"];
        all.extend_from_slice(args);
        quiet("systemctl", &all)
    }

    pub fn enable(home: &Path, exe: &Path) -> io::Result<()> {
        let dir = units_dir(home);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{UNIT}.service")), service(exe))?;
        fs::write(dir.join(format!("{UNIT}.timer")), timer_unit())?;
        let _ = systemctl(&["daemon-reload"]);
        if !systemctl(&["enable", "--now", &format!("{UNIT}.timer")])? {
            return Err(io::Error::other("systemctl --user refused the timer"));
        }
        Ok(())
    }

    pub fn disable(home: &Path) -> io::Result<()> {
        let _ = systemctl(&["disable", "--now", &format!("{UNIT}.timer")]);
        let dir = units_dir(home);
        remove(&dir.join(format!("{UNIT}.timer")))?;
        remove(&dir.join(format!("{UNIT}.service")))?;
        let _ = systemctl(&["daemon-reload"]);
        Ok(())
    }

    pub fn enabled(home: &Path) -> bool {
        units_dir(home).join(format!("{UNIT}.timer")).is_file()
            && systemctl(&["is-active", "--quiet", &format!("{UNIT}.timer")]).unwrap_or(false)
    }
}

// ------------------------------------------------------------------ Windows

/// `schtasks /Create`: every four hours for this user, starting now, so the first
/// check comes in four hours. It runs through `conhost --headless`, so no console
/// window flashes on screen.
pub fn schtasks_create(exe: &Path) -> Vec<String> {
    let hours = (INTERVAL_SECONDS / 3600).to_string();
    let run = format!("conhost.exe --headless \"{}\" hook timer", exe.display());
    [
        "/Create", "/F", "/TN", TASK, "/SC", "HOURLY", "/MO", &hours, "/TR", &run,
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

mod scheduler {
    use super::*;

    pub fn enable(exe: &Path) -> io::Result<()> {
        let args = schtasks_create(exe);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if !quiet("schtasks", &args)? {
            return Err(io::Error::other("the Task Scheduler refused the timer"));
        }
        Ok(())
    }

    pub fn disable() -> io::Result<()> {
        let _ = quiet("schtasks", &["/Delete", "/F", "/TN", TASK]);
        Ok(())
    }

    pub fn enabled() -> bool {
        quiet("schtasks", &["/Query", "/TN", TASK]).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plist_runs_the_hook_every_four_hours_and_escapes_the_path() {
        let text = plist(Path::new("/Users/a&b/.cargo/bin/delest"));
        assert!(text.contains("<string>/Users/a&amp;b/.cargo/bin/delest</string>"));
        assert!(text.contains("<string>timer</string>"));
        assert!(text.contains("<integer>14400</integer>"));
        assert!(text.contains("<key>RunAtLoad</key>\n    <false/>"));
    }

    #[test]
    fn the_systemd_units_run_the_hook_every_four_hours_not_at_login() {
        let service = service(Path::new("/home/a b/50%/delest"));
        assert!(service.contains("ExecStart=\"/home/a b/50%%/delest\" hook timer\n"));
        assert!(service.contains("Type=oneshot"));
        let timer = timer_unit();
        assert!(timer.contains("OnActiveSec=4h\nOnUnitActiveSec=4h\n"));
        assert!(!timer.contains("OnBootSec"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    #[test]
    fn the_task_runs_the_hook_every_four_hours_without_a_window() {
        let args = schtasks_create(Path::new(r"C:\Users\a b\delest.exe"));
        let at = |flag: &str| args[args.iter().position(|arg| arg == flag).unwrap() + 1].clone();
        assert_eq!(at("/SC"), "HOURLY");
        assert_eq!(at("/MO"), "4");
        assert!(!args.iter().any(|arg| arg == "/ST"));
        assert_eq!(
            at("/TR"),
            r#"conhost.exe --headless "C:\Users\a b\delest.exe" hook timer"#
        );
    }
}
