//! What the developer decided, and what Delest learns from it. A protected path is
//! an absolute ban. Habits only ever make Delest more cautious: an item refused
//! three times is no longer offered, and a restore makes the item, then its rule,
//! wait for a review. Restores are forgotten after 90 days.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "memoire.json";
pub const REFUSALS_TO_STOP: u32 = 3;
pub const RESTORES_REMEMBERED_DAYS: i64 = 90;
/// Restores of one rule after which all its items wait for a review.
pub const RESTORES_TO_DOUBT_A_RULE: usize = 2;
const DAY: i64 = 86_400;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Memory {
    /// Paths the developer protected, with everything below them.
    #[serde(default)]
    pub protected: BTreeSet<PathBuf>,
    /// How many times each item was refused when offered for review.
    #[serde(default)]
    pub refusals: BTreeMap<PathBuf, u32>,
    #[serde(default)]
    pub restores: Vec<Restore>,
    /// Rules taken out of the shadow by the developer.
    #[serde(default)]
    pub accepted_rules: BTreeSet<String>,
    /// The developer's agent is never asked, even on a click.
    #[serde(default)]
    pub agent_never: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Restore {
    pub path: PathBuf,
    pub rule: Option<String>,
    /// Seconds since the Unix epoch.
    pub at: i64,
}

impl Memory {
    /// An unreadable memory is an error: silently starting empty would drop the
    /// developer's protections.
    pub fn load(data_dir: &Path) -> io::Result<Memory> {
        match fs::read_to_string(data_dir.join(FILE)) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Memory::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temporary = data_dir.join(format!("{FILE}.tmp"));
        fs::write(&temporary, json)?;
        fs::rename(temporary, data_dir.join(FILE))
    }

    /// The protected path that covers `path`, if any.
    pub fn protection(&self, path: &Path) -> Option<&Path> {
        self.protected
            .iter()
            .find(|protected| path.starts_with(protected))
            .map(PathBuf::as_path)
    }

    pub fn refusals_of(&self, path: &Path) -> u32 {
        self.refusals.get(path).copied().unwrap_or(0)
    }

    pub fn refuse(&mut self, path: &Path) {
        *self.refusals.entry(path.to_path_buf()).or_default() += 1;
    }

    pub fn record_restore(&mut self, path: &Path, rule: Option<&str>, now: i64) {
        self.restores.push(Restore {
            path: path.to_path_buf(),
            rule: rule.map(str::to_string),
            at: now,
        });
        self.restores
            .retain(|restore| now - restore.at < RESTORES_REMEMBERED_DAYS * DAY);
    }

    fn recent(&self, now: i64) -> impl Iterator<Item = &Restore> {
        self.restores
            .iter()
            .filter(move |restore| now - restore.at < RESTORES_REMEMBERED_DAYS * DAY)
    }

    /// Days since this item was last put back, within the remembered period.
    pub fn restored_days_ago(&self, path: &Path, now: i64) -> Option<i64> {
        self.recent(now)
            .filter(|restore| restore.path == path)
            .map(|restore| (now - restore.at) / DAY)
            .min()
    }

    /// Recent restores of items this rule had offered.
    pub fn restores_of_rule(&self, rule: &str, now: i64) -> usize {
        self.recent(now)
            .filter(|restore| restore.rule.as_deref() == Some(rule))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protections_cover_everything_below() {
        let mut memory = Memory::default();
        memory.protected.insert("/work/client".into());
        assert_eq!(
            memory.protection(Path::new("/work/client/app/dist")),
            Some(Path::new("/work/client"))
        );
        assert!(memory.protection(Path::new("/work/clients")).is_none());
    }

    #[test]
    fn restores_are_forgotten_after_ninety_days() {
        let mut memory = Memory::default();
        let path = Path::new("/p/target");
        memory.record_restore(path, Some("target"), 0);
        assert_eq!(memory.restored_days_ago(path, 10 * DAY), Some(10));
        assert_eq!(memory.restores_of_rule("target", 10 * DAY), 1);
        assert_eq!(memory.restored_days_ago(path, 91 * DAY), None);
        memory.record_restore(Path::new("/q/target"), Some("target"), 91 * DAY);
        assert_eq!(memory.restores.len(), 1);
    }

    #[test]
    fn the_memory_is_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut memory = Memory::default();
        memory.refuse(Path::new("/p/.venv"));
        memory.refuse(Path::new("/p/.venv"));
        memory.save(dir.path()).unwrap();
        assert_eq!(
            Memory::load(dir.path())
                .unwrap()
                .refusals_of(Path::new("/p/.venv")),
            2
        );
        fs::write(dir.path().join(FILE), "{broken").unwrap();
        assert!(Memory::load(dir.path()).is_err());
    }
}
