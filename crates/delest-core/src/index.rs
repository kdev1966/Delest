//! The last scan, kept so that `status` and the hooks know what can be freed
//! without scanning again. A new scan replaces the items below its folders and
//! keeps the others.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::verdict::{Assessment, Verdict};

const FILE: &str = "index.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Index {
    /// Seconds since the Unix epoch, of the latest scan.
    pub scanned_at: i64,
    pub roots: Vec<PathBuf>,
    pub items: Vec<Assessment>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub rebuildable: u64,
    pub online: u64,
    pub to_review: u64,
}

impl Index {
    /// No index yet is not an error; an unreadable one is.
    pub fn load(data_dir: &Path) -> io::Result<Option<Index>> {
        match fs::read_to_string(data_dir.join(FILE)) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string(self).map_err(io::Error::other)?;
        let temporary = data_dir.join(format!("{FILE}.tmp"));
        fs::write(&temporary, json)?;
        fs::rename(temporary, data_dir.join(FILE))
    }

    /// The index after a scan of `roots` that found `items`.
    pub fn update(
        previous: Option<Index>,
        roots: &[PathBuf],
        items: &[Assessment],
        now: i64,
    ) -> Index {
        let mut index = previous.unwrap_or_default();
        let below = |path: &Path| roots.iter().any(|root| path.starts_with(root));
        index.items.retain(|item| !below(&item.path));
        index.items.extend(items.iter().cloned());
        index.roots.retain(|root| !below(root));
        index.roots.extend(roots.iter().cloned());
        index.scanned_at = now;
        index
    }

    /// What could be freed, counting only the items still on disk.
    pub fn totals(&self) -> Totals {
        let mut totals = Totals::default();
        for item in self.items.iter().filter(|item| item.path.exists()) {
            let size = item.size.unwrap_or(0);
            match item.verdict {
                Verdict::Rebuildable => totals.rebuildable += size,
                Verdict::RebuildOnline => totals.online += size,
                Verdict::ToReview => totals.to_review += size,
                Verdict::Protected | Verdict::Unknown => {}
            }
        }
        totals
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verdict::Category;

    fn item(path: &Path, verdict: Verdict, size: u64) -> Assessment {
        Assessment {
            path: path.to_path_buf(),
            verdict,
            category: Category::Build,
            rule: None,
            reasons: Vec::new(),
            size: Some(size),
            idle_days: None,
            shadow: None,
        }
    }

    #[test]
    fn a_scan_replaces_its_folders_and_keeps_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for path in [a.join("target"), b.join("build"), b.join("node_modules")] {
            fs::create_dir_all(path).unwrap();
        }
        let first = Index::update(
            None,
            &[a.clone(), b.clone()],
            &[
                item(&a.join("target"), Verdict::Rebuildable, 10),
                item(&b.join("build"), Verdict::Rebuildable, 5),
            ],
            1,
        );
        let second = Index::update(
            Some(first),
            std::slice::from_ref(&b),
            &[item(&b.join("node_modules"), Verdict::RebuildOnline, 7)],
            2,
        );
        assert_eq!(second.scanned_at, 2);
        assert_eq!(second.items.len(), 2);
        assert_eq!(
            second.totals(),
            Totals {
                rebuildable: 10,
                online: 7,
                to_review: 0
            }
        );

        fs::remove_dir(a.join("target")).unwrap();
        assert_eq!(second.totals().rebuildable, 0);

        second.save(dir.path()).unwrap();
        assert_eq!(Index::load(dir.path()).unwrap().unwrap().items.len(), 2);
    }
}
