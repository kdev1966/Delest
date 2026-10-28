//! Whether a newer Delest has been published. The app asks GitHub at most once a
//! day, unless the developer turned it off; GitHub is sent the request alone.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The latest published release, as GitHub's API gives it.
pub const LATEST: &str = "https://api.github.com/repos/kdev1966/Delest/releases/latest";
/// Where a newer version is downloaded.
pub const DOWNLOAD: &str = "https://kdev1966.github.io/delest-site/";

const FILE: &str = "update.json";
const EVERY_SECONDS: i64 = 24 * 60 * 60;

/// The last answer, kept so the question is asked once a day.
#[derive(Debug, Serialize, Deserialize)]
struct Last {
    /// Seconds since the Unix epoch.
    at: i64,
    latest: Option<String>,
}

/// The newer version than `current` published, if any: the last answer when it is
/// less than a day old, else `fetch`'s, which is then kept.
pub fn newer_than(
    current: &str,
    data_dir: &Path,
    now: i64,
    fetch: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let path = data_dir.join(FILE);
    let fresh = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Last>(&text).ok())
        .filter(|last| (0..EVERY_SECONDS).contains(&(now - last.at)));
    let latest = if let Some(last) = fresh {
        last.latest
    } else {
        let latest = fetch();
        let kept = Last {
            at: now,
            latest: latest.clone(),
        };
        if let Ok(text) = serde_json::to_string(&kept) {
            let _ = fs::create_dir_all(data_dir);
            let _ = fs::write(&path, text);
        }
        latest
    };
    latest.filter(|latest| is_newer(latest, current))
}

/// Asks GitHub for the latest release's version, with `curl`; `None` without an answer.
pub fn fetch_latest(current: &str) -> Option<String> {
    let mut command = Command::new("curl");
    command
        .args(["--fail", "--silent", "--location", "--max-time", "10"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .args(["--user-agent", &format!("Delest/{current}")])
        .arg(LATEST);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing over the app.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    tag_version(&String::from_utf8_lossy(&output.stdout))
}

/// "0.1.5" from a release's `tag_name` "v0.1.5".
fn tag_version(json: &str) -> Option<String> {
    let release: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = release.get("tag_name")?.as_str()?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    parse(version).map(|_| version.to_string())
}

fn parse(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// Whether `candidate` comes after `current`, number by number.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_number_by_number() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("0.2.0", "0.1.4"));
        assert!(!is_newer("0.1.4", "0.1.4"));
        assert!(!is_newer("0.1.3", "0.1.4"));
        assert!(!is_newer("0.1.5-beta", "0.1.4"));
    }

    #[test]
    fn the_tag_gives_the_version() {
        assert_eq!(
            tag_version(r#"{"tag_name":"v0.1.5","name":"x"}"#).as_deref(),
            Some("0.1.5")
        );
        assert_eq!(tag_version(r#"{"tag_name":"nightly"}"#), None);
        assert_eq!(tag_version("not json"), None);
    }

    #[test]
    fn github_is_asked_once_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let day = EVERY_SECONDS;
        let asked = std::cell::Cell::new(0);
        let fetch = |answer: &str| {
            asked.set(asked.get() + 1);
            Some(answer.to_string())
        };
        assert_eq!(
            newer_than("0.1.4", dir.path(), 1000, || fetch("0.1.5")).as_deref(),
            Some("0.1.5")
        );
        // Within the day: the kept answer, GitHub is not asked.
        assert_eq!(
            newer_than("0.1.4", dir.path(), 1000 + day - 1, || fetch("0.1.6")).as_deref(),
            Some("0.1.5")
        );
        assert_eq!(asked.get(), 1);
        // Once updated, the kept answer is no longer newer.
        assert_eq!(
            newer_than("0.1.5", dir.path(), 2000, || fetch("0.1.6")),
            None
        );
        // A day later, asked again.
        assert_eq!(
            newer_than("0.1.5", dir.path(), 1000 + day, || fetch("0.1.6")).as_deref(),
            Some("0.1.6")
        );
        assert_eq!(asked.get(), 2);
    }

    #[test]
    fn no_answer_is_no_update() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(newer_than("0.1.4", dir.path(), 1000, || None), None);
    }
}
