//! Delest's window and its menu-bar window, drawn by the system's web view through
//! Tauri. All the logic lives in delest-core; the pages in `ui/` only lay out what
//! the commands below return. Nothing is moved or deleted without a confirmation
//! in the page, and every move is checked again by the core right before it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use delest_core::activity::{Handoff, INACTIVE_AFTER_DAYS};
use delest_core::advisor::{Advice, Opinions};
use delest_core::agents::{self, AGENTS, Agent, State as HookState};
use delest_core::answers::Answers;
use delest_core::arbiter::Arbiter;
use delest_core::clean::{self, Skip};
use delest_core::history::{self, Reading};
use delest_core::index::{Index, Totals};
use delest_core::journal::{self, Action, Record};
use delest_core::lang::{self, Lang};
use delest_core::memory::Memory;
use delest_core::quarantine::{self, Quarantined};
use delest_core::report;
use delest_core::rules::RULES;
use delest_core::verdict::{Assessment, Verdict};
use delest_core::volume::{self, Level};
use delest_core::{home_dir, paths, scan, tilde, timer, tr, update};
use serde::Serialize;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WindowEvent};

mod ia;

pub(crate) struct Ctx {
    pub(crate) home: PathBuf,
    pub(crate) data_dir: PathBuf,
    lang: Mutex<Lang>,
    pub(crate) answers: Mutex<Answers>,
    /// The local AI's installation, when one was started.
    pub(crate) setup: Mutex<ia::Setup>,
    /// The page the main window opens on once, after it reloads in a new language.
    next_page: Mutex<Option<String>>,
}

impl Ctx {
    pub(crate) fn lang(&self) -> Lang {
        *self
            .lang
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The Arbiter with the developer's decisions; an unreadable memory is an error.
    pub(crate) fn arbiter(&self) -> Result<Arbiter, String> {
        let memory = Memory::load(&self.data_dir).map_err(|error| error.to_string())?;
        Ok(Arbiter::with_context(
            self.home.clone(),
            Handoff::load(),
            memory,
        ))
    }

    pub(crate) fn index(&self) -> Result<Option<Index>, String> {
        Index::load(&self.data_dir).map_err(|error| error.to_string())
    }

    pub(crate) fn size(&self, bytes: u64) -> String {
        lang::size(bytes, self.lang())
    }

    pub(crate) fn short(&self, path: &Path) -> String {
        tilde(path, &self.home)
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

#[tauri::command]
fn ui_lang(ctx: State<Ctx>) -> &'static str {
    ctx.lang().code()
}

#[derive(Serialize)]
struct Language {
    /// "system", "en" or "fr".
    choice: &'static str,
    system: &'static str,
    /// `DELEST_LANG` is set: it wins over the choice.
    forced: bool,
}

#[tauri::command]
fn language(ctx: State<Ctx>) -> Language {
    Language {
        choice: lang::chosen(&ctx.data_dir).map_or("system", Lang::code),
        system: Lang::system().code(),
        forced: std::env::var_os(lang::ENV_LANG).is_some(),
    }
}

/// Keeps the choice, for the window and the command alike, then redraws both windows in it.
#[tauri::command]
fn set_language(app: AppHandle, ctx: State<Ctx>, choice: String) -> Result<(), String> {
    let chosen = Lang::parse(&choice);
    lang::choose(&ctx.data_dir, chosen).map_err(|error| error.to_string())?;
    *ctx.lang
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Lang::detect();
    *ctx.next_page
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some("reglages".into());
    for window in app.webview_windows().values() {
        let _ = window.eval("location.reload()");
    }
    Ok(())
}

/// The page shown first: `DELEST_GUI_PAGE=quarantaine`, for checks; the disk otherwise.
#[tauri::command]
fn first_page(ctx: State<Ctx>) -> String {
    let next = ctx
        .next_page
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    next.or_else(|| std::env::var("DELEST_GUI_PAGE").ok())
        .filter(|page| {
            [
                "disque",
                "propositions",
                "inconnus",
                "quarantaine",
                "reglages",
            ]
            .contains(&page.as_str())
        })
        .unwrap_or_else(|| "disque".into())
}

#[derive(Serialize)]
struct About {
    version: &'static str,
    author: &'static str,
}

#[tauri::command]
fn about() -> About {
    About {
        version: env!("CARGO_PKG_VERSION"),
        author: env!("CARGO_PKG_AUTHORS"),
    }
}

/// Opens a fixed address in the browser, nothing taken from the page.
fn open_in_browser(address: &str) -> Result<(), String> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(address)
        .status()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The project's page in the browser.
#[tauri::command]
fn open_project() -> Result<(), String> {
    open_in_browser(env!("CARGO_PKG_REPOSITORY"))
}

/// The newer version published, if any, unless the developer turned the question off.
#[tauri::command]
async fn update_available(ctx: State<'_, Ctx>) -> Result<Option<String>, String> {
    let memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    if memory.updates_never {
        return Ok(None);
    }
    let current = env!("CARGO_PKG_VERSION");
    Ok(update::newer_than(current, &ctx.data_dir, now(), || {
        update::fetch_latest(current)
    }))
}

/// The download page in the browser.
#[tauri::command]
fn open_download() -> Result<(), String> {
    open_in_browser(update::DOWNLOAD)
}

#[tauri::command]
async fn set_updates(ctx: State<'_, Ctx>, never: bool) -> Result<(), String> {
    let mut memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    memory.updates_never = never;
    memory
        .save(&ctx.data_dir)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn js_error(page: String, message: String) {
    eprintln!("delest-gui: {page}: {message}");
}

// ---------------------------------------------------------------- Disk overview

#[derive(Serialize)]
struct Bar {
    label: String,
    size: u64,
    shown: String,
}

#[derive(Serialize)]
struct Overview {
    free: u64,
    total: u64,
    free_shown: String,
    total_shown: String,
    level: &'static str,
    rebuildable: u64,
    online: u64,
    to_review: u64,
    freeable_shown: String,
    after_shown: String,
    categories: Vec<Bar>,
    projects: Vec<Bar>,
    project_count: usize,
    history: Vec<Reading>,
    alert_bytes: u64,
    scanned: Option<String>,
    roots: Vec<String>,
    quarantine_count: usize,
    quarantine_shown: String,
}

fn overview_of(ctx: &Ctx) -> Result<Overview, String> {
    let space = volume::space(&ctx.home).ok_or("df did not report the free space")?;
    let now = now();
    let _ = history::record(&ctx.data_dir, space, now);
    let index = ctx.index()?;
    let totals: Totals = index.as_ref().map(Index::totals).unwrap_or_default();
    let items: &[Assessment] = index
        .as_ref()
        .map(|index| index.items.as_slice())
        .unwrap_or(&[]);
    let projects = report::projects(items, ctx.lang());
    let mut by_kind: Vec<(String, u64)> = Vec::new();
    for line in projects.iter().flat_map(|project| &project.lines) {
        match by_kind.iter_mut().find(|(kind, _)| *kind == line.kind) {
            Some((_, size)) => *size += line.size,
            None => by_kind.push((line.kind.clone(), line.size)),
        }
    }
    by_kind.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    let bar = |label: String, size: u64| Bar {
        label,
        shown: ctx.size(size),
        size,
    };
    let freeable = totals.rebuildable + totals.online;
    let entries = quarantine::list(&ctx.home, &ctx.data_dir);
    let scanned = index
        .as_ref()
        .map(|index| lang::since((now - index.scanned_at).max(0), ctx.lang()));
    Ok(Overview {
        free: space.available,
        total: space.total,
        free_shown: ctx.size(space.available),
        total_shown: ctx.size(space.total),
        level: match volume::level(space, freeable) {
            Level::Alert => "alert",
            Level::Notice => "notice",
            Level::Fine => "fine",
        },
        rebuildable: totals.rebuildable,
        online: totals.online,
        to_review: totals.to_review,
        freeable_shown: ctx.size(freeable),
        after_shown: ctx.size(space.available + freeable),
        categories: by_kind
            .into_iter()
            .take(6)
            .map(|(kind, size)| bar(kind, size))
            .collect(),
        project_count: projects.len(),
        projects: projects
            .iter()
            .take(6)
            .map(|project| {
                let name = project
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                bar(name, project.size)
            })
            .collect(),
        history: history::load(&ctx.data_dir),
        alert_bytes: (space.total / 100 * volume::ALERT_FREE_PERCENT).max(volume::ALERT_FREE_BYTES),
        scanned,
        roots: index
            .iter()
            .flat_map(|index| &index.roots)
            .map(|root| ctx.short(root))
            .collect(),
        quarantine_count: entries.len(),
        quarantine_shown: ctx.size(entries.iter().map(Quarantined::size).sum()),
    })
}

#[tauri::command]
async fn overview(ctx: State<'_, Ctx>) -> Result<Overview, String> {
    overview_of(&ctx)
}

/// Scans the folders of the last scan again, or the given one; slow, so async.
#[tauri::command]
async fn rescan(ctx: State<'_, Ctx>, folder: Option<String>) -> Result<Overview, String> {
    let roots: Vec<PathBuf> = match folder.filter(|folder| !folder.trim().is_empty()) {
        Some(folder) => {
            let path = delest_core::expand_home(Path::new(folder.trim()), &ctx.home);
            vec![paths::canonical(&path).map_err(|error| format!("{}: {error}", path.display()))?]
        }
        None => ctx
            .index()?
            .map(|index| index.roots)
            .filter(|roots| !roots.is_empty())
            .unwrap_or_else(|| vec![ctx.home.clone()]),
    };
    let arbiter = ctx.arbiter()?;
    let found = scan::scan(&roots, &arbiter, false);
    let previous = ctx.index()?;
    Index::update(previous, &roots, &found, now())
        .save(&ctx.data_dir)
        .map_err(|error| error.to_string())?;
    overview_of(&ctx)
}

// ---------------------------------------------------------------- Proposals

#[derive(Serialize)]
struct Item {
    path: String,
    shown: String,
    kind: String,
    verdict: Verdict,
    size: u64,
    size_shown: String,
    trace: String,
    /// Offered checked: freeable, and not the dependencies of an active project.
    suggested: bool,
    /// Why it is not checked by default, when it is not.
    note: Option<String>,
}

#[derive(Serialize)]
struct ProjectView {
    name: String,
    root: String,
    size: u64,
    size_shown: String,
    items: Vec<Item>,
}

#[tauri::command]
async fn proposals(ctx: State<'_, Ctx>) -> Result<Vec<ProjectView>, String> {
    let index = ctx.index()?;
    let items: &[Assessment] = index
        .as_ref()
        .map(|index| index.items.as_slice())
        .unwrap_or(&[]);
    let idle: std::collections::HashMap<&Path, Option<u64>> = items
        .iter()
        .map(|item| (item.path.as_path(), item.idle_days))
        .collect();
    let lang = ctx.lang();
    Ok(report::projects(items, lang)
        .into_iter()
        .map(|project| ProjectView {
            name: project
                .root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            root: ctx.short(&project.root),
            size: project.size,
            size_shown: ctx.size(project.size),
            items: project
                .lines
                .into_iter()
                .map(|line| {
                    let active = idle
                        .get(line.path.as_path())
                        .copied()
                        .flatten()
                        .is_some_and(|days| days < INACTIVE_AFTER_DAYS);
                    let note = match line.verdict {
                        Verdict::ToReview => Some(tr!(
                            lang,
                            "to review: check it only if you are sure",
                            "à juger : ne cochez que si vous êtes sûr"
                        )),
                        _ if active => Some(tr!(
                            lang,
                            "project worked on this month",
                            "projet travaillé ce mois-ci"
                        )),
                        Verdict::RebuildOnline => Some(tr!(
                            lang,
                            "reinstalled from the network",
                            "réinstallé par le réseau"
                        )),
                        _ => None,
                    };
                    let shown = line
                        .path
                        .strip_prefix(&project.root)
                        .map(|rest| rest.display().to_string())
                        .unwrap_or_else(|_| ctx.short(&line.path));
                    Item {
                        path: line.path.display().to_string(),
                        shown,
                        kind: line.kind,
                        verdict: line.verdict,
                        size: line.size,
                        size_shown: ctx.size(line.size),
                        trace: line.trace,
                        suggested: line.verdict == Verdict::Rebuildable && !active,
                        note,
                    }
                })
                .collect(),
        })
        .collect())
}

#[derive(Serialize)]
struct Preview {
    count: usize,
    size_shown: String,
    free_shown: String,
    after_shown: String,
    free: u64,
    after: u64,
}

pub(crate) fn chosen(ctx: &Ctx, paths: &[String]) -> Result<Vec<Assessment>, String> {
    let wanted: HashSet<&str> = paths.iter().map(String::as_str).collect();
    // An unknown folder the model offered for review may be chosen, one by one.
    let opinions = Opinions::load(&ctx.data_dir);
    Ok(ctx
        .index()?
        .map(|index| index.items)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| wanted.contains(item.path.to_string_lossy().as_ref()) && item.path.exists())
        .filter(|item| {
            matches!(
                item.verdict,
                Verdict::Rebuildable | Verdict::RebuildOnline | Verdict::ToReview
            ) || (item.verdict == Verdict::Unknown
                && opinions
                    .current(item)
                    .is_some_and(|o| o.advice == Advice::Review))
        })
        .collect())
}

#[tauri::command]
async fn preview(ctx: State<'_, Ctx>, paths: Vec<String>) -> Result<Preview, String> {
    let items = chosen(&ctx, &paths)?;
    let size: u64 = items.iter().filter_map(|item| item.size).sum();
    let space = volume::space(&ctx.home).ok_or("df did not report the free space")?;
    Ok(Preview {
        count: items.len(),
        size_shown: ctx.size(size),
        free_shown: ctx.size(space.available),
        after_shown: ctx.size(space.available + size),
        free: space.available,
        after: space.available + size,
    })
}

#[derive(Serialize)]
struct Moved {
    count: usize,
    size_shown: String,
    skipped: Vec<String>,
    purged: usize,
}

/// Moves the chosen items into the quarantine; the core checks each one again first.
#[tauri::command]
async fn quarantine_items(ctx: State<'_, Ctx>, paths: Vec<String>) -> Result<Moved, String> {
    let items = chosen(&ctx, &paths)?;
    let arbiter = ctx.arbiter()?;
    let outcome = clean::execute(&items, &arbiter, &ctx.data_dir, now());
    let lang = ctx.lang();
    let skipped = outcome
        .skipped
        .iter()
        .map(|(item, skip)| {
            let why = match skip {
                Skip::Changed { now } => {
                    let now = now.label(lang);
                    tr!(
                        lang,
                        "its verdict is now {now}",
                        "son verdict est maintenant {now}"
                    )
                }
                Skip::RecentlyModified => tr!(
                    lang,
                    "modified a few minutes ago",
                    "modifié il y a quelques minutes"
                ),
                Skip::InUse { program } => {
                    tr!(lang, "in use by {program}", "utilisé par {program}")
                }
                Skip::Failed(error) => error.text(lang),
            };
            format!("{} — {why}", ctx.short(&item.path))
        })
        .collect();
    Ok(Moved {
        count: outcome.moved.len(),
        size_shown: ctx.size(outcome.moved.iter().map(Quarantined::size).sum()),
        skipped,
        purged: outcome.purged.len(),
    })
}

// ---------------------------------------------------------------- Quarantine

#[derive(Serialize)]
struct Entry {
    id: String,
    path: String,
    kind: String,
    size: u64,
    size_shown: String,
    age_days: i64,
    days_left: i64,
    /// "now": rebuilds locally; "network": may go early on request, and needs the
    /// network to come back; "kept": to review, kept its whole retention period.
    purge: &'static str,
}

#[tauri::command]
async fn quarantine_list(ctx: State<'_, Ctx>) -> Result<Vec<Entry>, String> {
    let now = now();
    let mut entries = quarantine::list(&ctx.home, &ctx.data_dir);
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.size()));
    Ok(entries
        .iter()
        .map(|entry| Entry {
            id: entry.id(),
            path: ctx.short(&entry.entry.original),
            kind: report::kind(&entry.entry.assessment, ctx.lang()),
            size: entry.size(),
            size_shown: ctx.size(entry.size()),
            age_days: entry.age_days(now),
            days_left: (quarantine::RETENTION_DAYS - entry.age_days(now)).max(0),
            purge: if quarantine::may_purge_early(entry) {
                "now"
            } else if quarantine::may_purge_on_request(entry) {
                "network"
            } else {
                "kept"
            },
        })
        .collect())
}

#[tauri::command]
async fn restore(ctx: State<'_, Ctx>, id: String) -> Result<String, String> {
    let entries = quarantine::list(&ctx.home, &ctx.data_dir);
    let entry = entries
        .iter()
        .find(|entry| entry.id() == id)
        .ok_or("no such entry")?;
    quarantine::restore(entry).map_err(|error| error.text(ctx.lang()))?;
    let mut memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    memory.record_restore(
        &entry.entry.original,
        entry.entry.assessment.rule.as_deref(),
        now(),
    );
    memory
        .save(&ctx.data_dir)
        .map_err(|error| error.to_string())?;
    let record = Record::now(
        Action::Restore,
        &entry.entry.original,
        &entry.dir,
        &entry.entry.assessment,
    );
    let _ = journal::append(&ctx.data_dir, &record);
    Ok(ctx.short(&entry.entry.original))
}

/// Deletes the given entries for good: those that rebuild locally, and with
/// `network`, which the developer confirmed knowing it, those that rebuild from the
/// network too.
#[tauri::command]
async fn purge(ctx: State<'_, Ctx>, ids: Vec<String>, network: bool) -> Result<String, String> {
    // A second click while gigabytes are being deleted must not start a second run.
    static PURGING: Mutex<()> = Mutex::new(());
    let Ok(_running) = PURGING.try_lock() else {
        return Err(tr!(
            ctx.lang(),
            "A deletion is already running.",
            "Une suppression est déjà en cours."
        ));
    };
    let entries = quarantine::list(&ctx.home, &ctx.data_dir);
    let allowed = |entry: &Quarantined| {
        quarantine::may_purge_early(entry) || (network && quarantine::may_purge_on_request(entry))
    };
    let mut freed = 0;
    for entry in entries
        .iter()
        .filter(|entry| ids.contains(&entry.id()) && allowed(entry))
    {
        quarantine::purge(entry).map_err(|error| error.to_string())?;
        freed += entry.size();
        let record = Record::now(
            Action::Purge,
            &entry.entry.original,
            &entry.dir,
            &entry.entry.assessment,
        );
        let _ = journal::append(&ctx.data_dir, &record);
    }
    Ok(ctx.size(freed))
}

// ---------------------------------------------------------------- Settings

#[derive(Serialize)]
struct Shadow {
    rule: String,
    count: usize,
    size_shown: String,
}

#[derive(Serialize)]
struct HookLine {
    agent: &'static str,
    name: &'static str,
    state: &'static str,
}

#[derive(Serialize)]
struct Settings {
    protected: Vec<String>,
    command: Option<String>,
    hooks: Vec<HookLine>,
    timer_supported: bool,
    timer: bool,
    shadow: Vec<Shadow>,
    updates_never: bool,
}

/// The `delest` command the hook and the timer run: the one installed with the
/// developer's commands, which the hooks set from the terminal already run, else the
/// one inside Delest.app, next to this program.
fn command_line() -> Option<PathBuf> {
    delest_core::paths::find_program("delest").or_else(|| {
        let beside = std::env::current_exe()
            .ok()?
            .with_file_name(paths::executable_name("delest"));
        let built = beside.components().any(|part| part.as_os_str() == "target");
        (beside.is_file() && !built).then_some(beside)
    })
}

#[tauri::command]
async fn settings(ctx: State<'_, Ctx>) -> Result<Settings, String> {
    let memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    let command = command_line();
    let hooks = AGENTS
        .into_iter()
        .filter(|agent| agent.detected(&ctx.home))
        .map(|agent| HookLine {
            agent: agent.id(),
            name: agent.name(),
            state: match &command {
                None => "no-command",
                Some(exe) => match agents::state(
                    agent,
                    &agent.settings(&ctx.home),
                    &agents::hook_command(exe, agent),
                ) {
                    Ok(HookState::Installed) => "installed",
                    Ok(HookState::Outdated) => "outdated",
                    Ok(HookState::Absent) => "absent",
                    Err(_) => "unreadable",
                },
            },
        })
        .collect();
    let items = ctx.index()?.map(|index| index.items).unwrap_or_default();
    let shadow = RULES
        .iter()
        .filter(|rule| rule.shadow && !memory.accepted_rules.contains(rule.name))
        .map(|rule| {
            let found: Vec<&Assessment> = items
                .iter()
                .filter(|item| {
                    item.rule.as_deref() == Some(rule.name)
                        && item.shadow.is_some()
                        && item.path.exists()
                })
                .collect();
            Shadow {
                rule: rule.name.to_string(),
                count: found.len(),
                size_shown: ctx.size(found.iter().filter_map(|item| item.size).sum()),
            }
        })
        .collect();
    Ok(Settings {
        protected: memory
            .protected
            .iter()
            .map(|path| ctx.short(path))
            .collect(),
        command: command.map(|exe| ctx.short(&exe)),
        hooks,
        timer_supported: timer::supported(),
        timer: timer::enabled(&ctx.home),
        shadow,
        updates_never: memory.updates_never,
    })
}

#[tauri::command]
async fn set_protected(ctx: State<'_, Ctx>, path: String, on: bool) -> Result<(), String> {
    let full = delest_core::expand_home(Path::new(path.trim()), &ctx.home);
    let full = paths::canonical(&full).unwrap_or(full);
    let mut memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    if on {
        memory.protected.insert(full);
    } else {
        memory.protected.remove(&full);
    }
    memory
        .save(&ctx.data_dir)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_hook(ctx: State<'_, Ctx>, agent: String, on: bool) -> Result<(), String> {
    let agent = Agent::from_id(&agent).ok_or("unknown agent")?;
    let settings_file = agent.settings(&ctx.home);
    let change = if on {
        let exe = command_line().ok_or("the delest command is not installed")?;
        agents::plan_install(agent, &settings_file, &agents::hook_command(&exe, agent))
    } else {
        agents::plan_remove(agent, &settings_file)
    }
    .map_err(|error| error.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    change
        .apply(&ctx.data_dir.join("backups").join(stamp))
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_timer(ctx: State<'_, Ctx>, on: bool) -> Result<(), String> {
    if on {
        let exe = command_line().ok_or("the delest command is not installed")?;
        timer::enable(&ctx.home, &exe)
    } else {
        timer::disable(&ctx.home)
    }
    .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------- Windows

/// One window at a time: the main window takes the gauge's place, and the other
/// way round, so the gauge never shows figures older than the main window's.
#[tauri::command]
fn open_main(app: AppHandle) {
    if let Some(gauge) = app.get_webview_window("barre") {
        let _ = gauge.hide();
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The gauge in the main window's place, with fresh figures.
#[tauri::command]
fn open_gauge(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let _ = app.emit_to("barre", "refresh", ());
    toggle_barre(&app, None, true);
}

/// The gauge's × hides it, Delest staying in the menu bar; the main window's ×
/// quits Delest.
#[tauri::command]
fn close_window(app: AppHandle, label: String) {
    if label == "main" {
        app.exit(0);
    } else if let Some(window) = app.get_webview_window(&label) {
        let _ = window.hide();
    }
}

/// Where the developer left the gauge, kept in the data directory.
const PLACES: &str = "fenetres.json";

fn saved_gauge_place(data_dir: &Path) -> Option<(i32, i32)> {
    let text = std::fs::read_to_string(data_dir.join(PLACES)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let place = value["jauge"].as_array()?;
    Some((
        place.first()?.as_i64()? as i32,
        place.get(1)?.as_i64()? as i32,
    ))
}

fn save_gauge_place(data_dir: &Path, (x, y): (i32, i32)) {
    if saved_gauge_place(data_dir) == Some((x, y)) {
        return;
    }
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(
        data_dir.join(PLACES),
        serde_json::json!({ "jauge": [x, y] }).to_string() + "\n",
    );
}

/// Puts the gauge where the developer left it, if that is still on a screen; else in
/// the top right corner under the menu bar, or just left of it when Vario is
/// installed, since Vario's gauge takes that corner.
fn place_gauge(app: &AppHandle, data_dir: &Path) {
    const MARGIN: i32 = 12;
    let Some(window) = app.get_webview_window("barre") else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let on_a_screen = |(x, y): (i32, i32)| {
        window.available_monitors().is_ok_and(|monitors| {
            monitors.iter().any(|monitor| {
                let area = monitor.work_area();
                x >= area.position.x
                    && y >= area.position.y
                    && x + size.width as i32 <= area.position.x + area.size.width as i32
                    && y < area.position.y + area.size.height as i32
            })
        })
    };
    let place = saved_gauge_place(data_dir)
        .filter(|&place| on_a_screen(place))
        .or_else(|| {
            let monitor = window.current_monitor().ok().flatten()?;
            let area = monitor.work_area();
            let margin = (f64::from(MARGIN) * monitor.scale_factor()) as i32;
            let slots = if paths::find_program("vario").is_some() {
                2
            } else {
                1
            };
            let x = area.position.x + area.size.width as i32 - slots * (size.width as i32 + margin);
            Some((x, area.position.y + margin))
        });
    if let Some((x, y)) = place {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// The tray icon's place on screen, in physical pixels.
#[derive(Clone, Copy)]
struct Icon {
    center: f64,
    top: f64,
    bottom: f64,
}

/// Where the gauge goes next to the tray icon: under it when there is room (a menu
/// bar at the top), above it otherwise (a taskbar at the bottom), always inside the
/// screen's work area `(x, y, width, height)`.
fn beside_icon(icon: Icon, size: (f64, f64), area: (f64, f64, f64, f64)) -> (f64, f64) {
    let (width, height) = size;
    let (left, top, area_width, area_height) = area;
    let x = (icon.center - width / 2.0).clamp(left, (left + area_width - width).max(left));
    let y = if icon.bottom + height <= top + area_height {
        icon.bottom.max(top)
    } else {
        (icon.top - height).max(top)
    };
    (x, y)
}

/// Shows the floating gauge, next to the tray icon the first time, or hides it.
fn toggle_barre(app: &AppHandle, under: Option<Icon>, show_only: bool) {
    let Some(window) = app.get_webview_window("barre") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        if !show_only {
            let _ = window.hide();
        }
        return;
    }
    if let (Some(icon), Ok(size)) = (under, window.outer_size()) {
        let contains = |monitor: &tauri::Monitor| {
            let area = monitor.work_area();
            let (x, width) = (f64::from(area.position.x), f64::from(area.size.width));
            icon.center >= x && icon.center < x + width
        };
        let area = window
            .available_monitors()
            .ok()
            .and_then(|monitors| monitors.into_iter().find(|monitor| contains(monitor)))
            .map(|monitor| {
                let area = monitor.work_area();
                (
                    f64::from(area.position.x),
                    f64::from(area.position.y),
                    f64::from(area.size.width),
                    f64::from(area.size.height),
                )
            });
        let size = (f64::from(size.width), f64::from(size.height));
        let (x, y) = match area {
            Some(area) => beside_icon(icon, size, area),
            None => ((icon.center - size.0 / 2.0).max(0.0), icon.bottom),
        };
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn main() {
    let ctx = Ctx {
        home: home_dir(),
        data_dir: paths::data_dir(),
        lang: Mutex::new(Lang::detect()),
        next_page: Mutex::new(None),
        answers: Mutex::new(Answers::load(&paths::data_dir())),
        setup: Mutex::new(ia::Setup::default()),
    };
    tauri::Builder::default()
        .manage(ctx)
        .setup(|app| {
            // macOS: the menu bar's D, black on clear, tinted to match the bar. Elsewhere
            // the image would stay black on a dark panel or taskbar: the app's icon.
            let macos = cfg!(target_os = "macos");
            let icon = if macos {
                tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?
            } else {
                app.default_window_icon().cloned().ok_or("no icon")?
            };
            let mut tray = TrayIconBuilder::with_id("delest")
                .icon(icon)
                .icon_as_template(macos)
                .tooltip(concat!("Delest · ", env!("CARGO_PKG_AUTHORS")));
            // Linux shows a tray icon only with a menu, and sends it no click: the
            // menu does what the click does elsewhere (seen on Xubuntu 24.04).
            if cfg!(target_os = "linux") {
                use tauri::menu::{Menu, MenuItem};
                let lang = app.state::<Ctx>().lang();
                let gauge = MenuItem::with_id(
                    app,
                    "gauge",
                    tr!(lang, "Floating gauge", "Jauge flottante"),
                    true,
                    None::<&str>,
                )?;
                let open = MenuItem::with_id(
                    app,
                    "open",
                    tr!(lang, "Open Delest", "Ouvrir Delest"),
                    true,
                    None::<&str>,
                )?;
                let quit = MenuItem::with_id(
                    app,
                    "quit",
                    tr!(lang, "Quit", "Quitter"),
                    true,
                    None::<&str>,
                )?;
                let menu = Menu::with_items(app, &[&gauge, &open, &quit])?;
                tray = tray
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "gauge" => open_gauge(app.clone()),
                        "open" => open_main(app.clone()),
                        "quit" => app.exit(0),
                        _ => {}
                    });
            }
            tray.on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } = event
                {
                    let app = tray.app_handle();
                    // The main window open: the click goes back to the gauge.
                    if app
                        .get_webview_window("main")
                        .is_some_and(|main| main.is_visible().unwrap_or(false))
                    {
                        open_gauge(app.clone());
                        return;
                    }
                    let scale = app
                        .get_webview_window("barre")
                        .and_then(|window| window.scale_factor().ok())
                        .unwrap_or(1.0);
                    let position = rect.position.to_physical::<f64>(scale);
                    let size = rect.size.to_physical::<f64>(scale);
                    let icon = Icon {
                        center: position.x + size.width / 2.0,
                        top: position.y,
                        bottom: position.y + size.height,
                    };
                    toggle_barre(app, Some(icon), false);
                }
            })
            .build(app)?;
            // Delest starts as its gauge; the main window opens from it. A page asked
            // with DELEST_GUI_PAGE, for checks, opens the main window instead.
            if std::env::var_os("DELEST_GUI_PAGE").is_some() {
                open_main(app.handle().clone());
            } else {
                place_gauge(app.handle(), &paths::data_dir());
                toggle_barre(app.handle(), None, true);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the main window quits Delest, as its × does.
            WindowEvent::CloseRequested { .. } if window.label() == "main" => {
                window.app_handle().exit(0);
            }
            // The gauge opens again where the developer left it.
            WindowEvent::Moved(at) if window.label() == "barre" => {
                save_gauge_place(&paths::data_dir(), (at.x, at.y));
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            ui_lang,
            language,
            set_language,
            first_page,
            about,
            open_project,
            update_available,
            open_download,
            set_updates,
            js_error,
            overview,
            rescan,
            proposals,
            preview,
            quarantine_items,
            quarantine_list,
            restore,
            purge,
            settings,
            set_protected,
            set_hook,
            set_timer,
            open_main,
            open_gauge,
            close_window,
            ia::unknowns,
            ia::advise_unknowns,
            ia::ask,
            ia::ask_agent,
            ia::ai_settings,
            ia::ai_install_plan,
            ia::install_ai,
            ia::ai_install_status,
            ia::set_agent,
            ia::journal
        ])
        .run(tauri::generate_context!())
        .expect("Delest's window could not start");
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::{Icon, beside_icon};

    #[test]
    fn the_gauge_opens_under_a_top_menu_bar_and_above_a_bottom_taskbar() {
        let gauge = (312.0, 988.0);
        // macOS: menu bar at the top of a 2880 × 1800 screen, work area below it.
        let menu_bar = Icon {
            center: 2500.0,
            top: 0.0,
            bottom: 48.0,
        };
        assert_eq!(
            beside_icon(menu_bar, gauge, (0.0, 48.0, 2880.0, 1752.0)),
            (2344.0, 48.0)
        );
        // Windows: taskbar at the bottom of a 1920 × 1080 screen, icon near the corner.
        let taskbar = Icon {
            center: 1880.0,
            top: 1040.0,
            bottom: 1080.0,
        };
        assert_eq!(
            beside_icon(taskbar, (156.0, 494.0), (0.0, 0.0, 1920.0, 1040.0)),
            (1764.0, 546.0)
        );
    }

    /// The texts of `data-t` elements: what lies between the tag and the next one.
    fn marked_elements(page: &str) -> Vec<String> {
        let mut texts = Vec::new();
        for (at, _) in page.match_indices(" data-t") {
            let rest = &page[at..];
            let (Some(open), Some(close)) = (rest.find('>'), rest.find('<')) else {
                continue;
            };
            if open < close {
                // As the page reads it: `textContent`, entities decoded.
                let text = rest[open + 1..close].split_whitespace().collect::<Vec<_>>();
                let text = text
                    .join(" ")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&");
                texts.push(text);
            }
        }
        texts
    }

    /// The `placeholder` and `aria-label` values written in the page; a path given
    /// as an example (`~/…`) is not translated.
    fn marked_attributes(page: &str) -> Vec<String> {
        ["placeholder=\"", "aria-label=\""]
            .iter()
            .flat_map(|name| page.split(name).skip(1))
            .filter_map(|rest| rest.split('"').next())
            .filter(|value| !value.starts_with('~') && !value.contains("${"))
            .map(str::to_string)
            .collect()
    }

    /// The string literals a `t(…)` call may return: the whole argument, or a branch
    /// of a `?:` in it; the operands of a comparison are not texts.
    fn calls_of_t(script: &str) -> Vec<String> {
        let mut texts = Vec::new();
        let bytes = script.as_bytes();
        for (at, _) in script.match_indices("t(") {
            if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || b"_.$".contains(&bytes[at - 1]))
            {
                continue;
            }
            let (mut depth, mut i, mut previous) = (0, at + 1, b'(');
            while i < bytes.len() {
                match bytes[i] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'"' => {
                        let end = i + 1 + script[i + 1..].find('"').unwrap();
                        if b"(?:".contains(&previous) {
                            texts.push(script[i + 1..end].to_string());
                        }
                        i = end;
                        previous = b'"';
                    }
                    byte if !byte.is_ascii_whitespace() => previous = byte,
                    _ => {}
                }
                i += 1;
            }
        }
        texts
    }

    /// The French keys of the script's `ENGLISH_WORDS` table.
    fn table(script: &str) -> BTreeSet<String> {
        let body = script
            .split("const ENGLISH_WORDS = {")
            .nth(1)
            .unwrap()
            .split("\n};")
            .next()
            .unwrap();
        body.split("\":")
            .filter_map(|before| before.rsplit_once('"').map(|(_, key)| key.to_string()))
            .collect()
    }

    #[test]
    fn every_interface_text_is_translated_to_english() {
        let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
        let mut checked = 0;
        for (page, script) in [("index.html", "app.js"), ("barre.html", "barre.js")] {
            let page = std::fs::read_to_string(ui.join(page)).unwrap();
            let script = std::fs::read_to_string(ui.join(script)).unwrap();
            let marked: BTreeSet<String> = marked_elements(&page)
                .into_iter()
                .chain(marked_attributes(&page))
                .chain(calls_of_t(&script))
                .filter(|text| !text.is_empty())
                .collect();
            let english = table(&script);
            let missing: Vec<&String> = marked.difference(&english).collect();
            assert!(
                missing.is_empty(),
                "not translated to English: {missing:#?}"
            );
            checked += marked.len();
        }
        assert!(checked > 100, "found only {checked} texts");
    }
}
