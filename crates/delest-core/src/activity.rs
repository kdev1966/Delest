//! When a project was last worked on: its last commit, the latest change to its
//! files, and its last Handoff save. The dependencies of a project worked on in
//! the last 30 days are not offered: they would be reinstalled right away.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::rules;

pub const INACTIVE_AFTER_DAYS: u64 = 30;
/// How deep, and how many entries, the files of a project are looked at.
const DEPTH: usize = 4;
const ENTRIES: usize = 20_000;

/// The last save of each project known to Handoff, read once through its command.
#[derive(Debug, Default)]
pub struct Handoff {
    projects: Vec<(PathBuf, i64)>,
}

#[derive(Deserialize)]
struct Project {
    root: PathBuf,
    last_handoff_at: Option<String>,
}

impl Handoff {
    /// Empty when Handoff is not installed: Delest works without it.
    pub fn load() -> Handoff {
        Command::new("handoff")
            .args(["list", "--json"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| Handoff::parse(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    }

    pub fn parse(json: &str) -> Handoff {
        let projects: Vec<Project> = serde_json::from_str(json).unwrap_or_default();
        Handoff {
            projects: projects
                .into_iter()
                .filter_map(|project| {
                    let time =
                        chrono::DateTime::parse_from_rfc3339(project.last_handoff_at.as_deref()?)
                            .ok()?;
                    Some((project.root, time.timestamp()))
                })
                .collect(),
        }
    }

    /// The last save of the project that holds `path`, the innermost one.
    pub fn last_save(&self, path: &Path) -> Option<i64> {
        self.projects
            .iter()
            .filter(|(root, _)| path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map(|(_, time)| *time)
    }
}

pub use crate::verdict::ActivitySource as Source;

/// Days since the project was last worked on, and what tells it.
pub fn idle_days(
    project: &Path,
    last_commit_days: Option<u64>,
    handoff: &Handoff,
) -> Option<(u64, Source)> {
    let now = now();
    let files =
        latest_change(project).map(|time| (now.saturating_sub(time) / 86_400, Source::Files));
    let commit = last_commit_days.map(|days| (days, Source::Commit));
    let saved = handoff
        .last_save(project)
        .map(|time| ((now as i64 - time).max(0) as u64 / 86_400, Source::Handoff));
    [commit, files, saved]
        .into_iter()
        .flatten()
        .min_by_key(|(days, _)| *days)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// The most recent modification among the project's own files: artefacts and the
/// git folder are skipped, and the walk is bounded.
fn latest_change(project: &Path) -> Option<u64> {
    let mut latest = None;
    let mut seen = 0;
    let mut pending = vec![(project.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > ENTRIES {
                return latest;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if kind.is_dir() {
                let skipped = name == ".git" || rules::rule_for(&name).is_some();
                if !skipped && depth + 1 < DEPTH {
                    pending.push((entry.path(), depth + 1));
                }
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_secs());
            latest = latest.max(modified);
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handoff_gives_the_innermost_project() {
        let handoff = Handoff::parse(
            r#"[
                {"root": "/w", "last_handoff_at": "2026-01-01T00:00:00+00:00"},
                {"root": "/w/app", "last_handoff_at": "2026-10-01T00:00:00+00:00"},
                {"root": "/w/old", "last_handoff_at": null}
            ]"#,
        );
        let october = chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00+00:00").unwrap();
        assert_eq!(
            handoff.last_save(Path::new("/w/app/node_modules")),
            Some(october.timestamp())
        );
        assert!(handoff.last_save(Path::new("/elsewhere")).is_none());
        assert!(
            Handoff::parse("not json")
                .last_save(Path::new("/w"))
                .is_none()
        );
    }

    #[test]
    fn a_file_just_written_makes_the_project_active_but_artefacts_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        fs::create_dir_all(project.join("node_modules/dep")).unwrap();
        fs::write(project.join("node_modules/dep/index.js"), "x").unwrap();
        let old = SystemTime::now() - std::time::Duration::from_secs(90 * 86_400);
        fs::write(project.join("package.json"), "{}").unwrap();
        fs::File::options()
            .write(true)
            .open(project.join("package.json"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        let handoff = Handoff::default();
        assert_eq!(
            idle_days(project, None, &handoff),
            Some((90, Source::Files))
        );

        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/main.ts"), "x").unwrap();
        assert_eq!(
            idle_days(project, Some(40), &handoff),
            Some((0, Source::Files))
        );
    }
}
