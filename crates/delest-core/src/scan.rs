//! Walks the folders given by the developer, stops at each recognised artefact and
//! measures it. The scan only reads.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::arbiter::Arbiter;
use crate::rules::CACHES;
use crate::verdict::Assessment;

// Delest's own quarantine is never scanned.
use crate::quarantine::FOLDER as QUARANTINE;

/// Every recognised artefact below `roots`, plus the global caches when asked,
/// largest first.
/// A folder of a project that no rule recognises is listed, as unknown, from this size.
pub const UNKNOWN_MIN_BYTES: u64 = 500_000_000;

/// Every recognised artefact below `roots`, plus the large unknown folders of the
/// projects found and, when asked, the global caches; largest first.
pub fn scan(roots: &[PathBuf], arbiter: &Arbiter, caches: bool) -> Vec<Assessment> {
    scan_with(roots, arbiter, caches, UNKNOWN_MIN_BYTES)
}

/// `scan` with another threshold for unknown folders.
pub fn scan_with(
    roots: &[PathBuf],
    arbiter: &Arbiter,
    caches: bool,
    unknown_min: u64,
) -> Vec<Assessment> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut projects: Vec<PathBuf> = Vec::new();
    for root in roots {
        if arbiter.recognises(root) {
            found.push(root.clone());
        } else {
            walk(root, arbiter, &mut found, &mut projects);
        }
    }
    let unknowns = unknowns(&projects, arbiter, &found, unknown_min);
    if caches {
        for cache in CACHES {
            let path = arbiter.home().join(cache.path);
            if path.is_dir() && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    let mut assessments: Vec<Assessment> = found
        .iter()
        .map(|path| {
            let mut assessment = arbiter.assess(path);
            assessment.size = Some(disk_usage(path));
            assessment
        })
        .collect();
    assessments.extend(unknowns);
    assessments.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    assessments
}

/// Whether a folder holds a project: a git repository, a build manifest, a Unity project.
fn is_project_marker(name: &str) -> bool {
    name == ".git"
        || name == "ProjectSettings"
        || crate::rules::RULES
            .iter()
            .flat_map(|rule| rule.manifests)
            .any(|manifest| manifest.file == name)
}

fn walk(root: &Path, arbiter: &Arbiter, found: &mut Vec<PathBuf>, projects: &mut Vec<PathBuf>) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut project = false;
        for entry in entries.flatten() {
            project |= entry.file_name().to_str().is_some_and(is_project_marker);
            // Symbolic links are never followed: they may lead out of the project.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if name == ".git" || name == QUARANTINE {
                continue;
            }
            let path = entry.path();
            if arbiter.recognises(&path) {
                found.push(path);
            } else {
                pending.push(path);
            }
        }
        if project {
            projects.push(dir);
        }
    }
}

/// The direct subfolders of the projects that no rule recognises, are no project
/// themselves and weigh at least `min` bytes once the recognised artefacts
/// inside are left out. They are listed as unknown: never offered.
fn unknowns(
    projects: &[PathBuf],
    arbiter: &Arbiter,
    found: &[PathBuf],
    min: u64,
) -> Vec<Assessment> {
    let mut listed = Vec::new();
    for project in projects {
        let Ok(entries) = fs::read_dir(project) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let path = entry.path();
            let hidden = name.to_string_lossy().starts_with('.');
            if !kind.is_dir()
                || hidden
                || arbiter.recognises(&path)
                || found.contains(&path)
                || projects.contains(&path)
            {
                continue;
            }
            let size = disk_usage_outside(&path, arbiter);
            if size >= min {
                let mut assessment = arbiter.assess(&path);
                assessment.size = Some(size);
                listed.push(assessment);
            }
        }
    }
    listed
}

/// Like `disk_usage`, without the recognised artefacts and git folders inside.
fn disk_usage_outside(path: &Path, arbiter: &Arbiter) -> u64 {
    let mut total = 0;
    let mut seen = HashSet::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                let child = entry.path();
                if entry.file_name() == ".git" || arbiter.recognises(&child) {
                    continue;
                }
                pending.push(child);
            }
            if first_link(&metadata, &mut seen) {
                total += allocated(&metadata);
            }
        }
    }
    total
}

/// Bytes the folder takes on disk, without following links. A file with several
/// hard links, as Cargo makes in `target/`, is counted once.
pub fn disk_usage(path: &Path) -> u64 {
    let mut total = 0;
    let mut seen = HashSet::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            }
            if first_link(&metadata, &mut seen) {
                total += allocated(&metadata);
            }
        }
    }
    total
}

#[cfg(unix)]
fn first_link(metadata: &fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() <= 1 || seen.insert((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn first_link(_: &fs::Metadata, _: &mut HashSet<(u64, u64)>) -> bool {
    true
}

#[cfg(unix)]
fn allocated(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn the_scan_stops_at_artefacts_and_skips_git_and_the_quarantine() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("app/package.json"), 10);
        touch(&root.join("app/node_modules/dep/build/out.js"), 10);
        touch(&root.join("app/.git/build/x"), 10);
        touch(&root.join(QUARANTINE).join("target/x"), 10);
        touch(&root.join("lib/target/big.bin"), 100_000);
        let arbiter = Arbiter::new(PathBuf::from("/nonexistent-home"));
        let paths: Vec<PathBuf> = scan(&[root.to_path_buf()], &arbiter, false)
            .into_iter()
            .map(|assessment| assessment.path)
            .collect();
        assert_eq!(
            paths,
            vec![root.join("lib/target"), root.join("app/node_modules")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hard_linked_file_is_counted_once() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("deps/app-1234"), 100_000);
        let single = disk_usage(dir.path());
        fs::hard_link(dir.path().join("deps/app-1234"), dir.path().join("app")).unwrap();
        assert_eq!(disk_usage(dir.path()), single);
    }

    #[test]
    fn large_unknown_folders_of_a_project_are_listed_but_never_offered() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("game/ProjectSettings/ProjectVersion.txt"), 10);
        touch(&root.join("game/Library/cache.bin"), 60_000);
        touch(&root.join("game/Assets/small.png"), 1_000);
        touch(&root.join("game/Library/build/x"), 10);
        let arbiter = Arbiter::new(PathBuf::from("/nonexistent-home"));
        let found = scan_with(&[root.to_path_buf()], &arbiter, false, 50_000);
        let library = found
            .iter()
            .find(|item| item.path == root.join("game/Library"))
            .unwrap();
        assert_eq!(library.verdict, crate::verdict::Verdict::Unknown);
        assert!(library.size.unwrap() >= 60_000);
        assert!(
            !found
                .iter()
                .any(|item| item.path == root.join("game/Assets"))
        );
    }

    #[test]
    fn disk_usage_counts_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("a/b/c.bin"), 50_000);
        assert!(disk_usage(dir.path()) >= 50_000);
    }
}
