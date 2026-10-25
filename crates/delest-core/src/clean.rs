//! Cleaning: choose what to move into the quarantine, check each item again right
//! before moving it, journal every action, then purge what the quarantine no
//! longer keeps.

use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::activity::INACTIVE_AFTER_DAYS;
use crate::arbiter::Arbiter;
use crate::journal::{self, Action, Record};
use crate::processes::OpenFiles;
use crate::quarantine::{self, Quarantined};
use crate::verdict::{Assessment, Verdict};

/// A folder modified this recently may be in use by a build: it is left alone.
pub const RECENT: Duration = Duration::from_secs(10 * 60);
/// Reached to tell whether the network is there; nothing is sent.
const PROBE: (&str, u16) = ("registry.npmjs.org", 443);

/// Whether what needs the network to be rebuilt can be offered.
pub fn online() -> bool {
    let Some(address) = PROBE
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
    else {
        return false;
    };
    TcpStream::connect_timeout(&address, Duration::from_secs(3)).is_ok()
}

pub struct Plan {
    /// Offered for the quarantine.
    pub selected: Vec<Assessment>,
    /// Offered only one by one, with the developer's explicit consent.
    pub to_review: Vec<Assessment>,
    /// Held back: they need the network to be rebuilt, and it is not there.
    pub held_offline: Vec<Assessment>,
    /// Dependencies of a project worked on lately: they would be reinstalled at once.
    pub kept_active: Vec<Assessment>,
}

pub fn plan(assessments: Vec<Assessment>, online: bool) -> Plan {
    let mut plan = Plan {
        selected: Vec::new(),
        to_review: Vec::new(),
        held_offline: Vec::new(),
        kept_active: Vec::new(),
    };
    for assessment in assessments {
        let active = assessment
            .idle_days
            .is_some_and(|days| days < INACTIVE_AFTER_DAYS);
        if active && assessment.verdict.is_freeable() {
            plan.kept_active.push(assessment);
            continue;
        }
        match assessment.verdict {
            Verdict::Rebuildable => plan.selected.push(assessment),
            Verdict::RebuildOnline if online => plan.selected.push(assessment),
            Verdict::RebuildOnline => plan.held_offline.push(assessment),
            Verdict::ToReview => plan.to_review.push(assessment),
            Verdict::Protected | Verdict::Unknown => {}
        }
    }
    plan
}

#[derive(Debug)]
pub enum Skip {
    /// The verdict is no longer the one shown in the preview.
    Changed {
        now: Verdict,
    },
    RecentlyModified,
    /// A program has a file open inside.
    InUse {
        program: String,
    },
    Failed(quarantine::Error),
}

pub struct Outcome {
    pub moved: Vec<Quarantined>,
    pub skipped: Vec<(Assessment, Skip)>,
    pub purged: Vec<Quarantined>,
    /// Actions done but not journaled.
    pub journal_errors: usize,
    /// `lsof` could not tell which folders are in use.
    pub use_unchecked: bool,
}

/// Moves the previewed items into the quarantine, each one checked again first,
/// then purges the entries past their retention period or over the cap.
pub fn execute(items: &[Assessment], arbiter: &Arbiter, data_dir: &Path, now: i64) -> Outcome {
    let mut outcome = Outcome {
        moved: Vec::new(),
        skipped: Vec::new(),
        purged: Vec::new(),
        journal_errors: 0,
        use_unchecked: false,
    };
    let open = OpenFiles::snapshot();
    outcome.use_unchecked = open.is_none();
    for previewed in items {
        let mut current = arbiter.assess(&previewed.path);
        if current.verdict != previewed.verdict {
            let now = current.verdict;
            outcome
                .skipped
                .push((previewed.clone(), Skip::Changed { now }));
            continue;
        }
        if modified_within(&previewed.path, RECENT) {
            outcome
                .skipped
                .push((previewed.clone(), Skip::RecentlyModified));
            continue;
        }
        if let Some(program) = open.as_ref().and_then(|open| open.user_of(&previewed.path)) {
            let program = program.to_string();
            outcome
                .skipped
                .push((previewed.clone(), Skip::InUse { program }));
            continue;
        }
        current.size = previewed.size;
        match quarantine::put(&current, arbiter.home(), data_dir, now) {
            Ok(quarantined) => {
                let record = Record::now(
                    Action::Quarantine,
                    &current.path,
                    &quarantined.dir,
                    &current,
                );
                outcome.journal_errors += usize::from(journal::append(data_dir, &record).is_err());
                outcome.moved.push(quarantined);
            }
            Err(error) => outcome
                .skipped
                .push((previewed.clone(), Skip::Failed(error))),
        }
    }
    let entries = quarantine::list(arbiter.home(), data_dir);
    for entry in quarantine::due_for_purge(&entries, now, quarantine::volume_size) {
        if quarantine::purge(entry).is_ok() {
            let record = Record::now(
                Action::Purge,
                &entry.entry.original,
                &entry.dir,
                &entry.entry.assessment,
            );
            outcome.journal_errors += usize::from(journal::append(data_dir, &record).is_err());
            outcome.purged.push(entry.clone());
        }
    }
    outcome
}

/// Whether the folder or one of its direct children changed lately: a build writes
/// there first.
fn modified_within(path: &Path, window: Duration) -> bool {
    let recent = |path: &Path| {
        fs::symlink_metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| SystemTime::now().duration_since(time).ok())
            .is_some_and(|age| age < window)
    };
    recent(path)
        || fs::read_dir(path)
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .any(|entry| recent(&entry.path()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::verdict::Category;

    fn assessment(path: &str, verdict: Verdict) -> Assessment {
        Assessment {
            path: PathBuf::from(path),
            verdict,
            category: Category::Build,
            rule: None,
            reasons: Vec::new(),
            size: Some(1),
            idle_days: None,
            shadow: None,
        }
    }

    #[test]
    fn the_plan_holds_back_what_needs_the_network_when_offline() {
        let all = || {
            vec![
                assessment("/a", Verdict::Rebuildable),
                assessment("/b", Verdict::RebuildOnline),
                assessment("/c", Verdict::ToReview),
                assessment("/d", Verdict::Protected),
                assessment("/e", Verdict::Unknown),
            ]
        };
        let mut active = assessment("/f", Verdict::RebuildOnline);
        active.idle_days = Some(3);
        let mut idle = assessment("/g", Verdict::RebuildOnline);
        idle.idle_days = Some(45);
        let checked = plan(vec![active, idle], true);
        assert_eq!(checked.kept_active.len(), 1);
        assert_eq!(checked.selected[0].path, PathBuf::from("/g"));

        let offline = plan(all(), false);
        assert_eq!(offline.selected.len(), 1);
        assert_eq!(offline.held_offline.len(), 1);
        assert_eq!(offline.to_review.len(), 1);
        assert_eq!(plan(all(), true).selected.len(), 2);
    }

    #[test]
    fn a_folder_that_changed_since_the_preview_or_lately_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        let project = home.join("proj");
        fs::create_dir_all(project.join("__pycache__")).unwrap();
        fs::write(project.join("__pycache__/a.pyc"), "x").unwrap();
        let arbiter = Arbiter::new(home.clone());
        let cache = project.join("__pycache__");

        // Shown as rebuildable, but just written to: skipped as recent.
        let previewed = arbiter.assess(&cache);
        let outcome = execute(std::slice::from_ref(&previewed), &arbiter, &data, 0);
        assert!(matches!(outcome.skipped[0].1, Skip::RecentlyModified));
        assert!(cache.exists());

        // Shown with another verdict than the current one: skipped as changed.
        let mut stale = previewed.clone();
        stale.verdict = Verdict::RebuildOnline;
        let outcome = execute(&[stale], &arbiter, &data, 0);
        assert!(matches!(
            outcome.skipped[0].1,
            Skip::Changed {
                now: Verdict::Rebuildable
            }
        ));
    }

    #[test]
    fn a_checked_item_is_quarantined_and_journaled() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        let cache = home.join("proj/__pycache__");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("a.pyc"), "x").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        for path in [cache.join("a.pyc"), cache.clone()] {
            crate::testutil::set_modified(&path, old);
        }
        let arbiter = Arbiter::new(home.clone());
        let previewed = arbiter.assess(&cache);
        let outcome = execute(&[previewed], &arbiter, &data, 0);
        assert_eq!(outcome.moved.len(), 1, "{:?}", outcome.skipped);
        assert!(!cache.exists());
        let records = journal::read(&data).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].action, Action::Quarantine);
    }
}
