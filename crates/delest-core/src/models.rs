//! The shared model store, `~/.local/share/kdev-models`: one copy of each model for
//! Delest and the developer's other projects, with a manifest that records each
//! file's fingerprint, size and licence. A model already on the disk is reused
//! (cloned on APFS, so it takes no extra space) before anything is downloaded.

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::processes::OpenFiles;

/// Overrides the store's folder.
pub const ENV_STORE: &str = "KDEV_MODELS";
const MANIFEST: &str = "manifest.json";

/// A model Delest knows how to get.
#[derive(Debug)]
pub struct Known {
    pub name: &'static str,
    pub role: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub license: &'static str,
    /// The fingerprint and size Hugging Face publishes for the file: a copy found on
    /// the disk or a download is kept only when it matches.
    pub sha256: &'static str,
    pub size: u64,
}

pub const CATALOG: [Known; 3] = [
    Known {
        name: "embeddinggemma-2",
        role: "recognition",
        file: "embeddinggemma-2-Q8_0.gguf",
        url: "https://huggingface.co/unsloth/embeddinggemma-2-GGUF/resolve/main/embeddinggemma-2-Q8_0.gguf",
        license: "Apache-2.0",
        sha256: "6f1bd4ac6c5df7444f9cca7ca36cafe6cfa34cd6f49fefb1e0b4be8143aed8bc",
        size: 309_855_520,
    },
    Known {
        name: "gemma-4-e2b",
        role: "reasoning",
        file: "gemma-4-E2B-it-Q4_0.gguf",
        url: "https://huggingface.co/ggml-org/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_0.gguf",
        license: "Apache-2.0",
        sha256: "8e30dff3ac4c8434c49a7036fa15564bdbb6044e42bf04550bf1a096ad7e6a52",
        size: 2_841_481_184,
    },
    Known {
        name: "qwen3.5-2b",
        role: "fallback",
        file: "Qwen3.5-2B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf",
        license: "Apache-2.0",
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        size: 1_280_835_840,
    },
];

pub fn known(name: &str) -> Option<&'static Known> {
    CATALOG.iter().find(|known| known.name == name)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Installed {
    pub file: String,
    pub sha256: String,
    pub size: u64,
    pub license: String,
    /// Where it came from: a URL, or the local file it was cloned from.
    pub source: String,
}

/// The manifest, by model name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub models: BTreeMap<String, Installed>,
}

/// `$KDEV_MODELS`, else `$XDG_DATA_HOME/kdev-models`, else `~/.local/share/kdev-models`.
pub fn store_dir(home: &Path) -> PathBuf {
    let set = |name: &str| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    set(ENV_STORE).unwrap_or_else(|| {
        set("XDG_DATA_HOME")
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("kdev-models")
    })
}

impl Manifest {
    pub fn load(store: &Path) -> io::Result<Manifest> {
        match fs::read_to_string(store.join(MANIFEST)) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Manifest::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, store: &Path) -> io::Result<()> {
        fs::create_dir_all(store)?;
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temporary = store.join(format!("{MANIFEST}.tmp"));
        fs::write(&temporary, json)?;
        fs::rename(temporary, store.join(MANIFEST))
    }

    /// The model's file, when installed and still there.
    pub fn path(&self, store: &Path, name: &str) -> Option<PathBuf> {
        let installed = self.models.get(name)?;
        Some(store.join(&installed.file)).filter(|path| path.is_file())
    }
}

pub fn sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// The folders where other tools keep models, searched before downloading.
pub fn known_caches(home: &Path) -> Vec<PathBuf> {
    [
        ".cache/huggingface/hub",
        "Library/Caches/llama.cpp",
        ".cache/llama.cpp",
        ".lmstudio/models",
        ".cache/lm-studio/models",
        ".ollama/models",
    ]
    .iter()
    .map(|path| home.join(path))
    .filter(|path| path.is_dir())
    .collect()
}

/// A file named like the model below `roots`, symbolic links not followed.
pub fn find_copy(known: &Known, roots: &[PathBuf]) -> Option<PathBuf> {
    let mut pending: Vec<PathBuf> = roots.to_vec();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && entry.file_name() != ".git" {
                pending.push(entry.path());
            } else if kind.is_file() && entry.file_name() == known.file {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Copies `source` to `target` as an APFS clone when the system can (no extra
/// space), else as a plain copy.
pub fn clone_file(source: &Path, target: &Path) -> io::Result<()> {
    if cfg!(target_os = "macos") {
        let cloned = Command::new("cp")
            .arg("-c")
            .arg(source)
            .arg(target)
            .stderr(Stdio::null())
            .status()?
            .success();
        if cloned {
            return Ok(());
        }
    }
    fs::copy(source, target).map(drop)
}

/// A file is kept only when it is the published one; otherwise it is removed.
pub fn check(path: &Path, expected: &str) -> io::Result<()> {
    let found = sha256(path)?;
    if found == expected {
        return Ok(());
    }
    let _ = fs::remove_file(path);
    Err(io::Error::other(format!(
        "{} is not the published file (sha256 {}…, expected {}…)",
        path.display(),
        &found[..12],
        &expected[..12]
    )))
}

/// Where a model is written while it is copied or downloaded.
pub fn part_path(store: &Path, known: &Known) -> PathBuf {
    store.join(format!("{}.part", known.file))
}

/// Puts `file` into the store under the model's name, fingerprinted; the file is
/// cloned, the original stays where it is. A copy that is not the published file is
/// refused.
pub fn add_local(store: &Path, known: &Known, file: &Path) -> io::Result<Installed> {
    fs::create_dir_all(store)?;
    let target = store.join(known.file);
    if !target.exists() {
        let temporary = part_path(store, known);
        clone_file(file, &temporary)?;
        check(&temporary, known.sha256)?;
        fs::rename(&temporary, &target)?;
    }
    record(store, known, &target, file.display().to_string())
}

/// Downloads `url` into `part` with `curl`, picking up an interrupted download;
/// curl's progress bar is shown on the command line, the app draws its own.
pub fn fetch(url: &str, part: &Path, progress: bool) -> io::Result<()> {
    if let Some(parent) = part.parent() {
        fs::create_dir_all(parent)?;
    }
    let fetched = Command::new("curl")
        .args(["--fail", "--location", "--show-error"])
        .arg(if progress {
            "--progress-bar"
        } else {
            "--silent"
        })
        .args(["--continue-at", "-", "--output"])
        .arg(part)
        .arg(url)
        .status()?
        .success();
    if fetched {
        Ok(())
    } else {
        Err(io::Error::other(format!("the download of {url} failed")))
    }
}

/// Downloads the model into the store, then checks its fingerprint.
pub fn download(store: &Path, known: &Known, progress: bool) -> io::Result<Installed> {
    let temporary = part_path(store, known);
    fetch(known.url, &temporary, progress)?;
    check(&temporary, known.sha256)?;
    let target = store.join(known.file);
    fs::rename(&temporary, &target)?;
    record(store, known, &target, known.url.to_string())
}

fn record(store: &Path, known: &Known, path: &Path, source: String) -> io::Result<Installed> {
    let installed = Installed {
        file: known.file.to_string(),
        sha256: sha256(path)?,
        size: fs::metadata(path)?.len(),
        license: known.license.to_string(),
        source,
    };
    let mut manifest = Manifest::load(store)?;
    manifest
        .models
        .insert(known.name.to_string(), installed.clone());
    manifest.save(store)?;
    Ok(installed)
}

/// Identical model files: same size, same fingerprint, different files on the disk.
#[derive(Clone, Debug)]
pub struct Duplicate {
    pub sha256: String,
    pub size: u64,
    pub paths: Vec<PathBuf>,
}

impl Duplicate {
    /// The copy the others become clones of: the store's own, else the oldest.
    pub fn reference(&self, store: &Path) -> &Path {
        self.paths
            .iter()
            .find(|path| path.starts_with(store))
            .or_else(|| {
                self.paths.iter().min_by_key(|path| {
                    fs::metadata(path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                })
            })
            .unwrap_or(&self.paths[0])
    }
}

/// Why a copy was left as it is.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kept {
    /// Already the same file as the reference.
    SameFile,
    /// On another volume: a clone cannot cross volumes.
    OtherVolume,
    /// It has other names (hard links), which a clone would separate from it.
    OtherNames,
    /// Open in a program.
    InUse(String),
    /// Changed since it was measured.
    Changed,
    /// The volume cannot clone.
    NoClone,
}

/// Groups of identical model files below `roots`: same size, then same fingerprint.
/// A file seen under several names (hard links) counts once.
pub fn duplicates(roots: &[PathBuf]) -> io::Result<Vec<Duplicate>> {
    const EXTENSIONS: [&str; 3] = ["gguf", "safetensors", "onnx"];
    let mut by_size: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    // A folder given twice, or inside another one given, is walked once.
    let mut pending: Vec<PathBuf> = roots
        .iter()
        .filter(|root| {
            !roots
                .iter()
                .any(|other| other != *root && root.starts_with(other))
        })
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() && entry.file_name() != ".git" {
                pending.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| EXTENSIONS.contains(&ext))
                && let Ok(metadata) = entry.metadata()
                && seen.insert(file_id(&metadata, &path))
            {
                by_size.entry(metadata.len()).or_default().push(path);
            }
        }
    }
    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_, paths)| paths.len() > 1) {
        let mut by_hash: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for path in paths {
            by_hash.entry(sha256(&path)?).or_default().push(path);
        }
        groups.extend(
            by_hash
                .into_iter()
                .filter(|(_, paths)| paths.len() > 1)
                .map(|(sha256, paths)| Duplicate {
                    sha256,
                    size,
                    paths,
                }),
        );
    }
    Ok(groups)
}

#[cfg(unix)]
fn file_id(metadata: &fs::Metadata, _: &Path) -> (u64, u64, PathBuf) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino(), PathBuf::new())
}

#[cfg(not(unix))]
fn file_id(_: &fs::Metadata, path: &Path) -> (u64, u64, PathBuf) {
    (0, 0, path.to_path_buf())
}

/// Replaces every other copy of the group by an APFS clone of its reference: the
/// files then share their blocks. The fingerprints are read again first; a copy
/// that changed, is open, or cannot be cloned is left as it is.
pub fn replace_by_clones(
    duplicate: &Duplicate,
    store: &Path,
    open: &OpenFiles,
) -> io::Result<Vec<(PathBuf, Option<Kept>)>> {
    let reference = duplicate.reference(store);
    let copies = duplicate.paths.iter().filter(|path| *path != reference);
    if sha256(reference)? != duplicate.sha256 {
        return Ok(copies
            .map(|copy| (copy.clone(), Some(Kept::Changed)))
            .collect());
    }
    copies
        .map(|copy| Ok((copy.clone(), clone_over(reference, copy, duplicate, open)?)))
        .collect()
}

/// `None` once `copy` is a clone of `reference`, with the copy's permissions and dates.
#[cfg(unix)]
fn clone_over(
    reference: &Path,
    copy: &Path,
    duplicate: &Duplicate,
    open: &OpenFiles,
) -> io::Result<Option<Kept>> {
    use std::os::unix::fs::MetadataExt;
    let source = fs::metadata(reference)?;
    let target = fs::symlink_metadata(copy)?;
    if !target.is_file() || target.len() != duplicate.size {
        return Ok(Some(Kept::Changed));
    }
    if (source.dev(), source.ino()) == (target.dev(), target.ino()) {
        return Ok(Some(Kept::SameFile));
    }
    if source.dev() != target.dev() {
        return Ok(Some(Kept::OtherVolume));
    }
    if target.nlink() > 1 {
        return Ok(Some(Kept::OtherNames));
    }
    if let Some(program) = open.user_of(copy) {
        return Ok(Some(Kept::InUse(program.to_string())));
    }
    if !cfg!(target_os = "macos") {
        return Ok(Some(Kept::NoClone));
    }
    if sha256(copy)? != duplicate.sha256 {
        return Ok(Some(Kept::Changed));
    }
    let name = copy.file_name().unwrap_or_default().to_string_lossy();
    let temporary = copy.with_file_name(format!(".{name}.delest-clone"));
    let _ = fs::remove_file(&temporary);
    // `cp -c` clones or fails: it never falls back to a full copy.
    let cloned = Command::new("cp")
        .arg("-c")
        .arg(reference)
        .arg(&temporary)
        .stderr(Stdio::null())
        .status()?
        .success();
    if !cloned {
        let _ = fs::remove_file(&temporary);
        return Ok(Some(Kept::NoClone));
    }
    let finish = || -> io::Result<()> {
        let times = fs::FileTimes::new()
            .set_accessed(target.accessed()?)
            .set_modified(target.modified()?);
        File::open(&temporary)?.set_times(times)?;
        fs::set_permissions(&temporary, target.permissions())?;
        fs::rename(&temporary, copy)
    };
    if let Err(error) = finish() {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(None)
}

#[cfg(not(unix))]
fn clone_over(_: &Path, _: &Path, _: &Duplicate, _: &OpenFiles) -> io::Result<Option<Kept>> {
    Ok(Some(Kept::NoClone))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_copy_is_cloned_into_the_store_and_fingerprinted() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("project/models");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("tiny.gguf"), b"weights").unwrap();
        // A catalog entry whose published fingerprint is this file's.
        let sha = sha256(&elsewhere.join("tiny.gguf")).unwrap();
        let known: &'static Known = Box::leak(Box::new(Known {
            name: "embeddinggemma-2",
            role: "recognition",
            file: "tiny.gguf",
            url: "https://example.invalid/tiny.gguf",
            license: "Apache-2.0",
            sha256: Box::leak(sha.into_boxed_str()),
            size: 7,
        }));
        let found = find_copy(known, &[dir.path().join("project")]).unwrap();
        let store = dir.path().join("store");
        let installed = add_local(&store, known, &found).unwrap();
        assert_eq!(installed.size, 7);
        assert_eq!(installed.sha256, sha256(&found).unwrap());
        let manifest = Manifest::load(&store).unwrap();
        assert_eq!(
            manifest.path(&store, "embeddinggemma-2"),
            Some(store.join(known.file))
        );
        assert!(found.exists(), "the original stays where it is");
    }

    #[test]
    fn identical_model_files_are_grouped() {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in [
            ("a/x.gguf", "same"),
            ("b/y.gguf", "same"),
            ("c/z.gguf", "diff"),
            ("d/w.txt", "same"),
        ] {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        let groups = duplicates(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(groups[0].size, 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_hard_link_is_not_a_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.gguf"), b"same").unwrap();
        fs::hard_link(dir.path().join("a.gguf"), dir.path().join("b.gguf")).unwrap();
        assert!(duplicates(&[dir.path().to_path_buf()]).unwrap().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copies_become_clones_with_their_own_mode_and_date() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        let project = dir.path().join("project");
        for folder in [&store, &project] {
            fs::create_dir_all(folder).unwrap();
        }
        fs::write(store.join("m.gguf"), b"weights").unwrap();
        let copy = project.join("m.gguf");
        fs::write(&copy, b"weights").unwrap();
        fs::set_permissions(&copy, fs::Permissions::from_mode(0o444)).unwrap();
        let date =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        File::open(&copy)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(date))
            .unwrap();
        let groups = duplicates(&[dir.path().to_path_buf()]).unwrap();
        assert_eq!(groups[0].reference(&store), store.join("m.gguf"));
        let outcome = replace_by_clones(&groups[0], &store, &OpenFiles::parse("")).unwrap();
        assert_eq!(outcome, vec![(copy.clone(), None)]);
        assert_eq!(fs::read(&copy).unwrap(), b"weights");
        let metadata = fs::metadata(&copy).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o444);
        assert_eq!(metadata.modified().unwrap(), date);
        assert!(store.join("m.gguf").exists(), "the reference stays");
        assert_eq!(
            fs::read_dir(&project).unwrap().count(),
            1,
            "no clone left behind"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_copy_that_changed_or_is_open_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = ["a.gguf", "b.gguf", "c.gguf"]
            .iter()
            .map(|name| dir.path().join(name))
            .collect();
        for path in &paths {
            fs::write(path, b"same").unwrap();
        }
        let groups = duplicates(&[dir.path().to_path_buf()]).unwrap();
        let reference = groups[0].reference(dir.path()).to_path_buf();
        let others: Vec<&PathBuf> = paths.iter().filter(|path| **path != reference).collect();
        fs::write(others[0], b"diff").unwrap();
        let open = OpenFiles::parse(&format!("p1\ncllama\nn{}\n", others[1].display()));
        let mut outcome = replace_by_clones(&groups[0], dir.path(), &open).unwrap();
        outcome.sort();
        let mut expected = vec![
            (others[0].clone(), Some(Kept::Changed)),
            (others[1].clone(), Some(Kept::InUse("llama".into()))),
        ];
        expected.sort();
        assert_eq!(outcome, expected);
        assert_eq!(fs::read(others[0]).unwrap(), b"diff");
    }

    #[test]
    fn the_fingerprint_is_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
