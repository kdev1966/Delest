//! The history of free space: one reading per day at most, kept 90 days, for the
//! interface's chart. Every command that measures the disk adds to it.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::volume::Space;

const FILE: &str = "historique.json";
const KEPT_DAYS: i64 = 90;
const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    /// Seconds since the Unix epoch.
    pub at: i64,
    pub available: u64,
    pub total: u64,
}

pub fn load(data_dir: &Path) -> Vec<Reading> {
    fs::read_to_string(data_dir.join(FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Adds today's reading, or replaces it with this newer one, and forgets the oldest.
pub fn record(data_dir: &Path, space: Space, now: i64) -> io::Result<()> {
    let mut readings = load(data_dir);
    readings.retain(|reading| now - reading.at < KEPT_DAYS * DAY && reading.at / DAY != now / DAY);
    readings.push(Reading {
        at: now,
        available: space.available,
        total: space.total,
    });
    readings.sort_by_key(|reading| reading.at);
    fs::create_dir_all(data_dir)?;
    let json = serde_json::to_string(&readings).map_err(io::Error::other)?;
    fs::write(data_dir.join(FILE), json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_reading_a_day_kept_ninety_days() {
        let dir = tempfile::tempdir().unwrap();
        let space = |available| Space {
            total: 100,
            available,
        };
        record(dir.path(), space(50), 10).unwrap();
        record(dir.path(), space(40), 20).unwrap();
        assert_eq!(
            load(dir.path()),
            vec![Reading {
                at: 20,
                available: 40,
                total: 100
            }]
        );
        record(dir.path(), space(30), DAY + 5).unwrap();
        assert_eq!(load(dir.path()).len(), 2);
        record(dir.path(), space(20), 92 * DAY).unwrap();
        let kept = load(dir.path());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].available, 20);
    }
}
