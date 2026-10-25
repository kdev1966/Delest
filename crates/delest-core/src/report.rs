//! What the interface shows: the items of the last scan grouped by project, with
//! what each one consists of, and the overall figures.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::lang::Lang;
use crate::tr;
use crate::verdict::{Assessment, Category, Reason, Verdict};

/// The project an item belongs to: its outermost git working tree among the
/// items' own trees, or the folder that holds it outside git.
pub fn project_of(path: &Path) -> PathBuf {
    match crate::git::working_tree(path) {
        Some(root) => root,
        None => path.parent().unwrap_or(path).to_path_buf(),
    }
}

/// What a folder is, in words the developer recognises: "Flutter build",
/// "Next.js build", "npm dependencies".
pub fn kind(item: &Assessment, lang: Lang) -> String {
    let next_to = |file: &str| {
        item.reasons
            .iter()
            .any(|reason| matches!(reason, Reason::Manifest { file: f } if f == file))
    };
    match item.rule.as_deref().unwrap_or_default() {
        "target" if next_to("pom.xml") => tr!(lang, "Maven build", "compilation Maven"),
        "target" => tr!(lang, "Rust build", "compilation Rust"),
        "build" if next_to("pubspec.yaml") => tr!(lang, "Flutter build", "build Flutter"),
        "build" if next_to("build.gradle") || next_to("build.gradle.kts") => {
            tr!(lang, "Android build (Gradle)", "build Android (Gradle)")
        }
        "build" if next_to("ProjectSettings") => tr!(lang, "Unity build", "build Unity"),
        "build" if next_to("CMakeLists.txt") => tr!(lang, "CMake build", "build CMake"),
        "build" => tr!(lang, "build", "build"),
        ".next" => tr!(lang, "Next.js build", "build Next.js"),
        ".dart_tool" => tr!(lang, "Dart tools", "outils Dart"),
        ".gradle" => tr!(lang, "Gradle cache", "cache Gradle"),
        ".turbo" => tr!(lang, "Turborepo cache", "cache Turborepo"),
        "dist" => tr!(lang, "generated files", "fichiers générés"),
        "__pycache__" => tr!(lang, "Python bytecode", "bytecode Python"),
        "node_modules" => tr!(lang, "npm dependencies", "dépendances npm"),
        ".venv" | "venv" => tr!(lang, "Python environment", "environnement Python"),
        "Pods" => tr!(lang, "CocoaPods dependencies", "dépendances CocoaPods"),
        "Library" => tr!(lang, "Unity cache", "cache Unity"),
        "managed_components" => tr!(lang, "ESP-IDF components", "composants ESP-IDF"),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub path: PathBuf,
    pub kind: String,
    pub verdict: Verdict,
    pub category: Category,
    pub size: u64,
    pub trace: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub root: PathBuf,
    pub size: u64,
    pub lines: Vec<Line>,
}

/// The offered items (freeable, freeable online, to review) still on disk,
/// grouped by project, the largest project first.
pub fn projects(items: &[Assessment], lang: Lang) -> Vec<Project> {
    let offered: Vec<&Assessment> = items
        .iter()
        .filter(|item| {
            matches!(
                item.verdict,
                Verdict::Rebuildable | Verdict::RebuildOnline | Verdict::ToReview
            )
        })
        .filter(|item| item.path.exists())
        .collect();
    let roots: Vec<PathBuf> = offered.iter().map(|item| project_of(&item.path)).collect();
    // A project folder inside another one (a Unity build tree, a monorepo app outside
    // git) is shown with the outer one.
    let outer = |root: &PathBuf| {
        roots
            .iter()
            .filter(|other| root.starts_with(other))
            .min_by_key(|other| other.components().count())
            .cloned()
            .unwrap_or_else(|| root.clone())
    };
    let mut grouped: BTreeMap<PathBuf, Vec<Line>> = BTreeMap::new();
    for (item, root) in offered.iter().zip(&roots) {
        grouped.entry(outer(root)).or_default().push(Line {
            path: item.path.clone(),
            kind: kind(item, lang),
            verdict: item.verdict,
            category: item.category,
            size: item.size.unwrap_or(0),
            trace: item.trace(lang),
        });
    }
    let mut projects: Vec<Project> = grouped
        .into_iter()
        .map(|(root, mut lines)| {
            lines.sort_by_key(|line| std::cmp::Reverse(line.size));
            Project {
                size: lines.iter().map(|line| line.size).sum(),
                root,
                lines,
            }
        })
        .collect();
    projects.sort_by_key(|project| std::cmp::Reverse(project.size));
    projects
}

/// Bytes per category among the offered items.
pub fn by_category(projects: &[Project]) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for line in projects.iter().flat_map(|project| &project.lines) {
        let key = serde_json::to_value(line.category)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        *totals.entry(key).or_insert(0) += line.size;
    }
    totals
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn item(path: &Path, rule: &str, manifest: &str, verdict: Verdict, size: u64) -> Assessment {
        Assessment {
            path: path.to_path_buf(),
            verdict,
            category: Category::Build,
            rule: Some(rule.into()),
            reasons: vec![Reason::Manifest {
                file: manifest.into(),
            }],
            size: Some(size),
            idle_days: None,
            shadow: None,
        }
    }

    #[test]
    fn items_are_grouped_by_project_and_named_by_what_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        for path in [
            game.join("build"),
            game.join("Library/Bee/Gradle/build"),
            dir.path().join("app/build"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        let items = vec![
            item(
                &game.join("build"),
                "build",
                "ProjectSettings",
                Verdict::Rebuildable,
                600,
            ),
            item(
                &game.join("Library/Bee/Gradle/build"),
                "build",
                "build.gradle",
                Verdict::Rebuildable,
                300,
            ),
            item(
                &dir.path().join("app/build"),
                "build",
                "pubspec.yaml",
                Verdict::Rebuildable,
                100,
            ),
            item(
                &dir.path().join("gone/target"),
                "target",
                "Cargo.toml",
                Verdict::Rebuildable,
                5000,
            ),
            item(
                &game.join("src-protected"),
                "build",
                "x",
                Verdict::Protected,
                9,
            ),
        ];
        let projects = projects(&items, Lang::Fr);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].root, game);
        assert_eq!(projects[0].size, 900);
        assert_eq!(projects[0].lines[0].kind, "build Unity");
        assert_eq!(projects[0].lines[1].kind, "build Android (Gradle)");
        assert_eq!(projects[1].lines[0].kind, "build Flutter");
        assert_eq!(by_category(&projects).get("build"), Some(&1000));
    }
}
