//! Installing the local AI on the developer's click: the llama.cpp engine built for
//! this system, then the two models. Nothing is fetched otherwise. Each file is kept
//! only when its fingerprint is the published one; a copy of a model already on the
//! disk is cloned rather than downloaded again.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::engine;
use crate::lang::Lang;
use crate::models::{self, Known, Manifest};
use crate::tr;

/// The llama.cpp release Delest was measured with.
pub const ENGINE_RELEASE: &str = "b11756";
/// The models the local AI needs: recognition, then reasoning.
pub const MODELS: [&str; 2] = ["embeddinggemma-2", "gemma-4-e2b"];

/// One build of llama.cpp, as its release publishes it.
#[derive(Clone, Copy, Debug)]
pub struct Build {
    pub asset: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

const MACOS_ARM64: Build = Build {
    asset: "llama-b11756-bin-macos-arm64.tar.gz",
    sha256: "f86b5eb838b985a17587e794dee9c6b34193aca6dceec018031a4ba7951fb7e6",
    size: 16_103_382,
};
const MACOS_X64: Build = Build {
    asset: "llama-b11756-bin-macos-x64.tar.gz",
    sha256: "6d71968268a01ae975acde18441a70b2c7ba128b7a127f46b5ced4e8c5dd4eca",
    size: 11_809_449,
};
const LINUX_X64: Build = Build {
    asset: "llama-b11756-bin-ubuntu-x64.tar.gz",
    sha256: "9def57d9e9b0149979a9ceba40896a4e2bf88c94734069a5567d2f487ceab551",
    size: 22_240_705,
};
const WINDOWS_X64: Build = Build {
    asset: "llama-b11756-bin-win-cpu-x64.zip",
    sha256: "d164195b8f3e92f8fd92bc3a5c0eeaef8707b2f1a979e36360ce8192ebf4fbb3",
    size: 23_959_301,
};

/// llama.cpp's macOS builds need this version or later (their `minos`).
const MACOS_MINIMUM: (u32, u32) = (13, 3);
/// Its Windows build is made with a recent Visual Studio: with an older Visual C++
/// runtime (`msvcp140.dll`), it crashes at start (seen on Windows 11 with 14.00).
const VC_RUNTIME_MINIMUM: (u32, u32) = (14, 40);
const VC_REDIST: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

/// Why no engine can be fetched here; the models can still be installed.
#[derive(Clone, Debug, PartialEq)]
pub enum Unavailable {
    NoBuild,
    /// The macOS version found, older than the builds need.
    MacosTooOld(String),
    /// The Visual C++ runtime found on Windows (or none), older than the build needs.
    VcRuntimeTooOld(Option<String>),
}

impl Unavailable {
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Unavailable::NoBuild => {
                let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
                tr!(
                    lang,
                    "llama.cpp publishes no build for {os} on {arch}: build llama-server, then delest models engine <llama-server>.",
                    "llama.cpp ne publie pas de version pour {os} sur {arch} : compilez llama-server, puis delest models engine <llama-server>."
                )
            }
            Unavailable::MacosTooOld(found) => tr!(
                lang,
                "llama.cpp's builds need macOS 13.3 or later, and this Mac has macOS {found}: install it with brew install llama.cpp or build it, then delest models engine <llama-server>.",
                "les versions de llama.cpp demandent macOS 13.3 ou plus récent, et ce Mac a macOS {found} : installez-le avec brew install llama.cpp ou compilez-le, puis delest models engine <llama-server>."
            ),
            Unavailable::VcRuntimeTooOld(found) => {
                let found = found.clone().unwrap_or_else(|| tr!(lang, "none", "aucun"));
                tr!(
                    lang,
                    "llama.cpp's build needs the Visual C++ runtime 14.40 or later (found: {found}): install Microsoft's Visual C++ Redistributable ({VC_REDIST}), then try again.",
                    "la version de llama.cpp demande le runtime Visual C++ 14.40 ou plus récent (trouvé : {found}) : installez le Visual C++ Redistributable de Microsoft ({VC_REDIST}), puis réessayez."
                )
            }
        }
    }
}

/// The version of the Visual C++ runtime in System32, `14.00.23026.0` and the like.
fn vc_runtime_version() -> Option<String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Item \"$env:WINDIR\\System32\\msvcp140.dll\").VersionInfo.FileVersion",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.split_whitespace().next()?.to_string();
    major_minor(&version).map(|_| version)
}

fn macos_version() -> Option<String> {
    let output = Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `12.7.6` → `(12, 7)`.
fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.').map(|part| part.parse::<u32>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0)))
}

/// The build for this system and processor, when one exists and can run here.
pub fn engine_build() -> Result<Build, Unavailable> {
    let build = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => MACOS_ARM64,
        ("macos", "x86_64") => MACOS_X64,
        ("linux", "x86_64") => LINUX_X64,
        ("windows", "x86_64") => WINDOWS_X64,
        _ => return Err(Unavailable::NoBuild),
    };
    if cfg!(target_os = "macos")
        && let Some(found) = macos_version()
        && major_minor(&found).is_some_and(|version| version < MACOS_MINIMUM)
    {
        return Err(Unavailable::MacosTooOld(found));
    }
    if cfg!(windows) {
        let found = vc_runtime_version();
        let recent = found
            .as_deref()
            .and_then(major_minor)
            .is_some_and(|version| version >= VC_RUNTIME_MINIMUM);
        if !recent {
            return Err(Unavailable::VcRuntimeTooOld(found));
        }
    }
    Ok(build)
}

fn engine_dir(store: &Path) -> PathBuf {
    store.join("engine").join(ENGINE_RELEASE)
}

/// Where the engine's archive is written while it downloads.
pub fn engine_part(store: &Path, build: &Build) -> PathBuf {
    store.join("engine").join(format!("{}.part", build.asset))
}

/// The server of an engine installed this way. The archives differ: Windows' has its
/// files at the top, the others in a folder.
pub fn installed_engine(store: &Path) -> Option<PathBuf> {
    let program = crate::paths::executable_name("llama-server");
    let dir = engine_dir(store);
    let top = dir.join(&program);
    if top.is_file() {
        return Some(top);
    }
    fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(&program))
        .find(|path| path.is_file())
}

/// Downloads this system's build, checks it, and unpacks it into the store. `tar`
/// reads both the .tar.gz and Windows' .zip (it comes with Windows 10 and later).
pub fn install_engine(store: &Path) -> io::Result<PathBuf> {
    let build = engine_build().map_err(|why| io::Error::other(why.text(Lang::En)))?;
    let part = engine_part(store, &build);
    let url = format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{ENGINE_RELEASE}/{}",
        build.asset
    );
    models::fetch(&url, &part, false)?;
    models::check(&part, build.sha256)?;
    let dir = engine_dir(store);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let unpacked = Command::new("tar")
        .arg("-xf")
        .arg(&part)
        .arg("-C")
        .arg(&dir)
        .stdout(Stdio::null())
        .status()?
        .success();
    fs::remove_file(&part)?;
    if !unpacked {
        return Err(io::Error::other(format!(
            "tar could not unpack {}",
            build.asset
        )));
    }
    let program = installed_engine(store)
        .ok_or_else(|| io::Error::other("llama-server is not in the archive"))?;
    // A build that cannot start here (a library missing) is not kept.
    let tried = Command::new(&program)
        .arg("--version")
        .stdin(Stdio::null())
        .output()?;
    if !tried.status.success() {
        let _ = fs::remove_dir_all(&dir);
        let reason = String::from_utf8_lossy(&tried.stderr);
        let mut reason = reason.lines().next().unwrap_or("").trim().to_string();
        // On Windows, a crash at start without a word is a runtime or a DLL missing.
        if reason.is_empty() && cfg!(windows) {
            reason = format!(
                "it stopped at start (code {:?}); the Visual C++ runtime is likely too old: {VC_REDIST}",
                tried.status.code()
            );
        }
        return Err(io::Error::other(format!(
            "llama-server does not start on this system: {reason}"
        )));
    }
    Ok(program)
}

/// What one step of the installation fetches.
#[derive(Clone, Debug)]
pub enum What {
    Engine(Build),
    /// A model, from a copy already on the disk when there is one.
    Model {
        known: &'static Known,
        copy: Option<PathBuf>,
    },
}

#[derive(Clone, Debug)]
pub struct Step {
    pub what: What,
    /// The file being written, to follow the progress by its size.
    pub part: PathBuf,
    /// What will travel over the network: nothing for a copy found on the disk.
    pub download: u64,
}

/// What the local AI lacks, and what can be fetched for it.
#[derive(Clone, Debug)]
pub struct Plan {
    /// In order; empty when nothing is left to fetch.
    pub steps: Vec<Step>,
    /// The engine is missing and cannot be fetched here: why.
    pub engine: Option<Unavailable>,
}

/// What is missing for the local AI. Copies of the models are looked for in `roots`
/// (other tools' caches, scanned folders).
pub fn plan(store: &Path, roots: &[PathBuf]) -> io::Result<Plan> {
    let mut steps = Vec::new();
    let mut unavailable = None;
    if engine::server_program(store).is_none() {
        match engine_build() {
            Ok(build) => steps.push(Step {
                part: engine_part(store, &build),
                download: build.size,
                what: What::Engine(build),
            }),
            Err(why) => unavailable = Some(why),
        }
    }
    let manifest = Manifest::load(store)?;
    for name in MODELS {
        let Some(known) = models::known(name) else {
            continue;
        };
        if manifest.path(store, name).is_some() {
            continue;
        }
        let copy = models::find_copy(known, roots);
        steps.push(Step {
            part: models::part_path(store, known),
            download: if copy.is_some() { 0 } else { known.size },
            what: What::Model { known, copy },
        });
    }
    Ok(Plan {
        steps,
        engine: unavailable,
    })
}

/// Carries out one step of the plan.
pub fn run(store: &Path, step: &Step) -> io::Result<()> {
    match &step.what {
        What::Engine(_) => install_engine(store).map(drop),
        What::Model {
            known,
            copy: Some(copy),
        } => models::add_local(store, known, copy).map(drop),
        What::Model { known, copy: None } => models::download(store, known, false).map(drop),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_build_is_pinned_and_old_macos_is_told_why() {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") | ("windows", "x86_64") => assert!(engine_build().is_ok()),
            ("macos", _) => assert!(matches!(
                engine_build(),
                Ok(_) | Err(Unavailable::MacosTooOld(_))
            )),
            _ => {}
        }
        assert_eq!(major_minor("12.7.6"), Some((12, 7)));
        assert_eq!(major_minor("14"), Some((14, 0)));
        assert!(major_minor("12.7.6").unwrap() < MACOS_MINIMUM);
        assert!(major_minor("13.3").unwrap() >= MACOS_MINIMUM);
        assert!(major_minor("14.00.23026.0").unwrap() < VC_RUNTIME_MINIMUM);
        assert!(major_minor("14.44.35211.0").unwrap() >= VC_RUNTIME_MINIMUM);
        for build in [MACOS_ARM64, MACOS_X64, LINUX_X64, WINDOWS_X64] {
            assert!(build.asset.contains(ENGINE_RELEASE));
            assert_eq!(build.sha256.len(), 64);
            assert!(build.size > 1_000_000);
        }
    }

    #[test]
    fn the_plan_takes_a_copy_on_the_disk_before_a_download() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("embeddinggemma-2-Q8_0.gguf"), "x").unwrap();
        let steps = plan(&store, std::slice::from_ref(&cache)).unwrap().steps;
        let model = |name: &str| {
            steps
                .iter()
                .find(|step| matches!(&step.what, What::Model { known, .. } if known.name == name))
        };
        let recognition = model("embeddinggemma-2").unwrap();
        assert_eq!(recognition.download, 0);
        assert!(matches!(
            &recognition.what,
            What::Model { copy: Some(_), .. }
        ));
        let reasoning = model("gemma-4-e2b").unwrap();
        assert_eq!(reasoning.download, 2_841_481_184);
    }

    #[test]
    fn a_copy_that_is_not_the_published_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("store");
        let copy = dir.path().join("embeddinggemma-2-Q8_0.gguf");
        fs::write(&copy, "not the model").unwrap();
        let known = models::known("embeddinggemma-2").unwrap();
        assert!(models::add_local(&store, known, &copy).is_err());
        assert!(!store.join(known.file).exists());
        assert!(!models::part_path(&store, known).exists());
        assert!(copy.exists());
    }
}
