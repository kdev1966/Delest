//! The Arbiter decides in a fixed order and keeps the trace: absolute bans first
//! (paths the developer protected, work git knows, secrets, data and source code),
//! then the known rules, then the habits learned from the developer's decisions.
//! Habits only ever make an item more cautious; nothing after a ban can make it
//! freeable.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::activity::{self, Handoff};
use crate::git::{self, Status};
use crate::memory::{self, Memory};
use crate::rules::{self, CACHES, CacheRule, Kind, Rule};
use crate::verdict::{Assessment, Category, Reason, Verdict};

/// How deep a build folder is searched for secrets, data and source code.
const SENSITIVE_DEPTH: usize = 3;

pub struct Arbiter {
    home: PathBuf,
    handoff: Handoff,
    memory: Memory,
}

impl Arbiter {
    /// Rules and bans only, as the acceptance corpus replays them.
    pub fn new(home: PathBuf) -> Self {
        Arbiter::with_context(home, Handoff::default(), Memory::default())
    }

    /// With the developer's decisions and the projects' activity.
    pub fn with_context(home: PathBuf, handoff: Handoff, memory: Memory) -> Self {
        Arbiter {
            home,
            handoff,
            memory,
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The global cache at exactly this path, if any.
    pub fn cache_at(&self, path: &Path) -> Option<&'static CacheRule> {
        CACHES
            .iter()
            .find(|cache| self.home.join(cache.path) == path)
    }

    /// Whether the scan should stop at this folder and assess it.
    pub fn recognises(&self, path: &Path) -> bool {
        self.cache_at(path).is_some() || name_of(path).and_then(rules::rule_for).is_some()
    }

    pub fn assess(&self, path: &Path) -> Assessment {
        let cache = self.cache_at(path);
        let rule = name_of(path)
            .and_then(rules::rule_for)
            .filter(|_| path.is_dir());
        let (category, rule_name) = match (cache, rule) {
            (Some(cache), _) => (cache.category, cache.path),
            (None, Some(rule)) => (rule.category, rule.name),
            (None, None) => {
                let mut reasons = Vec::new();
                let verdict = self.unknown(path, &mut reasons);
                return Assessment {
                    path: path.to_path_buf(),
                    verdict,
                    category: Category::Unknown,
                    rule: None,
                    reasons,
                    size: None,
                    idle_days: None,
                    shadow: None,
                };
            }
        };
        let mut reasons = Vec::new();
        let mut idle_days = None;
        let verdict = self.decide(path, cache, rule, &mut reasons, &mut idle_days);
        let verdict = self.habits(path, rule_name, verdict, &mut reasons);
        // A rule in shadow concludes, but its items stay unknown until accepted.
        let in_shadow =
            rule.is_some_and(|rule| rule.shadow && !self.memory.accepted_rules.contains(rule.name));
        let (verdict, shadow) = if in_shadow && verdict != Verdict::Protected {
            reasons.push(Reason::InShadow {
                rule: rule_name.to_string(),
            });
            (Verdict::Unknown, Some(verdict))
        } else {
            (verdict, None)
        };
        Assessment {
            path: path.to_path_buf(),
            verdict,
            category,
            rule: Some(rule_name.to_string()),
            reasons,
            size: None,
            idle_days,
            shadow,
        }
    }

    fn decide(
        &self,
        path: &Path,
        cache: Option<&CacheRule>,
        rule: Option<&Rule>,
        reasons: &mut Vec<Reason>,
        idle_days: &mut Option<u64>,
    ) -> Verdict {
        if let Some(protected) = self.memory.protection(path) {
            let path = crate::tilde(protected, &self.home);
            reasons.push(Reason::ProtectedByYou { path });
            return Verdict::Protected;
        }
        let git = git::state(path);
        // Absolute bans: work that git tracks or would show as new.
        match git.status {
            Status::Tracked { files } => {
                reasons.push(Reason::GitTracked { files });
                return Verdict::Protected;
            }
            Status::Untracked => {
                reasons.push(Reason::GitUntracked);
                return Verdict::Protected;
            }
            Status::Unreadable => {
                reasons.push(Reason::GitUnreadable);
                return Verdict::Protected;
            }
            Status::Ignored => reasons.push(Reason::GitIgnored),
            Status::Outside => {}
        }
        let verdict = match (cache, rule) {
            (Some(cache), _) => {
                reasons.push(Reason::KnownCache { note: cache.note });
                cache.verdict
            }
            (None, Some(rule)) => {
                if matches!(rule.kind, Kind::Build | Kind::Bytecode | Kind::ToolCache)
                    && let Some(file) = first_inside(path, |name| {
                        rules::is_sensitive(name) && !rule.caches.contains(&name)
                    })
                {
                    reasons.push(Reason::SensitiveFile { file });
                    return Verdict::Protected;
                }
                if git.status == Status::Outside && cache.is_none() {
                    reasons.push(Reason::OutsideGit);
                }
                apply_rule(path, rule, git.root.as_deref(), reasons)
            }
            (None, None) => unreachable!("assess() returns early for unknown items"),
        };
        let dependencies = rule.is_some_and(|rule| matches!(rule.kind, Kind::Dependencies { .. }));
        if dependencies {
            let project = git.root.as_deref().or(path.parent()).unwrap_or(path);
            if let Some((days, source)) =
                activity::idle_days(project, git.last_commit_days, &self.handoff)
            {
                *idle_days = Some(days);
                reasons.push(Reason::Activity { days, source });
            }
        } else if let Some(days) = git.last_commit_days {
            reasons.push(Reason::LastCommit { days });
        }
        verdict
    }

    /// A folder no rule recognises: the same bans as any other, so that the model's
    /// opinion, later, can never be heard on protected work.
    fn unknown(&self, path: &Path, reasons: &mut Vec<Reason>) -> Verdict {
        if !path.is_dir() {
            reasons.push(Reason::NotAnArtifact);
            return Verdict::Unknown;
        }
        if let Some(protected) = self.memory.protection(path) {
            let path = crate::tilde(protected, &self.home);
            reasons.push(Reason::ProtectedByYou { path });
            return Verdict::Protected;
        }
        match git::state(path).status {
            Status::Tracked { files } => {
                reasons.push(Reason::GitTracked { files });
                return Verdict::Protected;
            }
            Status::Untracked => {
                reasons.push(Reason::GitUntracked);
                return Verdict::Protected;
            }
            Status::Unreadable => {
                reasons.push(Reason::GitUnreadable);
                return Verdict::Protected;
            }
            Status::Ignored => reasons.push(Reason::GitIgnored),
            Status::Outside => reasons.push(Reason::OutsideGit),
        }
        if let Some(marker) = rules::project_part(path) {
            reasons.push(Reason::ProjectPart {
                marker: marker.to_string(),
            });
            return Verdict::Protected;
        }
        if let Some(file) = first_inside(path, rules::is_sensitive) {
            reasons.push(Reason::SensitiveFile { file });
            return Verdict::Protected;
        }
        if let Some(file) = first_inside(path, rules::is_source) {
            reasons.push(Reason::SourceCode { file });
            return Verdict::Protected;
        }
        if let Some(file) = first_inside(path, rules::is_model) {
            reasons.push(Reason::ModelFiles { file });
            return Verdict::Protected;
        }
        reasons.push(Reason::NotAnArtifact);
        Verdict::Unknown
    }

    /// What the developer's past decisions change: only towards caution.
    fn habits(
        &self,
        path: &Path,
        rule: &str,
        verdict: Verdict,
        reasons: &mut Vec<Reason>,
    ) -> Verdict {
        if matches!(verdict, Verdict::Protected | Verdict::Unknown) {
            return verdict;
        }
        let times = self.memory.refusals_of(path);
        if times >= memory::REFUSALS_TO_STOP {
            reasons.push(Reason::RefusedByYou { times });
            return Verdict::Protected;
        }
        if !verdict.is_freeable() {
            return verdict;
        }
        let now = now();
        if let Some(days) = self.memory.restored_days_ago(path, now) {
            reasons.push(Reason::RestoredRecently { days });
            return Verdict::ToReview;
        }
        let count = self.memory.restores_of_rule(rule, now);
        if count >= memory::RESTORES_TO_DOUBT_A_RULE {
            reasons.push(Reason::RuleRestored { count });
            return Verdict::ToReview;
        }
        verdict
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn apply_rule(path: &Path, rule: &Rule, root: Option<&Path>, reasons: &mut Vec<Reason>) -> Verdict {
    let parent = path.parent().unwrap_or(path);
    let present: Vec<_> = rule
        .manifests
        .iter()
        .filter(|manifest| parent.join(manifest.file).exists())
        .collect();
    match &rule.kind {
        Kind::Bytecode => {
            reasons.push(Reason::Bytecode);
            Verdict::Rebuildable
        }
        Kind::ToolCache => {
            reasons.push(Reason::ToolCache);
            Verdict::Rebuildable
        }
        Kind::Build => {
            let Some(first) = present.first() else {
                if let Some(file) = first_inside(path, rules::is_source) {
                    reasons.push(Reason::SourceCode { file });
                    return Verdict::Protected;
                }
                reasons.push(Reason::NoManifest);
                return Verdict::ToReview;
            };
            if root.is_some() {
                reasons.push(Reason::Manifest {
                    file: first.file.into(),
                });
                return Verdict::Rebuildable;
            }
            match present.iter().find(|manifest| manifest.conclusive) {
                Some(manifest) => {
                    reasons.push(Reason::Manifest {
                        file: manifest.file.into(),
                    });
                    Verdict::Rebuildable
                }
                None => {
                    reasons.push(Reason::WeakManifestOutsideGit {
                        file: first.file.into(),
                    });
                    Verdict::ToReview
                }
            }
        }
        Kind::Dependencies {
            lockfiles,
            partial,
            inside,
        } => {
            if let Some(file) = inside
                && !path.join(file).exists()
            {
                reasons.push(Reason::NotAVirtualEnv);
                return Verdict::ToReview;
            }
            if !rule.manifests.is_empty() && present.is_empty() {
                reasons.push(Reason::NoManifest);
                return Verdict::ToReview;
            }
            // A workspace keeps its lockfile at its root: look up to the working tree,
            // and only next to the folder outside git.
            for (up, dir) in parent.ancestors().enumerate() {
                let name = |file: &str| format!("{}{file}", "../".repeat(up));
                if let Some(file) = lockfiles.iter().find(|file| dir.join(file).is_file()) {
                    reasons.push(Reason::Lockfile { file: name(file) });
                    return Verdict::RebuildOnline;
                }
                if let Some(file) = partial.iter().find(|file| dir.join(file).is_file()) {
                    reasons.push(Reason::PartialLockfile { file: name(file) });
                    return Verdict::ToReview;
                }
                if root.is_none_or(|root| dir == root) {
                    break;
                }
            }
            reasons.push(Reason::NoLockfile);
            Verdict::ToReview
        }
    }
}

/// The first file near the top of the folder whose name matches.
fn first_inside(path: &Path, matches: impl Fn(&str) -> bool) -> Option<String> {
    let mut pending = vec![(path.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            if kind.is_dir() && depth + 1 < SENSITIVE_DEPTH {
                pending.push((entry.path(), depth + 1));
            } else if kind.is_file() && name.to_str().is_some_and(&matches) {
                let relative = entry.path();
                let relative = relative.strip_prefix(path).unwrap_or(&relative);
                return Some(relative.display().to_string());
            }
        }
    }
    None
}

fn name_of(path: &Path) -> Option<&str> {
    path.file_name().and_then(OsStr::to_str)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{commit_all, native, repository};

    fn arbiter() -> Arbiter {
        Arbiter::new(PathBuf::from("/nonexistent-home"))
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn an_ignored_build_folder_next_to_its_manifest_is_rebuildable() {
        let repo = repository();
        let root = repo.path();
        touch(&root.join(".gitignore"));
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        touch(&root.join("Cargo.toml"));
        touch(&root.join("target/debug/app"));
        commit_all(root);
        let assessment = arbiter().assess(&root.join("target"));
        assert_eq!(assessment.verdict, Verdict::Rebuildable);
        assert!(assessment.reasons.contains(&Reason::GitIgnored));
    }

    #[test]
    fn a_tracked_folder_with_an_artefact_name_is_protected() {
        let repo = repository();
        let root = repo.path();
        touch(&root.join("package.json"));
        touch(&root.join("build/icon.png"));
        commit_all(root);
        assert_eq!(
            arbiter().assess(&root.join("build")).verdict,
            Verdict::Protected
        );
    }

    #[test]
    fn a_folder_git_would_show_as_new_work_is_protected() {
        let repo = repository();
        let root = repo.path();
        touch(&root.join("app.py"));
        touch(&root.join("__pycache__/app.cpython-314.pyc"));
        assert_eq!(
            arbiter().assess(&root.join("__pycache__")).verdict,
            Verdict::Protected
        );
    }

    #[test]
    fn a_secret_inside_a_build_folder_is_protected() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("pubspec.yaml"));
        touch(&root.join("build/app/.env"));
        let assessment = arbiter().assess(&root.join("build"));
        assert_eq!(assessment.verdict, Verdict::Protected);
        assert_eq!(
            assessment.reasons,
            vec![Reason::SensitiveFile {
                file: native("app/.env")
            }]
        );
    }

    #[test]
    fn a_build_folder_without_manifest_that_holds_source_code_is_protected() {
        let repo = repository();
        let root = repo.path();
        fs::write(root.join(".gitignore"), "managed_components/\n").unwrap();
        touch(&root.join("managed_components/camera/target/esp32/ll_cam.c"));
        commit_all(root);
        let assessment = arbiter().assess(&root.join("managed_components/camera/target"));
        assert_eq!(assessment.verdict, Verdict::Protected);
        assert!(assessment.reasons.contains(&Reason::SourceCode {
            file: native("esp32/ll_cam.c")
        }));
    }

    #[test]
    fn outside_git_only_a_conclusive_manifest_makes_a_build_folder_rebuildable() {
        let flutter = tempfile::tempdir().unwrap();
        touch(&flutter.path().join("pubspec.yaml"));
        touch(&flutter.path().join("build/app.apk"));
        assert_eq!(
            arbiter().assess(&flutter.path().join("build")).verdict,
            Verdict::Rebuildable
        );

        let node = tempfile::tempdir().unwrap();
        touch(&node.path().join("package.json"));
        touch(&node.path().join("dist/main.js"));
        assert_eq!(
            arbiter().assess(&node.path().join("dist")).verdict,
            Verdict::ToReview
        );
    }

    #[test]
    fn dependencies_need_a_lockfile_found_up_to_the_working_tree() {
        let repo = repository();
        let root = repo.path();
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        touch(&root.join("pnpm-lock.yaml"));
        touch(&root.join("apps/web/package.json"));
        touch(&root.join("apps/web/node_modules/react/index.js"));
        commit_all(root);
        let assessment = arbiter().assess(&root.join("apps/web/node_modules"));
        assert_eq!(assessment.verdict, Verdict::RebuildOnline);
        assert!(assessment.reasons.contains(&Reason::Lockfile {
            file: "../../pnpm-lock.yaml".into()
        }));
    }

    #[test]
    fn a_virtual_env_with_requirements_only_is_to_review() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("requirements.txt"));
        touch(&root.join(".venv/pyvenv.cfg"));
        let assessment = arbiter().assess(&root.join(".venv"));
        assert_eq!(assessment.verdict, Verdict::ToReview);
        assert!(assessment.reasons.contains(&Reason::PartialLockfile {
            file: "requirements.txt".into()
        }));

        touch(&root.join("other/venv/lib/x.py"));
        assert_eq!(
            arbiter().assess(&root.join("other/venv")).reasons.last(),
            Some(&Reason::NotAVirtualEnv)
        );
    }

    #[test]
    fn global_caches_are_recognised_by_their_exact_path() {
        let home = tempfile::tempdir().unwrap();
        touch(&home.path().join(".npm/_cacache/index"));
        touch(&home.path().join(".npm/other/file"));
        let arbiter = Arbiter::new(home.path().to_path_buf());
        assert_eq!(
            arbiter.assess(&home.path().join(".npm")).verdict,
            Verdict::RebuildOnline
        );
        assert_eq!(
            arbiter.assess(&home.path().join(".npm/other")).verdict,
            Verdict::Unknown
        );
    }

    #[test]
    fn the_developer_s_decisions_only_make_items_more_cautious() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("pubspec.yaml"));
        touch(&root.join("build/app.apk"));
        let build = root.join("build");
        let with = |memory: Memory| {
            Arbiter::with_context(
                PathBuf::from("/nonexistent-home"),
                Handoff::default(),
                memory,
            )
        };

        let mut memory = Memory::default();
        memory.protected.insert(root.to_path_buf());
        assert_eq!(with(memory).assess(&build).verdict, Verdict::Protected);

        let mut memory = Memory::default();
        memory.record_restore(&build, Some("build"), now());
        assert_eq!(with(memory).assess(&build).verdict, Verdict::ToReview);

        let mut memory = Memory::default();
        for _ in 0..memory::REFUSALS_TO_STOP {
            memory.refuse(&build);
        }
        let assessment = with(memory).assess(&build);
        assert_eq!(assessment.verdict, Verdict::Protected);
        assert_eq!(
            assessment.reasons.last(),
            Some(&Reason::RefusedByYou { times: 3 })
        );

        // A habit never lifts a ban: a tracked folder stays protected.
        let repo = repository();
        touch(&repo.path().join("package.json"));
        touch(&repo.path().join("dist/main.js"));
        commit_all(repo.path());
        let mut memory = Memory::default();
        memory.record_restore(&repo.path().join("dist"), Some("dist"), now());
        assert!(matches!(
            with(memory).assess(&repo.path().join("dist")).reasons[0],
            Reason::GitTracked { .. }
        ));
    }

    #[test]
    fn dependencies_carry_the_idle_days_of_their_project() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("package.json"));
        touch(&root.join("package-lock.json"));
        touch(&root.join("node_modules/dep/index.js"));
        let assessment = arbiter().assess(&root.join("node_modules"));
        assert_eq!(assessment.idle_days, Some(0));
    }

    #[test]
    fn a_rule_in_shadow_is_recognised_but_never_offered_until_accepted() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("app.py"));
        touch(&root.join(".pytest_cache/v/cache/lastfailed"));
        let cache = root.join(".pytest_cache");
        let shadowed = arbiter().assess(&cache);
        assert_eq!(shadowed.verdict, Verdict::Unknown);
        assert_eq!(shadowed.shadow, Some(Verdict::Rebuildable));
        let mut memory = Memory::default();
        memory.accepted_rules.insert(".pytest_cache".into());
        let accepted = Arbiter::with_context(
            PathBuf::from("/nonexistent-home"),
            Handoff::default(),
            memory,
        )
        .assess(&cache);
        assert_eq!(
            (accepted.verdict, accepted.shadow),
            (Verdict::Rebuildable, None)
        );
    }

    #[test]
    fn an_unknown_folder_passes_the_same_bans() {
        let repo = repository();
        let root = repo.path();
        touch(&root.join("packages/core/lib.py"));
        commit_all(root);
        assert_eq!(
            arbiter().assess(&root.join("packages")).verdict,
            Verdict::Protected
        );

        let outside = tempfile::tempdir().unwrap();
        touch(&outside.path().join("models/model.gguf"));
        touch(&outside.path().join("vendored/src/main.c"));
        assert_eq!(
            arbiter().assess(&outside.path().join("models")).verdict,
            Verdict::Protected
        );
        assert_eq!(
            arbiter().assess(&outside.path().join("vendored")).verdict,
            Verdict::Protected
        );
    }

    #[test]
    fn unity_s_library_is_a_cache_its_own_databases_included() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("ProjectSettings/ProjectVersion.txt"));
        touch(&root.join("Library/ShaderCache.db"));
        touch(&root.join("Library/Search/propertyDatabase.db"));
        let library = arbiter().assess(&root.join("Library"));
        assert_eq!(
            (library.verdict, library.shadow),
            (Verdict::Unknown, Some(Verdict::Rebuildable))
        );
        // Any other database inside still protects the folder.
        touch(&root.join("Library/players.db"));
        assert_eq!(
            arbiter().assess(&root.join("Library")).verdict,
            Verdict::Protected
        );
    }

    #[test]
    fn esp_idf_components_come_back_from_their_lockfile() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("dependencies.lock"));
        touch(&root.join("managed_components/espressif__esp-sr/src/sr.c"));
        let components = arbiter().assess(&root.join("managed_components"));
        assert_eq!(
            (components.verdict, components.shadow),
            (Verdict::Unknown, Some(Verdict::RebuildOnline))
        );
    }

    #[test]
    fn unity_assets_and_tauri_binaries_are_never_left_to_the_model() {
        let outside = tempfile::tempdir().unwrap();
        let root = outside.path();
        touch(&root.join("game/ProjectSettings/ProjectVersion.txt"));
        touch(&root.join("game/Assets/Textures/grass.png"));
        touch(&root.join("app/src-tauri/tauri.conf.json"));
        touch(&root.join("app/src-tauri/binaries/infer-x86_64-apple-darwin"));
        touch(&root.join("loose/Assets/grass.png"));
        for (path, marker) in [
            ("game/Assets", "ProjectSettings"),
            ("app/src-tauri/binaries", "tauri.conf.json"),
        ] {
            let assessment = arbiter().assess(&root.join(path));
            assert_eq!(assessment.verdict, Verdict::Protected, "{path}");
            assert!(assessment.reasons.contains(&Reason::ProjectPart {
                marker: marker.into()
            }));
        }
        // Without its marker, an Assets folder is an unknown like any other.
        assert_eq!(
            arbiter().assess(&root.join("loose/Assets")).verdict,
            Verdict::Unknown
        );
    }

    #[test]
    fn anything_else_is_unknown() {
        let outside = tempfile::tempdir().unwrap();
        touch(&outside.path().join("notes/todo.md"));
        assert_eq!(
            arbiter().assess(&outside.path().join("notes")).verdict,
            Verdict::Unknown
        );
    }
}
