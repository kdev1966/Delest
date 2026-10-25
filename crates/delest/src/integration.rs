//! `install`, `remove` and `hook`: Delest in the agents' sessions.

use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use delest_core::agents::{self, AGENTS, Agent, Change, State};
use delest_core::alerts::{self, Shown};
use delest_core::arbiter::Arbiter;
use delest_core::index::Index;
use delest_core::lang::Lang;
use delest_core::timer;
use delest_core::volume::{self, Level};
use delest_core::{paths, tilde, tr};

use crate::actions::{confirm, now};

/// An index older than this is refreshed in the background by the hook.
const REFRESH_AFTER: i64 = 86_400;
const LOG: &str = "hooks.log";
const LOG_LIMIT: u64 = 1024 * 1024;

/// The installed `delest`, refused when it sits in a build folder that a clean
/// would remove.
pub fn installed_exe(lang: Lang) -> io::Result<PathBuf> {
    let exe = std::env::current_exe().and_then(|exe| paths::canonical(&exe))?;
    if exe
        .components()
        .any(|part| part == Component::Normal("target".as_ref()))
    {
        let path = exe.display();
        return Err(io::Error::other(tr!(
            lang,
            "{path} is a build output: a clean would remove it. Install Delest first: cargo install --path crates/delest",
            "{path} est un produit de compilation : un nettoyage le supprimerait. Installez d'abord Delest : cargo install --path crates/delest"
        )));
    }
    Ok(exe)
}

fn backups_dir() -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    paths::data_dir().join("backups").join(stamp)
}

fn agents_present(home: &Path) -> Vec<Agent> {
    AGENTS
        .into_iter()
        .filter(|agent| agent.detected(home))
        .collect()
}

fn apply_changes(
    changes: Vec<(Agent, Change)>,
    verb: &str,
    dry_run: bool,
    yes: bool,
    lang: Lang,
    home: &Path,
) -> io::Result<ExitCode> {
    let pending: Vec<_> = changes
        .into_iter()
        .filter(|(_, change)| change.changes_anything())
        .collect();
    if pending.is_empty() {
        println!("{}", tr!(lang, "Nothing to change.", "Rien à changer."));
        return Ok(ExitCode::SUCCESS);
    }
    for (agent, change) in &pending {
        let (name, file) = (agent.name(), tilde(&change.path, home));
        println!(
            "{}",
            tr!(
                lang,
                "{name}: {verb} in {file}",
                "{name} : {verb} dans {file}"
            )
        );
    }
    if dry_run {
        println!(
            "{}",
            tr!(
                lang,
                "Preview only: nothing was changed.",
                "Aperçu seulement : rien n'a été modifié."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !yes {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other(tr!(
                lang,
                "no terminal to confirm: add --yes.",
                "pas de terminal pour confirmer : ajoutez --yes."
            )));
        }
        if !confirm(&tr!(lang, "Apply?", "Appliquer ?"), lang)? {
            println!(
                "{}",
                tr!(lang, "Nothing was changed.", "Rien n'a été modifié.")
            );
            return Ok(ExitCode::SUCCESS);
        }
    }
    let backups = backups_dir();
    for (_, change) in &pending {
        change.apply(&backups).map_err(io::Error::other)?;
    }
    let saved = tilde(&backups, home);
    println!(
        "{}",
        tr!(
            lang,
            "Done. The previous files are kept in {saved}.",
            "Fait. Les fichiers d'avant sont gardés dans {saved}."
        )
    );
    Ok(ExitCode::SUCCESS)
}

pub fn install(arbiter: &Arbiter, dry_run: bool, yes: bool, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let exe = installed_exe(lang)?;
    let agents = agents_present(home);
    if agents.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "No supported agent found (Claude Code, Codex, Cursor, Gemini CLI, VS Code, OpenCode).",
                "Aucun agent pris en charge trouvé (Claude Code, Codex, Cursor, Gemini CLI, VS Code, OpenCode)."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut changes = Vec::new();
    for agent in agents {
        let command = agents::hook_command(&exe, agent);
        let change = agents::plan_install(agent, &agent.settings(home), &command)
            .map_err(io::Error::other)?;
        changes.push((agent, change));
    }
    let pending: Vec<Agent> = changes
        .iter()
        .filter(|(_, change)| change.changes_anything())
        .map(|(agent, _)| *agent)
        .collect();
    let verb = tr!(
        lang,
        "a hook at the start of each session",
        "un hook au début de chaque session"
    );
    let code = apply_changes(changes, &verb, dry_run, yes, lang, home)?;
    if pending.contains(&Agent::Codex) {
        println!(
            "{}",
            tr!(
                lang,
                "Codex runs a new hook only once approved: type /hooks in a Codex session.",
                "Codex n'exécute un nouveau hook qu'une fois approuvé : tapez /hooks dans une session Codex."
            )
        );
    }
    if pending.contains(&Agent::VsCode) {
        println!(
            "{}",
            tr!(
                lang,
                "VS Code runs hooks when the chat.useHooks setting is on.",
                "VS Code exécute les hooks quand le réglage chat.useHooks est activé."
            )
        );
    }
    Ok(code)
}

pub fn remove(arbiter: &Arbiter, dry_run: bool, yes: bool, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let mut changes = Vec::new();
    for agent in agents_present(home) {
        let change = agents::plan_remove(agent, &agent.settings(home)).map_err(io::Error::other)?;
        changes.push((agent, change));
    }
    let verb = tr!(lang, "Delest's hook removed", "hook de Delest retiré");
    apply_changes(changes, &verb, dry_run, yes, lang, home)
}

/// `delest timer on|off|status`: the optional check every four hours, off by default.
pub fn timer_command(arbiter: &Arbiter, action: &str, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    if !timer::supported() {
        return Err(io::Error::other(tr!(
            lang,
            "this system has no timer Delest can use (systemd is needed on Linux).",
            "ce système n'a pas de minuterie utilisable par Delest (il faut systemd sous Linux)."
        )));
    }
    let hours = timer::INTERVAL_SECONDS / 3600;
    match action {
        "on" => {
            let exe = installed_exe(lang)?;
            timer::enable(home, &exe)?;
            let file = timer::place(home, lang);
            println!(
                "{}",
                tr!(
                    lang,
                    "Timer on: Delest checks the disk every {hours} hours ({file}).",
                    "Minuterie activée : Delest vérifie le disque toutes les {hours} heures ({file})."
                )
            );
        }
        "off" => {
            timer::disable(home)?;
            println!("{}", tr!(lang, "Timer off.", "Minuterie désactivée."));
        }
        _ => {
            if timer::enabled(home) {
                println!(
                    "{}",
                    tr!(
                        lang,
                        "Timer on: a check every {hours} hours.",
                        "Minuterie activée : un contrôle toutes les {hours} heures."
                    )
                );
            } else {
                println!(
                    "{}",
                    tr!(
                        lang,
                        "Timer off (delest timer on to turn it on).",
                        "Minuterie désactivée (delest timer on pour l'activer)."
                    )
                );
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `delest shadow`: what the rules in shadow would offer, from the last scan;
/// `delest shadow accept NAME`: take a rule out of the shadow once the corpus passes with it.
pub fn shadow(
    arbiter: &Arbiter,
    accept: Option<String>,
    corpus: &Path,
    lang: Lang,
) -> io::Result<ExitCode> {
    use delest_core::acceptance;
    use delest_core::activity::Handoff;
    use delest_core::memory::Memory;
    use delest_core::rules::RULES;

    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let mut memory = Memory::load(&data_dir)?;
    let pending: Vec<&str> = RULES
        .iter()
        .filter(|rule| rule.shadow && !memory.accepted_rules.contains(rule.name))
        .map(|rule| rule.name)
        .collect();

    let Some(name) = accept else {
        if pending.is_empty() {
            println!(
                "{}",
                tr!(lang, "No rule in shadow.", "Aucune règle en ombre.")
            );
            return Ok(ExitCode::SUCCESS);
        }
        let index = Index::load(&data_dir)?;
        println!(
            "{}",
            tr!(
                lang,
                "Rules in shadow, and what they would offer according to the last scan:",
                "Règles en ombre, et ce qu'elles proposeraient d'après le dernier scan :"
            )
        );
        for rule in &pending {
            let items: Vec<_> = index
                .iter()
                .flat_map(|index| &index.items)
                .filter(|item| {
                    item.rule.as_deref() == Some(*rule)
                        && item.shadow.is_some()
                        && item.path.exists()
                })
                .collect();
            let size =
                delest_core::lang::size(items.iter().filter_map(|item| item.size).sum(), lang);
            let count = items.len();
            println!(
                "{}",
                tr!(
                    lang,
                    "  {rule:15} {count} folders, {size}",
                    "  {rule:15} {count} dossiers, {size}"
                )
            );
        }
        println!(
            "{}",
            tr!(
                lang,
                "To accept one, after the corpus replay: delest shadow accept <rule>",
                "Pour en accepter une, après le rejeu du corpus : delest shadow accept <règle>"
            )
        );
        return Ok(ExitCode::SUCCESS);
    };

    if !pending.contains(&name.as_str()) {
        return Err(io::Error::other(tr!(
            lang,
            "{name} is not a rule in shadow (delest shadow lists them).",
            "{name} n'est pas une règle en ombre (delest shadow les liste)."
        )));
    }
    let text = fs::read_to_string(corpus).map_err(|error| {
        let file = corpus.display();
        io::Error::other(tr!(
            lang,
            "{file}: {error}; give the corpus with --corpus.",
            "{file} : {error} ; indiquez le corpus avec --corpus."
        ))
    })?;
    let mut trial = Memory::default();
    trial.accepted_rules.insert(name.clone());
    let judge = Arbiter::with_context(home.to_path_buf(), Handoff::default(), trial);
    let cases = acceptance::replay(&text, &judge, home).map_err(io::Error::other)?;
    let blocking: Vec<_> = cases.iter().filter(|case| case.is_blocking()).collect();
    if !blocking.is_empty() {
        let count = blocking.len();
        for case in &blocking {
            eprintln!("  {}", tilde(&case.path, home));
        }
        return Err(io::Error::other(tr!(
            lang,
            "with {name}, {count} protected items of the corpus come out freeable: the rule stays in shadow.",
            "avec {name}, {count} éléments protégés du corpus sortent supprimables : la règle reste en ombre."
        )));
    }
    memory.accepted_rules.insert(name.clone());
    memory.save(&data_dir)?;
    let checked = cases
        .iter()
        .filter(|case| case.assessment.is_some())
        .count();
    println!(
        "{}",
        tr!(
            lang,
            "Corpus passed ({checked} items). Rule {name} accepted: its folders can now be offered.",
            "Corpus passé ({checked} éléments). Règle {name} acceptée : ses dossiers peuvent maintenant être proposés."
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// Lines for `doctor`: whether each agent runs Delest's hook.
pub fn doctor_lines(home: &Path, lang: Lang) -> Vec<(bool, String)> {
    let mut lines = agent_lines(home, lang);
    match agents::kiro_hooked(home) {
        Some(true) => lines.push((
            true,
            tr!(
                lang,
                "Kiro: Delest checks the disk at each session (through Vario's agents)",
                "Kiro : Delest vérifie le disque à chaque session (par les agents de Vario)"
            ),
        )),
        Some(false) => lines.push((
            true,
            tr!(
                lang,
                "Kiro: no hook; Vario's agents carry it once Delest is installed (cargo install --path crates/delest, then vario install)",
                "Kiro : pas de hook ; les agents de Vario le portent une fois Delest installé (cargo install --path crates/delest, puis vario install)"
            ),
        )),
        None => {}
    }
    lines
}

fn agent_lines(home: &Path, lang: Lang) -> Vec<(bool, String)> {
    let exe = std::env::current_exe()
        .and_then(|exe| paths::canonical(&exe))
        .ok();
    agents_present(home)
        .into_iter()
        .map(|agent| {
            let name = agent.name();
            let command = exe
                .as_deref()
                .map(|exe| agents::hook_command(exe, agent))
                .unwrap_or_default();
            match agents::state(agent, &agent.settings(home), &command) {
                Ok(State::Installed) => (
                    true,
                    tr!(
                        lang,
                        "{name}: Delest checks the disk at each session",
                        "{name} : Delest vérifie le disque à chaque session"
                    ),
                ),
                Ok(State::Outdated) => (
                    false,
                    tr!(
                        lang,
                        "{name}: Delest's hook points to another program (delest install)",
                        "{name} : le hook de Delest pointe vers un autre programme (delest install)"
                    ),
                ),
                Ok(State::Absent) => (
                    true,
                    tr!(
                        lang,
                        "{name}: no hook (delest install to add it)",
                        "{name} : pas de hook (delest install pour l'ajouter)"
                    ),
                ),
                Err(error) => (false, format!("{name}: {error}")),
            }
        })
        .collect()
}

/// The hook: never fails and never blocks the agent. It reads `df` and the index,
/// shows an alert or a notice when one is due, and refreshes an old index in the
/// background. It answers each agent in the form that agent expects.
pub fn hook(arbiter: &Arbiter, via: Option<String>, lang: Lang) -> ExitCode {
    // Agents pipe their event in; the timer runs with a console for its input (the
    // Task Scheduler through conhost), which never ends: that one is not read.
    let mut input = String::new();
    if !io::stdin().is_terminal() {
        let _ = io::stdin().take(4 * 1024 * 1024).read_to_string(&mut input);
    }
    let agent = via.as_deref().and_then(Agent::from_id);
    // Cursor also runs Claude Code's hooks: when Delest's own Cursor hook is there,
    // the Claude Code copy stands aside, so the check does not run twice.
    let in_cursor =
        std::env::var_os("CURSOR_VERSION").is_some() || input.contains("\"cursor_version\"");
    if agent == Some(Agent::ClaudeCode)
        && in_cursor
        && agents::hooked(Agent::Cursor, arbiter.home())
    {
        return ExitCode::SUCCESS;
    }
    let data_dir = paths::data_dir();
    let alert = run_hook(arbiter, &data_dir, lang).unwrap_or_else(|error| {
        log(&data_dir, &error.to_string());
        None
    });
    // Kiro adds what the hook prints to the agent's context, as it is.
    if via.as_deref() == Some("kiro") {
        if let Some(alert) = &alert {
            println!("{alert}");
        }
        return ExitCode::SUCCESS;
    }
    if let Some(agent) = agent {
        let answer = agent.answer(alert.as_deref());
        if !answer.is_empty() {
            println!("{answer}");
        }
    }
    ExitCode::SUCCESS
}

/// The alert or notice to show, when one is due.
fn run_hook(arbiter: &Arbiter, data_dir: &Path, lang: Lang) -> io::Result<Option<String>> {
    let home = arbiter.home();
    let Some(space) = volume::space(home) else {
        return Err(io::Error::other("df did not answer"));
    };
    let index = Index::load(data_dir)?;
    let now = now();
    let _ = delest_core::history::record(data_dir, space, now);
    if let Some(index) = &index
        && now - index.scanned_at > REFRESH_AFTER
    {
        refresh_in_background(&index.roots)?;
    }
    let totals = index.as_ref().map(Index::totals).unwrap_or_default();
    let level = volume::level(space, totals.rebuildable + totals.online);
    let mut shown = Shown::load(data_dir);
    if level == Level::Fine || !alerts::due(level, space, &shown, now) {
        return Ok(None);
    }
    let text = alerts::message(level, space, totals, lang);
    alerts::record(level, space, &mut shown, now);
    shown.save(data_dir)?;
    alerts::notify(&text);
    Ok(Some(text))
}

/// `delest scan --quiet <roots>`, detached: the hook does not wait for it.
fn refresh_in_background(roots: &[PathBuf]) -> io::Result<()> {
    let roots: Vec<&PathBuf> = roots.iter().filter(|root| root.is_dir()).collect();
    if roots.is_empty() {
        return Ok(());
    }
    Command::new(std::env::current_exe()?)
        .args(["scan", "--quiet"])
        .args(roots)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

fn log(data_dir: &Path, message: &str) {
    let path = data_dir.join(LOG);
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > LOG_LIMIT) {
        let _ = fs::rename(&path, data_dir.join(format!("{LOG}.1")));
    }
    let _ = fs::create_dir_all(data_dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let time = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        let _ = writeln!(file, "{time} {message}");
    }
}
