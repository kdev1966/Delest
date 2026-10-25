//! Answers already given to a free question, so that the same question about an
//! unchanged folder does not make a model think again. The key is the whole prompt
//! and who answered it: the prompt holds the folder's description, so a folder that
//! changed (its size, what it holds, its verdict) is a new question.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FILE: &str = "reponses.json";
/// An answer is given again for a month at most; the oldest go beyond this many.
const KEEP: usize = 200;
const FRESH: i64 = 30 * 86_400;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    pub text: String,
    /// How long the model took the first time.
    pub seconds: f32,
    pub at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Answers {
    #[serde(default)]
    entries: BTreeMap<String, Kept>,
}

fn key(by: &str, prompt: &str) -> String {
    let digest = Sha256::digest(format!("{by}\n{prompt}"));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Answers {
    /// A broken file only costs the answers kept: it holds no decision of the developer.
    pub fn load(data_dir: &Path) -> Answers {
        fs::read_to_string(data_dir.join(FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string(self).map_err(io::Error::other)?;
        let temporary = data_dir.join(format!("{FILE}.tmp"));
        fs::write(&temporary, json)?;
        fs::rename(temporary, data_dir.join(FILE))
    }

    pub fn get(&self, by: &str, prompt: &str, now: i64) -> Option<&Kept> {
        self.entries
            .get(&key(by, prompt))
            .filter(|kept| now - kept.at < FRESH)
    }

    pub fn put(&mut self, by: &str, prompt: &str, kept: Kept) {
        self.entries.insert(key(by, prompt), kept);
        while self.entries.len() > KEEP {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, kept)| kept.at)
                .map(|(key, _)| key.clone());
            match oldest {
                Some(oldest) => self.entries.remove(&oldest),
                None => break,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(at: i64) -> Kept {
        Kept {
            text: "answer".into(),
            seconds: 9.0,
            at,
        }
    }

    #[test]
    fn the_same_question_to_the_same_model_gets_its_answer_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut answers = Answers::load(dir.path());
        answers.put("Gemma 4 E2B", "prompt", kept(100));
        answers.save(dir.path()).unwrap();
        let answers = Answers::load(dir.path());
        assert_eq!(answers.get("Gemma 4 E2B", "prompt", 200), Some(&kept(100)));
        assert_eq!(answers.get("Claude Code", "prompt", 200), None);
        assert_eq!(
            answers.get("Gemma 4 E2B", "prompt, folder grown", 200),
            None
        );
        assert_eq!(answers.get("Gemma 4 E2B", "prompt", 100 + FRESH), None);
    }

    #[test]
    fn the_oldest_answers_go_first() {
        let mut answers = Answers::default();
        for at in 0..=KEEP as i64 {
            answers.put("model", &format!("prompt {at}"), kept(at));
        }
        assert_eq!(answers.entries.len(), KEEP);
        assert_eq!(answers.get("model", "prompt 0", 0), None);
        assert!(answers.get("model", "prompt 1", 1).is_some());
    }
}
