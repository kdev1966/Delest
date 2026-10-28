//! The quarantine. An item is moved, never copied, into `.delest-quarantaine` on
//! its own volume: in the home folder for the volume that holds it, at the root of
//! any other volume. It is kept seven days and can be put back until then.
//!
//! Each entry is a folder holding `entry.json`, written before the move so that an
//! interruption never leaves an item without its record, and `item`, the moved folder.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::tr;
use crate::verdict::Assessment;

pub const FOLDER: &str = ".delest-quarantaine";
pub const RETENTION_DAYS: i64 = 7;
/// The quarantine of a volume may take up to this share of it.
pub const CAP_PERCENT: u64 = 10;
const ITEM: &str = "item";
const ENTRY_FILE: &str = "entry.json";
/// The quarantine folders used so far, kept in the data directory.
const ROOTS_FILE: &str = "quarantaines.json";
const DAY: i64 = 86_400;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub original: PathBuf,
    /// Seconds since the Unix epoch.
    pub moved_at: i64,
    pub assessment: Assessment,
}

#[derive(Clone, Debug)]
pub struct Quarantined {
    /// The entry's folder inside the quarantine.
    pub dir: PathBuf,
    pub entry: Entry,
}

impl Quarantined {
    pub fn id(&self) -> String {
        self.dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn item(&self) -> PathBuf {
        self.dir.join(ITEM)
    }

    pub fn age_days(&self, now: i64) -> i64 {
        (now - self.entry.moved_at) / DAY
    }

    pub fn size(&self) -> u64 {
        self.entry.assessment.size.unwrap_or(0)
    }

    /// The quarantine folder that holds this entry.
    fn root(&self) -> &Path {
        self.dir.parent().unwrap_or(&self.dir)
    }
}

#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// Symbolic links are never moved: they may point outside the project.
    Symlink(PathBuf),
    /// Moving would mean copying to another volume.
    OtherVolume(PathBuf),
    /// Something already stands where the item would be put back.
    AlreadyExists(PathBuf),
    /// The home folder, a volume root, a parent of the home folder or the quarantine itself.
    Forbidden(PathBuf),
}

impl Error {
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Error::Io { path, source } => format!("{}: {source}", path.display()),
            Error::Symlink(path) => {
                let path = path.display();
                tr!(
                    lang,
                    "{path} is a symbolic link: it is never moved",
                    "{path} est un lien symbolique : il n'est jamais déplacé"
                )
            }
            Error::OtherVolume(path) => {
                let path = path.display();
                tr!(
                    lang,
                    "{path} cannot be moved without copying it to another volume",
                    "{path} ne peut être déplacé sans le copier sur un autre volume"
                )
            }
            Error::AlreadyExists(path) => {
                let path = path.display();
                tr!(
                    lang,
                    "{path} exists again: it was not overwritten",
                    "{path} existe de nouveau : il n'a pas été écrasé"
                )
            }
            Error::Forbidden(path) => {
                let path = path.display();
                tr!(
                    lang,
                    "{path} may not be moved into the quarantine",
                    "{path} ne peut pas être mis en quarantaine"
                )
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text(Lang::En))
    }
}

impl std::error::Error for Error {}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The quarantine folder for an item on the same volume.
pub fn root_for(path: &Path, home: &Path) -> io::Result<PathBuf> {
    if same_volume(path, home)? {
        return Ok(home.join(FOLDER));
    }
    Ok(mount_point(path)?.join(FOLDER))
}

#[cfg(unix)]
fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::symlink_metadata(a)?.dev() == fs::metadata(b)?.dev())
}

/// On Windows, a volume is a drive or a share: the path's prefix (`C:`, `\\server\share`).
#[cfg(not(unix))]
fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    let prefix = |path: &Path| match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            Some(prefix.as_os_str().to_string_lossy().to_lowercase())
        }
        _ => None,
    };
    Ok(prefix(a) == prefix(b))
}

/// The topmost folder above `path` on the same volume.
#[cfg(unix)]
fn mount_point(path: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let device = fs::symlink_metadata(path)?.dev();
    let mut top = path.to_path_buf();
    for dir in path.ancestors().skip(1) {
        match fs::metadata(dir) {
            Ok(metadata) if metadata.dev() == device => top = dir.to_path_buf(),
            _ => break,
        }
    }
    Ok(top)
}

#[cfg(not(unix))]
fn mount_point(path: &Path) -> io::Result<PathBuf> {
    Ok(path.ancestors().last().unwrap_or(path).to_path_buf())
}

/// The leading dot hides the quarantine on Unix; Windows needs its hidden attribute.
fn hide(root: &Path) {
    if cfg!(windows) {
        let _ = std::process::Command::new("attrib")
            .arg("+H")
            .arg(root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn is_forbidden(path: &Path, home: &Path) -> bool {
    path.parent().is_none()
        || home.starts_with(path)
        || path
            .components()
            .any(|part| part == Component::Normal(FOLDER.as_ref()))
}

/// Moves the assessed item into its volume's quarantine.
pub fn put(
    assessment: &Assessment,
    home: &Path,
    data_dir: &Path,
    now: i64,
) -> Result<Quarantined, Error> {
    let path = &assessment.path;
    let metadata = fs::symlink_metadata(path).map_err(io_error(path))?;
    if metadata.file_type().is_symlink() {
        return Err(Error::Symlink(path.clone()));
    }
    if is_forbidden(path, home) {
        return Err(Error::Forbidden(path.clone()));
    }
    let root = root_for(path, home).map_err(io_error(path))?;
    // Moving a parent of the quarantine would move the quarantine into itself.
    if root.starts_with(path) {
        return Err(Error::Forbidden(path.clone()));
    }
    fs::create_dir_all(&root).map_err(io_error(&root))?;
    hide(&root);
    let dir = new_entry_dir(&root, now)?;
    let entry = Entry {
        original: path.clone(),
        moved_at: now,
        assessment: assessment.clone(),
    };
    let json = serde_json::to_string_pretty(&entry).map_err(|error| Error::Io {
        path: dir.clone(),
        source: io::Error::other(error),
    })?;
    let entry_file = dir.join(ENTRY_FILE);
    if let Err(source) = fs::write(&entry_file, json) {
        let _ = fs::remove_dir_all(&dir);
        return Err(Error::Io {
            path: entry_file,
            source,
        });
    }
    if let Err(source) = fs::rename(path, dir.join(ITEM)) {
        let _ = fs::remove_dir_all(&dir);
        return Err(match source.kind() {
            io::ErrorKind::CrossesDevices => Error::OtherVolume(path.clone()),
            _ => Error::Io {
                path: path.clone(),
                source,
            },
        });
    }
    remember_root(data_dir, &root).map_err(io_error(data_dir))?;
    Ok(Quarantined { dir, entry })
}

/// A new, empty entry folder named after the local time.
fn new_entry_dir(root: &Path, now: i64) -> Result<PathBuf, Error> {
    let stamp = chrono::DateTime::from_timestamp(now, 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y%m%d-%H%M%S")
                .to_string()
        })
        .unwrap_or_else(|| now.to_string());
    for n in 1.. {
        let dir = root.join(format!("{stamp}-{n}"));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(Error::Io { path: dir, source }),
        }
    }
    unreachable!("the loop only ends by returning")
}

/// Puts the item back where it was, and removes its entry.
pub fn restore(quarantined: &Quarantined) -> Result<(), Error> {
    let original = &quarantined.entry.original;
    if fs::symlink_metadata(original).is_ok() {
        return Err(Error::AlreadyExists(original.clone()));
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    let item = quarantined.item();
    fs::rename(&item, original).map_err(io_error(&item))?;
    let entry_file = quarantined.dir.join(ENTRY_FILE);
    fs::remove_file(&entry_file).map_err(io_error(&entry_file))?;
    fs::remove_dir(&quarantined.dir).map_err(io_error(&quarantined.dir))
}

/// Deletes the entry for good; an entry already gone counts as deleted.
pub fn purge(quarantined: &Quarantined) -> io::Result<()> {
    match fs::remove_dir_all(&quarantined.dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

/// Whether the entry may be deleted before its retention period ends: only what
/// rebuilds locally, without the network and without a doubt.
pub fn may_purge_early(quarantined: &Quarantined) -> bool {
    quarantined.entry.assessment.verdict == crate::verdict::Verdict::Rebuildable
}

/// Whether the developer may still delete the entry early by asking for it: what
/// rebuilds from the network too, which needs the network again to come back. Never
/// what was to review: a doubt keeps its whole retention period.
pub fn may_purge_on_request(quarantined: &Quarantined) -> bool {
    use crate::verdict::Verdict;
    matches!(
        quarantined.entry.assessment.verdict,
        Verdict::Rebuildable | Verdict::RebuildOnline
    )
}

/// Every entry of every known quarantine, newest first.
pub fn list(home: &Path, data_dir: &Path) -> Vec<Quarantined> {
    let mut roots = remembered_roots(data_dir);
    roots.insert(home.join(FOLDER));
    let mut entries: Vec<Quarantined> = roots
        .iter()
        .filter_map(|root| fs::read_dir(root).ok())
        .flat_map(|dirs| dirs.flatten())
        .filter_map(|dir| {
            let text = fs::read_to_string(dir.path().join(ENTRY_FILE)).ok()?;
            let entry = serde_json::from_str(&text).ok()?;
            Some(Quarantined {
                dir: dir.path(),
                entry,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.entry
            .moved_at
            .cmp(&a.entry.moved_at)
            .then_with(|| b.dir.cmp(&a.dir))
    });
    entries
}

/// The entries to delete: older than the retention period, or beyond the cap of
/// their volume, oldest first. The newest entry of a quarantine is never deleted
/// for the cap, only for its age.
pub fn due_for_purge(
    entries: &[Quarantined],
    now: i64,
    volume_size: impl Fn(&Path) -> Option<u64>,
) -> Vec<&Quarantined> {
    let roots: BTreeSet<&Path> = entries.iter().map(Quarantined::root).collect();
    let mut due = Vec::new();
    for root in roots {
        let cap = volume_size(root).map(|size| size / 100 * CAP_PERCENT);
        let mut used = 0;
        let newest_first = entries.iter().filter(|entry| entry.root() == root);
        for (index, entry) in newest_first.enumerate() {
            used += entry.size();
            let expired = entry.age_days(now) >= RETENTION_DAYS;
            let over_cap = index > 0 && cap.is_some_and(|cap| used > cap);
            if expired || over_cap {
                due.push(entry);
            }
        }
    }
    due.sort_by_key(|entry| entry.entry.moved_at);
    due
}

/// The size of the volume that holds `path`.
pub fn volume_size(path: &Path) -> Option<u64> {
    crate::volume::space(path).map(|space| space.total)
}

fn remembered_roots(data_dir: &Path) -> BTreeSet<PathBuf> {
    fs::read_to_string(data_dir.join(ROOTS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn remember_root(data_dir: &Path, root: &Path) -> io::Result<()> {
    let mut roots = remembered_roots(data_dir);
    if roots.insert(root.to_path_buf()) {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string_pretty(&roots).map_err(io::Error::other)?;
        fs::write(data_dir.join(ROOTS_FILE), json)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn on_windows_another_drive_is_another_volume() {
        use std::path::Path;
        assert!(
            super::same_volume(Path::new(r"C:\Users\a\proj"), Path::new(r"c:\Users\a")).unwrap()
        );
        assert!(!super::same_volume(Path::new(r"D:\proj"), Path::new(r"C:\Users\a")).unwrap());
        assert_eq!(
            super::root_for(Path::new(r"D:\proj\target"), Path::new(r"C:\Users\a")).unwrap(),
            Path::new(r"D:\").join(super::FOLDER)
        );
    }

    use super::*;
    use crate::verdict::{Category, Verdict};

    fn assessment(path: &Path, size: u64) -> Assessment {
        Assessment {
            path: path.to_path_buf(),
            verdict: Verdict::Rebuildable,
            category: Category::Build,
            rule: Some("target".into()),
            reasons: Vec::new(),
            size: Some(size),
            idle_days: None,
            shadow: None,
        }
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        fs::create_dir_all(home.join("proj/target/debug")).unwrap();
        fs::write(home.join("proj/target/debug/app"), "binary").unwrap();
        (dir, home, data)
    }

    #[test]
    fn an_item_goes_into_the_quarantine_and_comes_back() {
        let (_dir, home, data) = setup();
        let target = home.join("proj/target");
        let quarantined = put(&assessment(&target, 6), &home, &data, 1_000_000).unwrap();
        assert!(!target.exists());
        assert!(quarantined.item().join("debug/app").exists());
        assert!(quarantined.dir.starts_with(home.join(FOLDER)));

        let listed = list(&home, &data);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].entry.original, target);

        restore(&listed[0]).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("debug/app")).unwrap(),
            "binary"
        );
        assert!(list(&home, &data).is_empty());
    }

    #[test]
    fn restore_never_overwrites() {
        let (_dir, home, data) = setup();
        let target = home.join("proj/target");
        let quarantined = put(&assessment(&target, 6), &home, &data, 1_000_000).unwrap();
        fs::create_dir_all(&target).unwrap();
        assert!(matches!(
            restore(&quarantined),
            Err(Error::AlreadyExists(_))
        ));
        assert!(quarantined.item().exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_the_home_and_the_quarantine_are_refused() {
        let (_dir, home, data) = setup();
        let link = home.join("proj/link");
        std::os::unix::fs::symlink(home.join("proj/target"), &link).unwrap();
        assert!(matches!(
            put(&assessment(&link, 0), &home, &data, 0),
            Err(Error::Symlink(_))
        ));
        assert!(matches!(
            put(&assessment(&home, 0), &home, &data, 0),
            Err(Error::Forbidden(_))
        ));
        let inside = home.join(FOLDER).join("x");
        fs::create_dir_all(&inside).unwrap();
        assert!(matches!(
            put(&assessment(&inside, 0), &home, &data, 0),
            Err(Error::Forbidden(_))
        ));
    }

    #[test]
    fn only_what_rebuilds_locally_may_be_purged_early() {
        let (_dir, home, data) = setup();
        let target = home.join("proj/target");
        let mut quarantined = put(&assessment(&target, 6), &home, &data, 0).unwrap();
        assert!(may_purge_early(&quarantined));
        quarantined.entry.assessment.verdict = Verdict::RebuildOnline;
        assert!(!may_purge_early(&quarantined));
        assert!(may_purge_on_request(&quarantined));
        quarantined.entry.assessment.verdict = Verdict::ToReview;
        assert!(!may_purge_on_request(&quarantined));
    }

    #[test]
    fn an_entry_already_gone_counts_as_purged() {
        let (_dir, home, data) = setup();
        let quarantined = put(&assessment(&home.join("proj/target"), 6), &home, &data, 0).unwrap();
        purge(&quarantined).unwrap();
        assert!(!quarantined.dir.exists());
        purge(&quarantined).unwrap();
    }

    #[test]
    fn purge_by_age_and_by_cap_spares_the_newest() {
        let (_dir, home, data) = setup();
        let mut quarantined = Vec::new();
        for (name, day) in [("a", 0), ("b", 5), ("c", 6)] {
            let path = home.join(name);
            fs::create_dir_all(&path).unwrap();
            quarantined.push(put(&assessment(&path, 40), &home, &data, day * DAY).unwrap());
        }
        let entries = list(&home, &data);
        let ids = |due: Vec<&Quarantined>| {
            due.iter()
                .map(|q| q.entry.original.clone())
                .collect::<Vec<_>>()
        };
        // Day 8: "a" is past its seven days; the cap is large.
        assert_eq!(
            ids(due_for_purge(&entries, 8 * DAY, |_| Some(1_000_000))),
            vec![home.join("a")]
        );
        // Day 6 with a cap of 50 bytes: "c" (newest) stays, "b" and "a" go.
        assert_eq!(
            ids(due_for_purge(&entries, 6 * DAY, |_| Some(500))),
            vec![home.join("a"), home.join("b")]
        );
        purge(&entries[2]).unwrap();
        assert_eq!(list(&home, &data).len(), 2);
    }
}
