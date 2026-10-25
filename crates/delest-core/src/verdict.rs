//! What Delest concludes about an item, and why.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::lang::{self, Lang};
use crate::tr;

/// From the most cautious to the least. The Arbiter never lets a later source
/// move an item towards the end of this list once a ban has spoken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Never proposed.
    Protected,
    /// Not recognised: never proposed, and said so rather than guessed.
    Unknown,
    /// Possibly freeable, but nothing guarantees an identical rebuild: proposed only
    /// with the developer's explicit consent.
    ToReview,
    /// Rebuilt identically from the network; never proposed offline.
    RebuildOnline,
    /// Rebuilt locally.
    Rebuildable,
}

impl Verdict {
    /// The verdict names used by the acceptance corpus.
    pub fn from_corpus(name: &str) -> Option<Verdict> {
        match name {
            "protege" => Some(Verdict::Protected),
            "a_juger" => Some(Verdict::ToReview),
            "supprimable_en_ligne" => Some(Verdict::RebuildOnline),
            "supprimable" => Some(Verdict::Rebuildable),
            _ => None,
        }
    }

    /// Whether Delest may offer to free it without asking what it is.
    pub fn is_freeable(self) -> bool {
        matches!(self, Verdict::RebuildOnline | Verdict::Rebuildable)
    }

    pub fn label(self, lang: Lang) -> String {
        match self {
            Verdict::Protected => tr!(lang, "protected", "protégé"),
            Verdict::Unknown => tr!(lang, "unknown", "inconnu"),
            Verdict::ToReview => tr!(lang, "to review", "à juger"),
            Verdict::RebuildOnline => tr!(lang, "freeable online", "supprimable avec réseau"),
            Verdict::Rebuildable => tr!(lang, "freeable", "supprimable"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Build,
    Dependencies,
    GlobalCache,
    PlatformTool,
    AiModels,
    Unknown,
}

impl Category {
    pub fn label(self, lang: Lang) -> String {
        match self {
            Category::Build => tr!(lang, "build", "compilation"),
            Category::Dependencies => tr!(lang, "dependencies", "dépendances"),
            Category::GlobalCache => tr!(lang, "global cache", "cache global"),
            Category::PlatformTool => tr!(lang, "platform tool", "outil de plateforme"),
            Category::AiModels => tr!(lang, "AI models", "modèles d'IA"),
            Category::Unknown => tr!(lang, "unknown", "inconnu"),
        }
    }
}

/// One step of the trace: a fact the Arbiter relied on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    GitTracked {
        files: usize,
    },
    GitUntracked,
    GitUnreadable,
    GitIgnored,
    OutsideGit,
    LastCommit {
        days: u64,
    },
    SensitiveFile {
        file: String,
    },
    /// A part the project needs that no build recreates (Unity's Assets, Tauri's binaries).
    ProjectPart {
        marker: String,
    },
    SourceCode {
        file: String,
    },
    ModelFiles {
        file: String,
    },
    Manifest {
        file: String,
    },
    NoManifest,
    WeakManifestOutsideGit {
        file: String,
    },
    Lockfile {
        file: String,
    },
    PartialLockfile {
        file: String,
    },
    NoLockfile,
    NotAVirtualEnv,
    Bytecode,
    ToolCache,
    InShadow {
        rule: String,
    },
    KnownCache {
        note: CacheNote,
    },
    NotAnArtifact,
    ProtectedByYou {
        path: String,
    },
    RefusedByYou {
        times: u32,
    },
    RestoredRecently {
        days: i64,
    },
    RuleRestored {
        count: usize,
    },
    Activity {
        days: u64,
        source: ActivitySource,
    },
}

/// What tells when a project was last worked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    Commit,
    Files,
    Handoff,
}

/// Why a global cache gets its verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheNote {
    Downloaded,
    Mixed,
    Models,
    Toolchains,
    AppCaches,
    Simulators,
    Docker,
    XcodeBuild,
}

impl Reason {
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Reason::GitTracked { files } => {
                tr!(
                    lang,
                    "{files} files tracked by git",
                    "{files} fichiers suivis par git"
                )
            }
            Reason::GitUntracked => tr!(
                lang,
                "neither tracked nor ignored by git",
                "ni suivi ni ignoré par git"
            ),
            Reason::GitUnreadable => tr!(
                lang,
                "inside a git repository that git could not read",
                "dans un dépôt git que git n'a pas pu lire"
            ),
            Reason::GitIgnored => tr!(lang, "ignored by git", "ignoré par git"),
            Reason::OutsideGit => tr!(lang, "outside any git repository", "hors de tout dépôt git"),
            Reason::LastCommit { days } => {
                let ago = lang::ago(*days, lang);
                tr!(lang, "last commit {ago}", "dernier commit {ago}")
            }
            Reason::ProjectPart { marker } => tr!(
                lang,
                "a part of the project that no build recreates (next to {marker})",
                "une partie du projet qu'aucune compilation ne recrée (à côté de {marker})"
            ),
            Reason::SensitiveFile { file } => tr!(
                lang,
                "holds {file} (secret, local configuration or data)",
                "contient {file} (secret, configuration locale ou données)"
            ),
            Reason::SourceCode { file } => tr!(
                lang,
                "holds source code ({file}) and no project manifest says it is an artefact",
                "contient du code source ({file}) et aucun manifeste n'en fait un artefact"
            ),
            Reason::ModelFiles { file } => tr!(
                lang,
                "holds AI models ({file}): deduplication into the shared store only",
                "contient des modèles d'IA ({file}) : dédoublonnage dans le dépôt commun seulement"
            ),
            Reason::Manifest { file } => tr!(lang, "next to {file}", "à côté de {file}"),
            Reason::NoManifest => tr!(
                lang,
                "no project manifest next to it",
                "aucun manifeste de projet à côté"
            ),
            Reason::WeakManifestOutsideGit { file } => tr!(
                lang,
                "outside git, {file} alone does not prove it is an artefact",
                "hors git, {file} seul ne prouve pas que c'est un artefact"
            ),
            Reason::Lockfile { file } => tr!(lang, "lockfile {file}", "lockfile {file}"),
            Reason::PartialLockfile { file } => tr!(
                lang,
                "only {file}: a reinstall may bring other versions",
                "seulement {file} : une réinstallation peut donner d'autres versions"
            ),
            Reason::NoLockfile => tr!(lang, "no lockfile", "aucun lockfile"),
            Reason::NotAVirtualEnv => tr!(
                lang,
                "named like a virtual environment, but has no pyvenv.cfg",
                "nommé comme un environnement virtuel, mais sans pyvenv.cfg"
            ),
            Reason::Bytecode => tr!(
                lang,
                "Python bytecode, regenerated on the next run",
                "bytecode Python, régénéré à la prochaine exécution"
            ),
            Reason::ToolCache => tr!(
                lang,
                "cache of a development tool, regenerated on its next run",
                "cache d'un outil de développement, régénéré à sa prochaine exécution"
            ),
            Reason::InShadow { rule } => tr!(
                lang,
                "new rule {rule} in shadow: never offered until you accept it (delest shadow)",
                "nouvelle règle {rule} en ombre : jamais proposée avant votre accord (delest shadow)"
            ),
            Reason::KnownCache { note } => note.text(lang),
            Reason::ProtectedByYou { path } => tr!(
                lang,
                "protected by you ({path})",
                "protégé par vous ({path})"
            ),
            Reason::RefusedByYou { times } => tr!(
                lang,
                "refused {times} times: no longer offered",
                "refusé {times} fois : n'est plus proposé"
            ),
            Reason::RestoredRecently { days } => {
                let ago = lang::ago((*days).max(0) as u64, lang);
                tr!(
                    lang,
                    "put back from the quarantine {ago}: to review",
                    "remis en place depuis la quarantaine {ago} : à juger"
                )
            }
            Reason::RuleRestored { count } => tr!(
                lang,
                "{count} items of this rule were put back lately: to review",
                "{count} éléments de cette règle ont été remis en place récemment : à juger"
            ),
            Reason::Activity { days, source } => {
                let source = match source {
                    ActivitySource::Commit => tr!(lang, "last commit", "dernier commit"),
                    ActivitySource::Files => tr!(lang, "last file change", "dernière modification"),
                    ActivitySource::Handoff => {
                        tr!(lang, "last Handoff save", "dernière passation Handoff")
                    }
                };
                let ago = lang::ago(*days, lang);
                tr!(
                    lang,
                    "project worked on {ago} ({source})",
                    "projet travaillé {ago} ({source})"
                )
            }
            Reason::NotAnArtifact => tr!(
                lang,
                "matches no known artefact",
                "ne correspond à aucun artefact connu"
            ),
        }
    }
}

impl CacheNote {
    pub fn text(self, lang: Lang) -> String {
        match self {
            CacheNote::Downloaded => tr!(lang, "download cache", "cache de téléchargement"),
            CacheNote::Mixed => tr!(
                lang,
                "mixed content, AI models included",
                "contenu hétérogène, modèles d'IA compris"
            ),
            CacheNote::Models => tr!(
                lang,
                "AI models: deduplication only",
                "modèles d'IA : dédoublonnage seulement"
            ),
            CacheNote::Toolchains => tr!(
                lang,
                "toolchains: reinstall needed",
                "chaînes d'outils : réinstallation nécessaire"
            ),
            CacheNote::AppCaches => tr!(
                lang,
                "caches of every application, not only development",
                "caches de toutes les applications, pas seulement de développement"
            ),
            CacheNote::Simulators => tr!(
                lang,
                "simulators may hold app data",
                "les simulateurs peuvent contenir des données d'apps"
            ),
            CacheNote::Docker => tr!(
                lang,
                "Docker storage: only through Docker's own operations; volumes are banned",
                "stockage de Docker : seulement par les opérations de Docker ; les volumes sont interdits"
            ),
            CacheNote::XcodeBuild => {
                tr!(lang, "Xcode build data", "données de compilation de Xcode")
            }
        }
    }
}

/// The Arbiter's conclusion about one item, with its trace.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assessment {
    pub path: PathBuf,
    pub verdict: Verdict,
    pub category: Category,
    /// The rule that recognised it, by name.
    pub rule: Option<String>,
    pub reasons: Vec<Reason>,
    /// Bytes on disk, when measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// For dependencies: days since their project was last worked on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_days: Option<u64>,
    /// For a rule in shadow: what it would conclude, while the item stays unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Verdict>,
}

impl Assessment {
    pub fn trace(&self, lang: Lang) -> String {
        self.reasons
            .iter()
            .map(|reason| reason.text(lang))
            .collect::<Vec<_>>()
            .join(&tr!(lang, "; ", " ; "))
    }
}
