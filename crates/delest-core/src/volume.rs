//! The size and free space of a volume, and the alert thresholds Delest uses.

use std::path::Path;
#[cfg(not(windows))]
use std::process::Command;

/// Below this share of free space, Delest raises an alert.
pub const ALERT_FREE_PERCENT: u64 = 10;
/// Below this free space, Delest raises an alert, whatever the share.
pub const ALERT_FREE_BYTES: u64 = 20_000_000_000;
/// From this much reclaimable space, Delest gives a notice.
pub const NOTICE_RECLAIMABLE_BYTES: u64 = 5_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Space {
    pub total: u64,
    pub available: u64,
}

/// What `df` reports for the volume that holds `path`.
#[cfg(not(windows))]
pub fn space(path: &Path) -> Option<Space> {
    let output = Command::new("df").arg("-Pk").arg(path).output().ok()?;
    parse_df(&String::from_utf8(output.stdout).ok()?)
}

/// What Windows reports for the volume that holds `path` (no `df` there).
#[cfg(windows)]
pub fn space(path: &Path) -> Option<Space> {
    let stats = fs4::statvfs(path).ok()?;
    Some(Space {
        total: stats.total_space(),
        available: stats.available_space(),
    })
}

#[cfg_attr(windows, allow(dead_code))]
/// `df -Pk`: a header, then `filesystem 1024-blocks used available capacity mount`.
fn parse_df(text: &str) -> Option<Space> {
    let fields: Vec<&str> = text.lines().nth(1)?.split_whitespace().collect();
    let kilobytes = |index: usize| {
        fields
            .get(index)?
            .parse::<u64>()
            .ok()
            .map(|value| value * 1024)
    };
    Some(Space {
        total: kilobytes(1)?,
        available: kilobytes(3)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Fine,
    /// Enough space, but at least 5 GB could be freed.
    Notice,
    /// Less than 10 % or less than 20 GB free.
    Alert,
}

pub fn level(space: Space, reclaimable: u64) -> Level {
    let low_share = space.available * 100 < space.total * ALERT_FREE_PERCENT;
    if low_share || space.available < ALERT_FREE_BYTES {
        Level::Alert
    } else if reclaimable >= NOTICE_RECLAIMABLE_BYTES {
        Level::Notice
    } else {
        Level::Fine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_the_output_of_df() {
        let text = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk1s5s1 488245288 14649728 77478132 16% /\n";
        assert_eq!(
            parse_df(text),
            Some(Space {
                total: 488_245_288 * 1024,
                available: 77_478_132 * 1024
            })
        );
        assert_eq!(parse_df("garbage"), None);
    }

    #[test]
    fn thresholds() {
        let gb = 1_000_000_000;
        let space = |available| Space {
            total: 500 * gb,
            available,
        };
        assert_eq!(level(space(45 * gb), 0), Level::Alert);
        assert_eq!(
            level(
                Space {
                    total: 100 * gb,
                    available: 15 * gb
                },
                0
            ),
            Level::Alert
        );
        assert_eq!(level(space(60 * gb), 6 * gb), Level::Notice);
        assert_eq!(level(space(60 * gb), 4 * gb), Level::Fine);
    }
}
