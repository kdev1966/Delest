//! The journal: one JSON line per action, with the trace that justified it.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::verdict::Assessment;

const FILE: &str = "journal.jsonl";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Moved into the quarantine.
    Quarantine,
    /// Put back where it was.
    Restore,
    /// Deleted from the quarantine, after its retention period or over the cap.
    Purge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    /// Local time, RFC 3339.
    pub time: String,
    pub action: Action,
    /// Where the item lives, or lived, in the developer's folders.
    pub path: PathBuf,
    /// The quarantine entry involved.
    pub entry: PathBuf,
    pub assessment: Assessment,
}

impl Record {
    pub fn now(action: Action, path: &Path, entry: &Path, assessment: &Assessment) -> Record {
        Record {
            time: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            action,
            path: path.to_path_buf(),
            entry: entry.to_path_buf(),
            assessment: assessment.clone(),
        }
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

pub fn append(data_dir: &Path, record: &Record) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path(data_dir))?;
    let line = serde_json::to_string(record).map_err(io::Error::other)?;
    writeln!(file, "{line}")
}

/// Every record, oldest first; lines that cannot be read are skipped.
pub fn read(data_dir: &Path) -> io::Result<Vec<Record>> {
    let file = match fs::File::open(path(data_dir)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verdict::{Category, Reason, Verdict};

    #[test]
    fn records_are_appended_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let assessment = Assessment {
            path: "/p/target".into(),
            verdict: Verdict::Rebuildable,
            category: Category::Build,
            rule: Some("target".into()),
            reasons: vec![Reason::Manifest {
                file: "Cargo.toml".into(),
            }],
            size: Some(42),
            idle_days: None,
            shadow: None,
        };
        let entry = Path::new("/q/1");
        append(
            dir.path(),
            &Record::now(Action::Quarantine, &assessment.path, entry, &assessment),
        )
        .unwrap();
        append(
            dir.path(),
            &Record::now(Action::Restore, &assessment.path, entry, &assessment),
        )
        .unwrap();
        let records = read(dir.path()).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].action, Action::Restore);
        assert_eq!(records[0].assessment.reasons, assessment.reasons);
    }
}
