//! The AI in the window: the unknown folders with the model's opinion, the free
//! questions to Delest (the local model) or to the developer's agent, and the AI
//! settings with the journal of opinions. None of it can make a folder freeable.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use delest_core::advisor::{self, Advice, Opinions};
use delest_core::answers::Kept;
use delest_core::capacities::{self, Reasoning};
use delest_core::index::Index;
use delest_core::install::{self, Step, What};
use delest_core::journal::{self, Action};
use delest_core::lang;
use delest_core::memory::Memory;
use delest_core::models::{self, CATALOG, Manifest};
use delest_core::verdict::{Assessment, Verdict};
use delest_core::{engine, paths, tr};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{Ctx, now};

/// The agent's model for the questions Delest passes on: the most economical one.
const AGENT_MODEL: &str = "claude-haiku-5-5";

#[derive(Serialize)]
pub struct OpinionView {
    nature: String,
    advice: Advice,
    reason: String,
    neighbours: Vec<String>,
    model: String,
    seconds: f32,
}

#[derive(Serialize)]
pub struct UnknownView {
    path: String,
    shown: String,
    size: u64,
    size_shown: String,
    trace: String,
    opinion: Option<OpinionView>,
}

fn opinion_view(ctx: &Ctx, opinion: &advisor::Opinion) -> OpinionView {
    OpinionView {
        nature: opinion.nature.clone(),
        advice: opinion.advice,
        reason: opinion.reason.clone(),
        neighbours: opinion
            .neighbours
            .iter()
            .map(|neighbour| {
                format!(
                    "{} ({})",
                    ctx.short(&neighbour.path),
                    neighbour.verdict.label(ctx.lang())
                )
            })
            .collect(),
        model: opinion.model.clone(),
        seconds: opinion.seconds,
    }
}

/// The true unknowns of the last scan: no rule, no rule in shadow, no ban.
fn unknown_items(ctx: &Ctx) -> Result<(Vec<Assessment>, Vec<Assessment>), String> {
    let items = ctx.index()?.map(|index| index.items).unwrap_or_default();
    let (unknowns, others): (Vec<Assessment>, Vec<Assessment>) = items
        .into_iter()
        .partition(|item| item.verdict == Verdict::Unknown && item.shadow.is_none());
    // The index may predate a rule or a ban: each folder is judged again as it is now.
    let arbiter = ctx.arbiter()?;
    let still: Vec<Assessment> = unknowns
        .into_iter()
        .filter(|item| item.path.is_dir() && arbiter.assess(&item.path).verdict == Verdict::Unknown)
        .collect();
    Ok((still, others))
}

#[tauri::command]
pub async fn unknowns(ctx: State<'_, Ctx>) -> Result<Vec<UnknownView>, String> {
    let (items, _) = unknown_items(&ctx)?;
    let opinions = Opinions::load(&ctx.data_dir);
    let mut views: Vec<UnknownView> = items
        .iter()
        .map(|item| UnknownView {
            path: item.path.display().to_string(),
            shown: ctx.short(&item.path),
            size: item.size.unwrap_or(0),
            size_shown: ctx.size(item.size.unwrap_or(0)),
            trace: item.trace(ctx.lang()),
            opinion: opinions
                .current(item)
                .map(|opinion| opinion_view(&ctx, opinion)),
        })
        .collect();
    views.sort_by_key(|view| std::cmp::Reverse(view.size));
    Ok(views)
}

/// Gives the model's opinion on the unknowns that have none; slow, so async.
#[tauri::command]
pub async fn advise_unknowns(ctx: State<'_, Ctx>) -> Result<usize, String> {
    let (items, others) = unknown_items(&ctx)?;
    let setup = advisor::setup(&ctx.home).map_err(|missing| missing.text(ctx.lang()))?;
    let examples: Vec<Assessment> = others
        .into_iter()
        .filter(|item| item.verdict != Verdict::Unknown)
        .collect();
    let given = advisor::advise(&items, &examples, &setup.models(), &ctx.data_dir, |_| {})
        .map_err(|error| error.to_string())?;
    Ok(given.len())
}

#[derive(Serialize)]
pub struct Answer {
    text: String,
    by: String,
    seconds: f32,
    /// When the answer was first given, if it is one kept from then.
    kept: Option<String>,
}

/// The answer already given to the same prompt by the same model, unless the
/// developer asks again.
fn kept(ctx: &Ctx, by: &str, prompt: &str, fresh: bool) -> Option<Answer> {
    if fresh {
        return None;
    }
    let answers = ctx
        .answers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let kept = answers.get(by, prompt, now())?;
    Some(Answer {
        text: kept.text.clone(),
        by: by.to_string(),
        seconds: kept.seconds,
        kept: Some(lang::since(now() - kept.at, ctx.lang())),
    })
}

/// Keeps a new answer; failing to write it only costs the next wait.
fn keep(ctx: &Ctx, prompt: &str, answer: Answer) -> Answer {
    let mut answers = ctx
        .answers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let kept = Kept {
        text: answer.text.clone(),
        seconds: answer.seconds,
        at: now(),
    };
    answers.put(&answer.by, prompt, kept);
    let _ = answers.save(&ctx.data_dir);
    answer
}

/// The folder as last scanned, or as it is now if the scan did not see it.
fn item_for(ctx: &Ctx, path: &str) -> Result<Assessment, String> {
    let path = Path::new(path);
    let indexed = ctx
        .index()?
        .and_then(|index: Index| index.items.into_iter().find(|item| item.path == path));
    match indexed {
        Some(item) => Ok(item),
        None => Ok(ctx.arbiter()?.assess(path)),
    }
}

/// A free question to the local model about one folder.
#[tauri::command]
pub async fn ask(
    ctx: State<'_, Ctx>,
    path: String,
    question: String,
    fresh: bool,
) -> Result<Answer, String> {
    let item = item_for(&ctx, &path)?;
    let setup = advisor::setup(&ctx.home).map_err(|missing| missing.text(ctx.lang()))?;
    let prompt = advisor::question_prompt(&item, question.trim(), ctx.lang());
    if let Some(answer) = kept(&ctx, setup.reasoning_name, &prompt, fresh) {
        return Ok(answer);
    }
    let (text, seconds) = advisor::ask(&prompt, &setup).map_err(|error| error.to_string())?;
    let answer = Answer {
        text,
        by: setup.reasoning_name.to_string(),
        seconds,
        kept: None,
    };
    Ok(keep(&ctx, &prompt, answer))
}

/// Vario's thermostat, when Vario is installed: "normal" lets Delest ask the agent.
fn thermostat() -> Option<String> {
    let output = Command::new(paths::find_program("vario")?)
        .arg("usage")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("Thermostat"))?;
    Some(line.split(':').nth(1)?.trim().to_string())
}

/// The same question to the developer's agent (Claude Code, its most economical
/// model), on an explicit click, when the quota allows it according to Vario.
#[tauri::command]
pub async fn ask_agent(
    ctx: State<'_, Ctx>,
    path: String,
    question: String,
    fresh: bool,
) -> Result<Answer, String> {
    let lang = ctx.lang();
    let memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    if memory.agent_never {
        return Err(tr!(
            lang,
            "asking the agent is turned off in the settings.",
            "la sollicitation de l'agent est désactivée dans les réglages."
        ));
    }
    // An answer kept from before costs no quota: it comes before Vario's thermostat.
    let item = item_for(&ctx, &path)?;
    let prompt = advisor::question_prompt(&item, question.trim(), lang);
    let by = format!("Claude Code ({AGENT_MODEL})");
    if let Some(answer) = kept(&ctx, &by, &prompt, fresh) {
        return Ok(answer);
    }
    if let Some(state) = thermostat().filter(|state| state != "normal") {
        return Err(tr!(
            lang,
            "Vario's thermostat is at {state}: the agent is spared for now.",
            "le thermostat de Vario est à {state} : l'agent est ménagé pour l'instant."
        ));
    }
    let not_found = || {
        tr!(
            lang,
            "the claude command is not installed.",
            "la commande claude n'est pas installée."
        )
    };
    let claude = paths::find_program("claude").ok_or_else(not_found)?;
    let started = Instant::now();
    let output = Command::new(claude)
        .args(["-p", "--model", AGENT_MODEL, "--output-format", "text"])
        .arg(&prompt)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| not_found())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let answer = Answer {
        text: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        by,
        seconds: started.elapsed().as_secs_f32(),
        kept: None,
    };
    Ok(keep(&ctx, &prompt, answer))
}

#[derive(Serialize)]
pub struct ModelLine {
    name: String,
    role: String,
    installed: bool,
    size_shown: String,
}

#[derive(Serialize)]
pub struct AiSettings {
    models: Vec<ModelLine>,
    store: String,
    engine: bool,
    free_memory: String,
    battery: Option<u32>,
    reasoning: &'static str,
    agent_never: bool,
    agent_available: bool,
    thermostat: Option<String>,
    /// The engine or one of the two models is missing: the button to install them shows.
    missing: bool,
    /// Why no model can run now, with the figures: memory needed and free, or battery.
    capacity_note: Option<String>,
}

#[tauri::command]
pub async fn ai_settings(ctx: State<'_, Ctx>) -> Result<AiSettings, String> {
    let store = models::store_dir(&ctx.home);
    let manifest = Manifest::load(&store).unwrap_or_default();
    let memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    let available = capacities::available_memory();
    let battery = capacities::on_battery();
    Ok(AiSettings {
        models: CATALOG
            .iter()
            .map(|known| {
                let installed = manifest.path(&store, known.name).is_some();
                let size = manifest
                    .models
                    .get(known.name)
                    .map(|model| model.size)
                    .unwrap_or(0);
                ModelLine {
                    name: known.name.to_string(),
                    role: known.role.to_string(),
                    installed,
                    size_shown: if installed {
                        ctx.size(size)
                    } else {
                        String::new()
                    },
                }
            })
            .collect(),
        store: ctx.short(&store),
        engine: engine::server_program(&store).is_some(),
        capacity_note: match advisor::setup(&ctx.home) {
            Err(why @ (advisor::Missing::Memory { .. } | advisor::Missing::Battery(_))) => {
                Some(why.text(ctx.lang()))
            }
            _ => None,
        },
        missing: engine::server_program(&store).is_none()
            || install::MODELS
                .iter()
                .any(|name| manifest.path(&store, name).is_none()),
        free_memory: available.map(|bytes| ctx.size(bytes)).unwrap_or_default(),
        battery,
        reasoning: match capacities::reasoning(available, battery) {
            Reasoning::Main => "main",
            Reasoning::Fallback => "fallback",
            Reasoning::None => "none",
        },
        agent_never: memory.agent_never,
        agent_available: paths::find_program("claude").is_some_and(|claude| {
            Command::new(claude)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        }),
        thermostat: thermostat(),
    })
}

#[tauri::command]
pub async fn set_agent(ctx: State<'_, Ctx>, never: bool) -> Result<(), String> {
    let mut memory = Memory::load(&ctx.data_dir).map_err(|error| error.to_string())?;
    memory.agent_never = never;
    memory
        .save(&ctx.data_dir)
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
pub struct JournalRow {
    when: String,
    path: String,
    size_shown: String,
    nature: String,
    advice: Advice,
    model: String,
    decision: String,
}

/// Every opinion given, with what the developer decided afterwards.
#[tauri::command]
pub async fn journal(ctx: State<'_, Ctx>) -> Result<Vec<JournalRow>, String> {
    let lang = ctx.lang();
    let opinions = Opinions::load(&ctx.data_dir);
    let records = journal::read(&ctx.data_dir).unwrap_or_default();
    let memory = Memory::load(&ctx.data_dir).unwrap_or_default();
    let arbiter = ctx.arbiter()?;
    let mut rows: Vec<(i64, JournalRow)> = opinions
        .folders
        .values()
        .map(|opinion| {
            let moved = records
                .iter()
                .any(|record| record.action == Action::Quarantine && record.path == opinion.path);
            let refused = memory.refusals_of(&opinion.path);
            let decision = if moved {
                tr!(lang, "moved into the quarantine", "mis en quarantaine")
            } else if refused > 0 {
                tr!(lang, "refused {refused} times", "refusé {refused} fois")
            } else if !opinion.path.exists() {
                tr!(lang, "no longer on the disk", "n'est plus sur le disque")
            } else if arbiter.assess(&opinion.path).verdict == Verdict::Protected {
                tr!(
                    lang,
                    "now protected by a rule",
                    "désormais protégé par une règle"
                )
            } else {
                tr!(lang, "not decided yet", "pas encore décidé")
            };
            let when = chrono::DateTime::from_timestamp(opinion.at, 0)
                .map(|time| {
                    time.with_timezone(&chrono::Local)
                        .format("%d/%m %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            (
                opinion.at,
                JournalRow {
                    when,
                    path: ctx.short(&opinion.path),
                    size_shown: ctx.size(opinion.size),
                    nature: opinion.nature.clone(),
                    advice: opinion.advice,
                    model: opinion.model.clone(),
                    decision,
                },
            )
        })
        .collect();
    rows.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    Ok(rows.into_iter().map(|(_, row)| row).collect())
}

// ------------------------------------------------------------------ Installing the local AI

/// The installation running in the background, read by `ai_install_status`.
#[derive(Default)]
pub struct Setup {
    steps: Vec<Step>,
    /// The step under way; `steps.len()` once all are done.
    current: usize,
    running: bool,
    error: Option<String>,
}

fn label(step: &Step) -> String {
    match &step.what {
        What::Engine(_) => format!("llama.cpp {}", install::ENGINE_RELEASE),
        What::Model { known, .. } => match known.name {
            "embeddinggemma-2" => "EmbeddingGemma 2".to_string(),
            "gemma-4-e2b" => "Gemma 4 E2B".to_string(),
            other => other.to_string(),
        },
    }
}

/// The steps from the models' caches of other tools only: walking the scanned
/// folders for a copy would take minutes.
fn plan(ctx: &Ctx) -> Result<install::Plan, String> {
    let store = models::store_dir(&ctx.home);
    install::plan(&store, &models::known_caches(&ctx.home)).map_err(|error| error.to_string())
}

#[derive(Serialize)]
pub struct PlanLine {
    label: String,
    /// What travels over the network, shown; empty for a copy found on the disk.
    download_shown: String,
    from_disk: bool,
}

#[derive(Serialize)]
pub struct Plan {
    lines: Vec<PlanLine>,
    total_shown: String,
    /// Why the engine cannot be fetched here, when it is missing.
    engine_note: Option<String>,
}

/// What the installation would fetch, for the developer to confirm.
#[tauri::command]
pub async fn ai_install_plan(ctx: State<'_, Ctx>) -> Result<Plan, String> {
    let plan = plan(&ctx)?;
    let steps = plan.steps;
    let total: u64 = steps.iter().map(|step| step.download).sum();
    Ok(Plan {
        engine_note: plan.engine.map(|why| why.text(ctx.lang())),
        lines: steps
            .iter()
            .map(|step| PlanLine {
                label: label(step),
                download_shown: if step.download > 0 {
                    ctx.size(step.download)
                } else {
                    String::new()
                },
                from_disk: step.download == 0,
            })
            .collect(),
        total_shown: ctx.size(total),
    })
}

/// Starts the installation the developer confirmed; it runs apart, step by step.
#[tauri::command]
pub fn install_ai(app: AppHandle, ctx: State<'_, Ctx>) -> Result<(), String> {
    let steps = plan(&ctx)?.steps;
    {
        let mut setup = ctx
            .setup
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if setup.running {
            return Err(tr!(
                ctx.lang(),
                "the installation is already under way.",
                "l'installation est déjà en cours."
            ));
        }
        *setup = Setup {
            steps: steps.clone(),
            current: 0,
            running: true,
            error: None,
        };
    }
    let store = models::store_dir(&ctx.home);
    std::thread::spawn(move || {
        let ctx = app.state::<Ctx>();
        for (at, step) in steps.iter().enumerate() {
            ctx.setup
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .current = at;
            if let Err(error) = install::run(&store, step) {
                let mut setup = ctx
                    .setup
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                setup.error = Some(format!("{}: {error}", label(step)));
                setup.running = false;
                return;
            }
        }
        let mut setup = ctx
            .setup
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        setup.current = steps.len();
        setup.running = false;
    });
    Ok(())
}

#[derive(Serialize)]
pub struct SetupStatus {
    running: bool,
    finished: bool,
    error: Option<String>,
    /// The step under way, shown.
    step: String,
    /// Downloaded so far and in all, for the bar.
    done: u64,
    total: u64,
    done_shown: String,
    total_shown: String,
}

/// The installation's progress: the steps done, and the size of the file being written.
#[tauri::command]
pub fn ai_install_status(ctx: State<'_, Ctx>) -> SetupStatus {
    let setup = ctx
        .setup
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let total: u64 = setup.steps.iter().map(|step| step.download).sum();
    let before: u64 = setup
        .steps
        .iter()
        .take(setup.current)
        .map(|step| step.download)
        .sum();
    let under_way = setup.steps.get(setup.current).map_or(0, |step| {
        std::fs::metadata(&step.part)
            .map(|metadata| metadata.len().min(step.download))
            .unwrap_or(0)
    });
    let done = if setup.current >= setup.steps.len() {
        total
    } else {
        before + under_way
    };
    SetupStatus {
        running: setup.running,
        finished: !setup.running && setup.error.is_none() && setup.current >= setup.steps.len(),
        error: setup.error.clone(),
        step: setup
            .steps
            .get(setup.current)
            .map(label)
            .unwrap_or_default(),
        done,
        total,
        done_shown: ctx.size(done),
        total_shown: ctx.size(total),
    }
}
