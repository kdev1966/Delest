//! The model's opinion on the folders no rule recognises. EmbeddingGemma 2 finds
//! the three closest folders the rules already judged; Gemma 4 E2B, given them as
//! examples, says what the folder is and picks between "keep" and "to review". The
//! grammar leaves it no other answer: the model can never make a folder freeable,
//! only offer it for review, one by one, with the developer's consent.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::{Engine, Mode};
use crate::lang::Lang;
use crate::rules::RULES;
use crate::verdict::{Assessment, Verdict};

const OPINIONS: &str = "avis.json";
const VECTORS: &str = "vecteurs.json";
const NEIGHBOURS: usize = 3;
/// A folder whose size moved by more than this share gets a new opinion.
const SIZE_DRIFT: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advice {
    /// The developer's work, data, a model or anything hard to get back.
    Keep,
    /// Probably rebuildable: offered one by one, never checked in advance.
    Review,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Neighbour {
    pub path: PathBuf,
    pub verdict: Verdict,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Opinion {
    pub path: PathBuf,
    /// What the folder is, in a few words.
    pub nature: String,
    pub advice: Advice,
    pub reason: String,
    pub neighbours: Vec<Neighbour>,
    pub model: String,
    /// Seconds since the Unix epoch.
    pub at: i64,
    pub size: u64,
    pub seconds: f32,
}

/// The opinions given so far, by folder: the journal the interface shows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Opinions {
    #[serde(default)]
    pub folders: BTreeMap<PathBuf, Opinion>,
}

impl Opinions {
    pub fn load(data_dir: &Path) -> Opinions {
        fs::read_to_string(data_dir.join(OPINIONS))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(data_dir.join(OPINIONS), json)
    }

    /// The opinion still valid for this folder: same place, size within a tenth.
    pub fn current(&self, item: &Assessment) -> Option<&Opinion> {
        let opinion = self.folders.get(&item.path)?;
        let size = item.size.unwrap_or(0) as f64;
        let drift = (size - opinion.size as f64).abs() / size.max(1.0);
        (drift <= SIZE_DRIFT).then_some(opinion)
    }
}

/// What the model is told about a folder: names and facts only, no file content.
pub fn describe(item: &Assessment) -> String {
    let path = &item.path;
    let parent = path.parent().unwrap_or(path);
    let markers: Vec<String> = fs::read_dir(parent)
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name == ".git"
                || name == "ProjectSettings"
                || RULES
                    .iter()
                    .flat_map(|rule| rule.manifests)
                    .any(|manifest| manifest.file == name)
        })
        .collect();
    let mut inside: Vec<String> = fs::read_dir(path)
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    inside.sort();
    let count = inside.len();
    inside.truncate(20);
    let name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let size_mb = item.size.unwrap_or(0) / 1_000_000;
    let trace: Vec<String> = item
        .reasons
        .iter()
        .map(|reason| reason.text(Lang::Fr))
        .collect();
    format!(
        "nom : {}\ndossier parent : {}\nmarqueurs dans le dossier parent : {}\ncontenu de premier niveau ({count} éléments) : {}\ntaille en Mo : {size_mb}\nfaits établis : {}",
        name(path),
        name(parent),
        if markers.is_empty() {
            "aucun".into()
        } else {
            markers.join(", ")
        },
        inside.join(", "),
        trace.join(" ; "),
    )
}

fn corpus_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Protected | Verdict::Unknown => "protege",
        Verdict::ToReview => "a_juger",
        Verdict::RebuildOnline => "supprimable_en_ligne",
        Verdict::Rebuildable => "supprimable",
    }
}

const GRAMMAR: &str = r#"root ::= "{" ws "\"nature\":" ws text60 "," ws "\"avis\":" ws avis "," ws "\"motif\":" ws text240 ws "}"
avis ::= "\"a_garder\"" | "\"a_juger\""
text60 ::= "\"" [^"\n]{1,60} "\""
text240 ::= "\"" [^"\n]{1,240} "\""
ws ::= [ ]?
"#;

fn prompt(description: &str, examples: &[(String, Verdict)]) -> String {
    let examples: Vec<String> = examples
        .iter()
        .enumerate()
        .map(|(index, (text, verdict))| {
            format!(
                "Exemple {} (verdict : {}) :\n{text}",
                index + 1,
                corpus_name(*verdict)
            )
        })
        .collect();
    format!(
        "Tu es l'assistant de Delest, un outil qui libère l'espace disque d'un développeur sans jamais altérer son travail.\n\
         Aucune règle ne reconnaît le dossier ci-dessous. Dis en quelques mots ce qu'il est (nature), puis donne ton avis :\n\
         - a_garder : le travail du développeur, des données, un modèle d'IA ou un fichier difficile à retrouver ; et dans le doute.\n\
         - a_juger : probablement reconstructible (cache, compilation, téléchargement) ; le développeur décidera lui-même.\n\
         Explique ton avis en une ou deux phrases, en français, sans inventer de fait absent de la description.\n\n\
         Voici des dossiers proches, déjà jugés par les règles de Delest :\n{}\n\n\
         Dossier à juger :\n{description}\n\nRéponds en JSON.",
        examples.join("\n\n")
    )
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The models and the server the advisor runs.
pub struct Models<'a> {
    pub program: &'a Path,
    pub embedding: &'a Path,
    pub reasoning: &'a Path,
    pub reasoning_name: &'a str,
    pub threads: usize,
}

/// Gives an opinion on each unknown folder that has none still valid, using the
/// recognised folders of the scan as examples. Returns the new opinions.
pub fn advise(
    unknowns: &[Assessment],
    examples: &[Assessment],
    models: &Models,
    data_dir: &Path,
    mut progress: impl FnMut(&Opinion),
) -> io::Result<Vec<Opinion>> {
    let mut opinions = Opinions::load(data_dir);
    let todo: Vec<&Assessment> = unknowns
        .iter()
        .filter(|item| opinions.current(item).is_none())
        .collect();
    if todo.is_empty() {
        return Ok(Vec::new());
    }
    let examples: Vec<(&Assessment, String)> = examples
        .iter()
        .filter(|item| item.verdict != Verdict::Unknown && item.path.exists())
        .map(|item| (item, describe(item)))
        .collect();
    let questions: Vec<String> = todo.iter().map(|item| describe(item)).collect();

    // Recognition: vectors of the examples (cached) and of the questions.
    let mut vectors: BTreeMap<String, Vec<f32>> = fs::read_to_string(data_dir.join(VECTORS))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let mut question_vectors = Vec::new();
    {
        let mut engine = Engine::start(
            models.program,
            models.embedding,
            Mode::Embedding,
            models.threads,
        )?;
        for (_, text) in &examples {
            // An example that cannot be embedded is left out, not the whole task.
            if let std::collections::btree_map::Entry::Vacant(slot) = vectors.entry(digest(text))
                && let Ok(vector) = engine.embed(text)
            {
                slot.insert(vector);
            }
        }
        for text in &questions {
            question_vectors.push(engine.embed(text)?);
        }
    }
    fs::create_dir_all(data_dir)?;
    let _ = fs::write(
        data_dir.join(VECTORS),
        serde_json::to_string(&vectors).map_err(io::Error::other)?,
    );

    // Reasoning: one answer per folder, with its neighbours as examples.
    let mut engine = Engine::start(models.program, models.reasoning, Mode::Chat, models.threads)?;
    let mut given = Vec::new();
    for ((item, text), vector) in todo.iter().zip(&questions).zip(&question_vectors) {
        let mut scored: Vec<(f32, &(&Assessment, String))> = examples
            .iter()
            .filter_map(|example| {
                Some((cosine(vector, vectors.get(&digest(&example.1))?), example))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let nearest: Vec<&(&Assessment, String)> = scored
            .into_iter()
            .take(NEIGHBOURS)
            .map(|(_, example)| example)
            .collect();
        let shown: Vec<(String, Verdict)> = nearest
            .iter()
            .map(|(example, text)| (text.clone(), example.verdict))
            .collect();
        let started = Instant::now();
        let answer = engine.chat(&prompt(text, &shown), Some(GRAMMAR), 200)?;
        let seconds = started.elapsed().as_secs_f32();
        let parsed: serde_json::Value = serde_json::from_str(&answer).map_err(io::Error::other)?;
        let field = |name: &str| parsed[name].as_str().unwrap_or_default().to_string();
        let opinion = Opinion {
            path: item.path.clone(),
            nature: field("nature"),
            advice: if field("avis") == "a_juger" {
                Advice::Review
            } else {
                Advice::Keep
            },
            reason: field("motif"),
            neighbours: nearest
                .iter()
                .map(|(example, _)| Neighbour {
                    path: example.path.clone(),
                    verdict: example.verdict,
                })
                .collect(),
            model: models.reasoning_name.to_string(),
            at: now(),
            size: item.size.unwrap_or(0),
            seconds,
        };
        progress(&opinion);
        opinions.folders.insert(item.path.clone(), opinion.clone());
        opinions.save(data_dir)?;
        given.push(opinion);
    }
    Ok(given)
}

/// What is needed to run the models now, found in the store.
pub struct Setup {
    pub program: PathBuf,
    pub embedding: PathBuf,
    pub reasoning: PathBuf,
    pub reasoning_name: &'static str,
    pub threads: usize,
}

/// Why the models cannot run now.
#[derive(Debug, PartialEq, Eq)]
pub enum Missing {
    /// Not enough free memory, or a low battery.
    Capacity,
    Model(&'static str),
    Engine,
}

impl Missing {
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Missing::Capacity => crate::tr!(
                lang,
                "not enough free memory, or the battery is low: no model is started now.",
                "pas assez de mémoire libre, ou batterie faible : aucun modèle n'est lancé maintenant."
            ),
            Missing::Model(name) => crate::tr!(
                lang,
                "{name} is not installed: Settings › AI › Install the local AI, or delest models add {name}",
                "{name} n'est pas installé : Réglages › IA › Installer l'IA locale, ou delest models add {name}"
            ),
            Missing::Engine => crate::tr!(
                lang,
                "no llama.cpp server: Settings › AI › Install the local AI, or delest models engine",
                "pas de serveur llama.cpp : Réglages › IA › Installer l'IA locale, ou delest models engine"
            ),
        }
    }
}

/// The engine and the models that fit the machine right now.
pub fn setup(home: &Path) -> Result<Setup, Missing> {
    use crate::capacities::{self, Reasoning};
    let (name, label) =
        match capacities::reasoning(capacities::available_memory(), capacities::on_battery()) {
            Reasoning::Main => ("gemma-4-e2b", "Gemma 4 E2B"),
            Reasoning::Fallback => ("qwen3.5-2b", "Qwen3.5 2B"),
            Reasoning::None => return Err(Missing::Capacity),
        };
    let store = crate::models::store_dir(home);
    let manifest = crate::models::Manifest::load(&store).unwrap_or_default();
    let embedding = manifest
        .path(&store, "embeddinggemma-2")
        .ok_or(Missing::Model("embeddinggemma-2"))?;
    let reasoning = manifest.path(&store, name).ok_or(Missing::Model(name))?;
    let program = crate::engine::server_program(&store).ok_or(Missing::Engine)?;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(2)
        .max(2);
    Ok(Setup {
        program,
        embedding,
        reasoning,
        reasoning_name: label,
        threads,
    })
}

impl Setup {
    pub fn models(&self) -> Models<'_> {
        Models {
            program: &self.program,
            embedding: &self.embedding,
            reasoning: &self.reasoning,
            reasoning_name: self.reasoning_name,
            threads: self.threads,
        }
    }
}

/// The question a developer asks about a folder, worded for a model: the folder's
/// description (names and facts only) and the rules Delest keeps to. The answer is
/// asked in the interface's language: a small model follows the prompt's French otherwise.
pub fn question_prompt(item: &Assessment, question: &str, lang: Lang) -> String {
    let language = match lang {
        Lang::En => "en anglais",
        Lang::Fr => "en français",
    };
    format!(
        "Tu es l'assistant de Delest, un outil qui libère l'espace disque d'un développeur sans jamais altérer son travail.\n\
         Réponds {language}, en trois phrases au plus, à la question du développeur sur ce dossier. \
         Delest ne fait que supprimer : ce sont la compilation ou l'installation des dépendances qui recréent un dossier, jamais Delest. \
         Ne t'appuie que sur la description et sur ce que Delest sait déjà ; si cela ne suffit pas, dis-le. \
         Ne conseille jamais de supprimer quelque chose que tu ne peux pas identifier.\n\n\
         Dossier : {}\n{}{}\n\nQuestion : {question}",
        item.path.display(),
        describe(item),
        known(item)
    )
}

/// What Delest's rules already concluded about a folder they recognise, so that a
/// free question gets the facts Delest itself acts on. Empty for an unknown folder:
/// the model's opinion on those must stay its own.
fn known(item: &Assessment) -> String {
    let meaning = match item.verdict {
        Verdict::Unknown => return String::new(),
        Verdict::Rebuildable => {
            "supprimable : il se reconstruit tout seul (compilation, installation des dépendances), sans perte de travail"
        }
        Verdict::RebuildOnline => {
            "supprimable avec le réseau : il se retélécharge à la prochaine installation"
        }
        Verdict::ToReview => "à juger : c'est au développeur de décider",
        Verdict::Protected => "protégé : Delest n'y touche jamais",
    };
    format!(
        "\nce que Delest sait déjà : {} ; verdict : {meaning}",
        crate::report::kind(item, Lang::Fr)
    )
}

/// The local model's answer to a free question, worded by `question_prompt`; it changes nothing.
pub fn ask(prompt: &str, setup: &Setup) -> io::Result<(String, f32)> {
    let mut engine = Engine::start(&setup.program, &setup.reasoning, Mode::Chat, setup.threads)?;
    let started = Instant::now();
    let answer = engine.chat(prompt, None, 220)?;
    Ok((answer.trim().to_string(), started.elapsed().as_secs_f32()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verdict::{Category, Reason};

    fn unknown(path: &Path, size: u64) -> Assessment {
        Assessment {
            path: path.to_path_buf(),
            verdict: Verdict::Unknown,
            category: Category::Unknown,
            rule: None,
            reasons: vec![Reason::OutsideGit, Reason::NotAnArtifact],
            size: Some(size),
            idle_days: None,
            shadow: None,
        }
    }

    #[test]
    fn a_free_question_gets_what_delest_knows_but_not_for_an_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let mut item = unknown(&dir.path().join("target"), 1_000_000);
        assert!(!question_prompt(&item, "?", Lang::Fr).contains("\nce que Delest sait déjà :"));
        item.verdict = Verdict::Rebuildable;
        let prompt = question_prompt(&item, "?", Lang::Fr);
        assert!(prompt.contains("\nce que Delest sait déjà :"));
        assert!(prompt.contains("se reconstruit tout seul"));
        assert!(prompt.contains("Réponds en français"));
        assert!(question_prompt(&item, "?", Lang::En).contains("Réponds en anglais"));
    }

    #[test]
    fn the_description_holds_names_and_facts_only() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        fs::create_dir_all(game.join("ProjectSettings")).unwrap();
        fs::create_dir_all(game.join("Library/PackageCache")).unwrap();
        fs::write(game.join("Library/ArtifactDB"), "secret-ish content").unwrap();
        let text = describe(&unknown(&game.join("Library"), 5_000_000_000));
        assert!(text.contains("nom : Library"));
        assert!(text.contains("marqueurs dans le dossier parent : ProjectSettings"));
        assert!(text.contains("ArtifactDB, PackageCache"));
        assert!(text.contains("taille en Mo : 5000"));
        assert!(!text.contains("secret-ish"), "no file content is read");
    }

    #[test]
    fn an_opinion_holds_until_the_folder_changes_size() {
        let path = PathBuf::from("/p/Library");
        let mut opinions = Opinions::default();
        opinions.folders.insert(
            path.clone(),
            Opinion {
                path: path.clone(),
                nature: "cache".into(),
                advice: Advice::Review,
                reason: "r".into(),
                neighbours: Vec::new(),
                model: "m".into(),
                at: 0,
                size: 1000,
                seconds: 1.0,
            },
        );
        assert!(opinions.current(&unknown(&path, 1050)).is_some());
        assert!(opinions.current(&unknown(&path, 2000)).is_none());
    }

    #[test]
    fn the_grammar_offers_no_freeable_answer() {
        assert!(GRAMMAR.contains("a_garder") && GRAMMAR.contains("a_juger"));
        assert!(!GRAMMAR.contains("supprimable"));
    }
}
