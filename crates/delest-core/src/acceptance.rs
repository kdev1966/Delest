//! Replays the acceptance corpus: real items with the verdict the developer set.
//! Blocking criterion: no item whose reference is protected may come out freeable.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::arbiter::Arbiter;
use crate::expand_home;
use crate::verdict::{Assessment, Verdict};

#[derive(Deserialize)]
struct Corpus {
    #[serde(default)]
    dossier: Vec<Entry>,
    #[serde(default)]
    cache: Vec<Entry>,
    #[serde(default)]
    piege: Vec<Entry>,
    /// Folders no rule recognised when they were surveyed: they measure the model.
    #[serde(default)]
    inconnu: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    chemin: String,
    reference: String,
    /// The synthetic corpus only: what to create, relative to the home folder.
    #[serde(default)]
    fichiers: Vec<String>,
    /// A git repository to make there, every file in it committed.
    #[serde(default)]
    depot: Option<String>,
    /// Its `.gitignore`, written before the commit.
    #[serde(default)]
    ignore: Option<String>,
    /// Files made after the commit, so untracked.
    #[serde(default)]
    apres: Vec<String>,
}

pub struct Case {
    pub section: &'static str,
    pub path: PathBuf,
    pub reference: Verdict,
    /// `None` when the item no longer exists.
    pub assessment: Option<Assessment>,
}

impl Case {
    /// An unknown item is never proposed, which is what a protected reference asks.
    pub fn agrees(&self) -> bool {
        self.assessment.as_ref().is_some_and(|assessment| {
            assessment.verdict == self.reference
                || (self.reference == Verdict::Protected && assessment.verdict == Verdict::Unknown)
        })
    }

    /// Whether the case measures the model rather than the rules.
    pub fn is_unknown_folder(&self) -> bool {
        self.section == "inconnu"
    }

    pub fn is_blocking(&self) -> bool {
        self.reference == Verdict::Protected
            && self
                .assessment
                .as_ref()
                .is_some_and(|assessment| assessment.verdict.is_freeable())
    }
}

#[derive(Debug)]
pub enum Error {
    Toml(toml::de::Error),
    Reference { path: String, value: String },
    Build { path: String, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Toml(error) => write!(f, "{error}"),
            Error::Reference { path, value } => {
                write!(f, "{path}: unknown reference verdict '{value}'")
            }
            Error::Build { path, source } => write!(f, "{path}: {source}"),
        }
    }
}

impl std::error::Error for Error {}

/// Makes the synthetic corpus's cases under `home`: their files, their git
/// repositories, what git ignores and what it does not track.
pub fn build(text: &str, home: &Path) -> Result<(), Error> {
    let corpus: Corpus = toml::from_str(text).map_err(Error::Toml)?;
    let entries = [corpus.dossier, corpus.cache, corpus.piege, corpus.inconnu];
    for entry in entries.iter().flatten() {
        let fail = |source: io::Error| Error::Build {
            path: entry.chemin.clone(),
            source,
        };
        let write = |file: &String| -> io::Result<()> {
            let path = home.join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, "x")
        };
        for file in &entry.fichiers {
            write(file).map_err(fail)?;
        }
        if let Some(repository) = &entry.depot {
            let dir = home.join(repository);
            fs::create_dir_all(&dir).map_err(fail)?;
            if let Some(ignore) = &entry.ignore {
                fs::write(dir.join(".gitignore"), ignore).map_err(fail)?;
            }
            // The developer's global ignore file must not change the verdicts.
            fs::write(dir.join(".no-global-excludes"), "").map_err(fail)?;
            for args in [
                &["init", "-q"][..],
                &["config", "core.excludesFile", ".no-global-excludes"],
                &["add", "-A"],
                &[
                    "-c",
                    "user.name=Delest",
                    "-c",
                    "user.email=corpus@delest.invalid",
                    "commit",
                    "-q",
                    "-m",
                    "corpus",
                ],
            ] {
                git(&dir, args).map_err(fail)?;
            }
        }
        for file in &entry.apres {
            write(file).map_err(fail)?;
        }
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> io::Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git {} failed", args.join(" "))))
    }
}

pub fn replay(text: &str, arbiter: &Arbiter, home: &Path) -> Result<Vec<Case>, Error> {
    let corpus: Corpus = toml::from_str(text).map_err(Error::Toml)?;
    let sections = [
        ("dossier", corpus.dossier),
        ("cache", corpus.cache),
        ("piege", corpus.piege),
        ("inconnu", corpus.inconnu),
    ];
    let mut cases = Vec::new();
    for (section, entries) in sections {
        for entry in entries {
            let reference =
                Verdict::from_corpus(&entry.reference).ok_or_else(|| Error::Reference {
                    path: entry.chemin.clone(),
                    value: entry.reference.clone(),
                })?;
            let path = expand_home(Path::new(&entry.chemin), home);
            let assessment = path.exists().then(|| arbiter.assess(&path));
            cases.push(Case {
                section,
                path,
                reference,
                assessment,
            });
        }
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn a_protected_reference_that_comes_out_freeable_is_blocking() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".npm")).unwrap();
        fs::write(home.path().join("notes.db"), "x").unwrap();
        let corpus = r#"
            [[cache]]
            chemin = "~/.npm"
            reference = "protege"

            [[piege]]
            chemin = "~/notes.db"
            reference = "protege"

            [[dossier]]
            chemin = "~/gone/node_modules"
            reference = "supprimable_en_ligne"

            [[inconnu]]
            chemin = "~/.npm"
            reference = "a_juger"
        "#;
        let arbiter = Arbiter::new(home.path().to_path_buf());
        let cases = replay(corpus, &arbiter, home.path()).unwrap();
        let by_path = |end: &str| cases.iter().find(|case| case.path.ends_with(end)).unwrap();
        assert!(by_path(".npm").is_blocking());
        assert!(by_path("notes.db").agrees());
        let unknown = cases.iter().find(|case| case.section == "inconnu").unwrap();
        assert!(unknown.is_unknown_folder());
        assert!(!by_path("notes.db").is_unknown_folder());
        assert!(by_path("node_modules").assessment.is_none());
    }

    /// Phase 5's criterion: the corpus passes on each system. The synthetic corpus
    /// recreates the reference machine's situations wherever the tests run.
    #[test]
    fn the_synthetic_corpus_passes_on_this_system() {
        let text = include_str!("../../../corpus/synthetique.toml");
        let dir = tempfile::tempdir().unwrap();
        let home = crate::paths::canonical(dir.path()).unwrap();
        build(text, &home).unwrap();
        let arbiter = Arbiter::new(home.clone());
        let cases = replay(text, &arbiter, &home).unwrap();
        assert_eq!(cases.len(), 34);
        let wrong: Vec<String> = cases
            .iter()
            .filter(|case| !case.agrees() || case.is_blocking())
            .map(|case| {
                let got = case
                    .assessment
                    .as_ref()
                    .map(|assessment| format!("{:?} {:?}", assessment.verdict, assessment.reasons));
                format!("{}: {:?} → {got:?}", case.path.display(), case.reference)
            })
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn an_unknown_reference_is_an_error() {
        let arbiter = Arbiter::new(PathBuf::from("/nonexistent-home"));
        let corpus = "[[dossier]]\nchemin = \"/x\"\nreference = \"peut-etre\"\n";
        assert!(matches!(
            replay(corpus, &arbiter, Path::new("/")),
            Err(Error::Reference { .. })
        ));
    }
}
