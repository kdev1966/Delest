//! The `delest` command: find what can be freed on a developer's disk, and why.

mod actions;
mod integration;
mod mcp;
mod models_cmd;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use clap::{Parser, Subcommand};
use delest_core::acceptance::{self, Case};
use delest_core::activity::Handoff;
use delest_core::arbiter::Arbiter;
use delest_core::index::Index;
use delest_core::lang::{self, Lang};
use delest_core::memory::Memory;
use delest_core::verdict::{Assessment, Verdict};
use delest_core::{home_dir, paths, scan, tilde, tr};

#[derive(Parser)]
#[command(
    version,
    about = "Free the disk space taken by rebuildable artefacts, without touching your work.",
    after_help = concat!("© 2026 ", env!("CARGO_PKG_AUTHORS"), " · MIT · ", env!("CARGO_PKG_REPOSITORY"))
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum ModelsAction {
    /// The models Delest knows, and which are installed.
    List,
    /// Print the file of an installed model, for any program or script.
    Path { name: String },
    /// Install a model: a copy already on the disk is cloned first, else it is downloaded.
    Add {
        name: String,
        /// Also look for a copy in these folders.
        #[arg(long = "search", value_name = "FOLDER")]
        search: Vec<PathBuf>,
        /// Do not ask before downloading.
        #[arg(short, long)]
        yes: bool,
    },
    /// List identical model files (GGUF, safetensors, ONNX); with --replace, make the copies APFS clones.
    Dedupe {
        paths: Vec<PathBuf>,
        /// Replace each copy by a clone of one reference, after a preview and a confirmation.
        #[arg(long)]
        replace: bool,
        /// Do not ask before replacing.
        #[arg(short, long)]
        yes: bool,
    },
    /// Install the llama.cpp server for the model's opinions: this system's official
    /// build (checked against its fingerprint), or a llama-server you give.
    Engine { program: Option<PathBuf> },
}

#[derive(Subcommand)]
enum Command {
    /// List what can be freed below these folders, with the reason for each verdict.
    Scan {
        /// Folders to scan; the current folder by default.
        paths: Vec<PathBuf>,
        /// Also measure the global caches (npm, Cargo, Gradle, Hugging Face...).
        #[arg(long)]
        caches: bool,
        /// Also show what is protected or unknown.
        #[arg(long)]
        all: bool,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Print nothing: only refresh the index (used by the hook).
        #[arg(long, conflicts_with = "json")]
        quiet: bool,
    },
    /// Move what can be freed into the quarantine, after a preview and a confirmation.
    Clean {
        /// Folders to clean; the current folder by default.
        paths: Vec<PathBuf>,
        /// Also clean the global caches (npm, Cargo, Gradle...).
        #[arg(long)]
        caches: bool,
        /// Only these kinds of artefacts, by name: --only target,build,.next
        #[arg(long, value_delimiter = ',', value_name = "NAMES")]
        only: Vec<String>,
        /// Ask about each item to review, one by one.
        #[arg(long)]
        review: bool,
        /// Act as if there were no network: hold back what needs it to be rebuilt.
        #[arg(long)]
        offline: bool,
        /// Show what would be moved without moving it.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation (items to review are still asked about).
        #[arg(short, long)]
        yes: bool,
    },
    /// Put an item back from the quarantine; the most recent one by default.
    Restore {
        /// Entry to put back, as listed by --list.
        id: Option<String>,
        /// List the quarantine instead.
        #[arg(long, conflicts_with = "id")]
        list: bool,
        /// Do not ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Delete quarantine entries now, to free their space: only what rebuilds locally.
    Purge {
        /// Entry to delete, as listed by `restore --list`; every eligible entry by default.
        id: Option<String>,
        /// Show what would be deleted without deleting it.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Check what Delest relies on: git, its data folder, the quarantine, the network.
    Doctor,
    /// Free space, what the last scan found and the quarantine, without scanning again.
    Status,
    /// Add Delest's hook to the AI agents found (Claude Code): it checks the disk at each session.
    Install {
        /// Show the changes without making them.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove Delest's hook from the agents.
    Remove {
        /// Show the changes without making them.
        #[arg(long)]
        dry_run: bool,
        /// Do not ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// The optional check every four hours, for work done without an agent; off by default.
    Timer {
        #[arg(value_parser = ["on", "off", "status"], default_value = "status")]
        action: String,
    },
    /// Show what the new rules in shadow would offer; `accept <rule>` takes one out once the corpus passes.
    Shadow {
        #[arg(value_parser = ["accept"])]
        action: Option<String>,
        /// The rule to accept.
        rule: Option<String>,
        /// The corpus replayed before accepting a rule.
        #[arg(long, default_value = "corpus/poste-reference.toml")]
        corpus: PathBuf,
    },
    /// Serve Delest's read-only tools to AI agents over MCP (standard input and output).
    Mcp,
    /// The model's opinion on the unknown folders of the last scan; never makes one freeable.
    Advise {
        /// Ask again even where an opinion is still valid.
        #[arg(long)]
        refresh: bool,
    },
    /// Ask the local model a question about one folder; nothing is changed.
    Ask { path: PathBuf, question: String },
    /// The shared model store (~/.local/share/kdev-models), for Delest and your other projects.
    Models {
        #[command(subcommand)]
        action: ModelsAction,
    },
    /// Run by the agents at each session; never fails.
    #[command(hide = true)]
    Hook {
        event: String,
        #[arg(long)]
        via: Option<String>,
    },
    /// Never offer this folder or anything below it; lists the protected folders without a path.
    Protect { path: Option<PathBuf> },
    /// Remove a protection set with `protect`.
    Unprotect { path: PathBuf },
    /// Explain the verdict on one folder.
    Explain {
        path: PathBuf,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Replay the acceptance corpus; fails if a protected item comes out freeable.
    Corpus {
        /// The corpus file.
        #[arg(default_value = "corpus/poste-reference.toml")]
        file: PathBuf,
        /// List every disagreement instead of the first ones.
        #[arg(long)]
        details: bool,
        /// Also ask the model about the unknown folders, and score its opinions.
        #[arg(long)]
        advise: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let lang = Lang::detect();
    let home = home_dir();
    let result = context(&cli.command, home).and_then(|arbiter| run(cli.command, &arbiter, lang));
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("delest: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The corpus is replayed with the rules and bans only; every other command also
/// weighs the developer's decisions and the projects' activity.
fn context(command: &Command, home: PathBuf) -> io::Result<Arbiter> {
    // The hook needs no memory, and must not fail if the memory cannot be read.
    if matches!(command, Command::Corpus { .. } | Command::Hook { .. }) {
        return Ok(Arbiter::new(home));
    }
    let memory = Memory::load(&paths::data_dir())?;
    Ok(Arbiter::with_context(home, Handoff::load(), memory))
}

fn run(command: Command, arbiter: &Arbiter, lang: Lang) -> io::Result<ExitCode> {
    match command {
        Command::Scan {
            paths,
            caches,
            all,
            json,
            quiet,
        } => scan_command(arbiter, paths, caches, all, json, quiet, lang),
        Command::Clean {
            paths,
            caches,
            only,
            review,
            offline,
            dry_run,
            yes,
        } => roots(paths).and_then(|roots| {
            let options = actions::CleanOptions {
                caches,
                only,
                review,
                offline,
                dry_run,
                yes,
            };
            actions::clean(arbiter, roots, options, lang)
        }),
        Command::Restore { id, list, yes } => actions::restore(arbiter, id, list, yes, lang),
        Command::Purge { id, dry_run, yes } => actions::purge(arbiter, id, dry_run, yes, lang),
        Command::Doctor => actions::doctor(arbiter, lang),
        Command::Status => actions::status(arbiter, lang),
        Command::Install { dry_run, yes } => integration::install(arbiter, dry_run, yes, lang),
        Command::Remove { dry_run, yes } => integration::remove(arbiter, dry_run, yes, lang),
        Command::Hook { via, .. } => Ok(integration::hook(arbiter, via, lang)),
        Command::Timer { action } => integration::timer_command(arbiter, &action, lang),
        Command::Mcp => mcp::serve(arbiter, lang),
        Command::Advise { refresh } => models_cmd::advise(arbiter, refresh, lang),
        Command::Ask { path, question } => {
            absolute(path).and_then(|path| models_cmd::ask(arbiter, &path, &question, lang))
        }
        Command::Models { action } => match action {
            ModelsAction::List => models_cmd::list(arbiter, lang),
            ModelsAction::Path { name } => models_cmd::path(arbiter, &name, lang),
            ModelsAction::Add { name, search, yes } => {
                models_cmd::add(arbiter, &name, search, yes, lang)
            }
            ModelsAction::Engine { program } => {
                models_cmd::engine(arbiter, program.as_deref(), lang)
            }
            ModelsAction::Dedupe {
                paths,
                replace,
                yes,
            } => roots(paths)
                .and_then(|roots| models_cmd::dedupe(arbiter, roots, replace, yes, lang)),
        },
        Command::Shadow {
            action,
            rule,
            corpus,
        } => match (action, rule) {
            (Some(_), None) => Err(io::Error::other("delest shadow accept <rule>")),
            (_, rule) => integration::shadow(arbiter, rule, &corpus, lang),
        },
        Command::Explain { path, json } => explain(arbiter, &path, json, lang),
        Command::Protect { path } => path
            .map(absolute)
            .transpose()
            .and_then(|path| actions::protect(arbiter, path, lang)),
        Command::Unprotect { path } => actions::unprotect(arbiter, absolute_lenient(path), lang),
        Command::Corpus {
            file,
            details,
            advise,
        } => corpus(arbiter, &file, details, advise, lang),
    }
}

/// Made absolute even if it no longer exists, so a protection can be removed.
fn absolute_lenient(path: PathBuf) -> PathBuf {
    paths::canonical(&path).unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|dir| dir.join(&path))
            .unwrap_or(path)
    })
}

fn absolute(path: PathBuf) -> io::Result<PathBuf> {
    paths::canonical(&path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
}

/// The folders given, made absolute; the current folder when none is given.
fn roots(paths: Vec<PathBuf>) -> io::Result<Vec<PathBuf>> {
    let paths = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths
    };
    paths.into_iter().map(absolute).collect()
}

fn scan_command(
    arbiter: &Arbiter,
    paths: Vec<PathBuf>,
    caches: bool,
    all: bool,
    json: bool,
    quiet: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let roots = roots(paths)?;
    let assessments = scan::scan(&roots, arbiter, caches);
    actions::remember_scan(&paths::data_dir(), &roots, &assessments, lang);
    if quiet {
        return Ok(ExitCode::SUCCESS);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&assessments).map_err(io::Error::other)?
        );
        return Ok(ExitCode::SUCCESS);
    }
    let shown: &[Verdict] = if all {
        &[
            Verdict::Rebuildable,
            Verdict::RebuildOnline,
            Verdict::ToReview,
            Verdict::Protected,
            Verdict::Unknown,
        ]
    } else {
        &[
            Verdict::Rebuildable,
            Verdict::RebuildOnline,
            Verdict::ToReview,
        ]
    };
    for &verdict in shown {
        let group: Vec<&Assessment> = assessments
            .iter()
            .filter(|a| a.verdict == verdict)
            .collect();
        if group.is_empty() {
            continue;
        }
        let total = lang::size(group.iter().filter_map(|a| a.size).sum(), lang);
        let count = group.len();
        let label = verdict.label(lang);
        println!("\n{label} — {total} ({count})");
        for assessment in group {
            print_line(assessment, arbiter.home(), lang);
        }
    }
    let total = |verdict| {
        lang::size(
            assessments
                .iter()
                .filter(|a| a.verdict == verdict)
                .filter_map(|a| a.size)
                .sum(),
            lang,
        )
    };
    let (rebuildable, online, review) = (
        total(Verdict::Rebuildable),
        total(Verdict::RebuildOnline),
        total(Verdict::ToReview),
    );
    println!();
    println!(
        "{}",
        tr!(
            lang,
            "Freeable: {rebuildable}, plus {online} with the network; {review} to review. Nothing was changed.",
            "Supprimable : {rebuildable}, plus {online} avec le réseau ; {review} à juger. Rien n'a été modifié."
        )
    );
    Ok(ExitCode::SUCCESS)
}

fn print_line(assessment: &Assessment, home: &Path, lang: Lang) {
    let size = assessment
        .size
        .map(|bytes| lang::size(bytes, lang))
        .unwrap_or_default();
    println!("  {size:>8}  {}", tilde(&assessment.path, home));
    println!("            {}", assessment.trace(lang));
}

fn explain(arbiter: &Arbiter, path: &Path, json: bool, lang: Lang) -> io::Result<ExitCode> {
    let path = absolute(path.to_path_buf())?;
    let mut assessment = arbiter.assess(&path);
    assessment.size = Some(scan::disk_usage(&path));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&assessment).map_err(io::Error::other)?
        );
        return Ok(ExitCode::SUCCESS);
    }
    let verdict = assessment.verdict.label(lang);
    let category = assessment.category.label(lang);
    println!("{} — {verdict} ({category})", tilde(&path, arbiter.home()));
    for reason in &assessment.reasons {
        println!("  - {}", reason.text(lang));
    }
    if let Some(bytes) = assessment.size {
        let size = lang::size(bytes, lang);
        println!(
            "{}",
            tr!(
                lang,
                "  Size on disk: {size}",
                "  Taille sur le disque : {size}"
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The model's opinion on the corpus's unknown folders, against the reference: a
/// protected folder must get "keep" (or be protected by a ban already), any other
/// "review", the most the model may say. Opinions go to a temporary folder, never
/// into the developer's journal.
fn score_the_model(arbiter: &Arbiter, unknowns: &[&Case], lang: Lang) -> io::Result<()> {
    use delest_core::advisor::{self, Advice};
    let setup =
        advisor::setup(arbiter.home()).map_err(|missing| io::Error::other(missing.text(lang)))?;
    let items: Vec<_> = unknowns
        .iter()
        .filter_map(|case| case.assessment.clone())
        .filter(|assessment| assessment.verdict == Verdict::Unknown && assessment.shadow.is_none())
        .collect();
    let examples: Vec<_> = Index::load(&paths::data_dir())?
        .map(|index| index.items)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| item.verdict != Verdict::Unknown)
        .collect();
    let scratch = std::env::temp_dir().join(format!("delest-corpus-{}", std::process::id()));
    fs::create_dir_all(&scratch)?;
    let count = items.len();
    println!(
        "{}",
        tr!(
            lang,
            "\nThe model's opinion on {count} unknown folders (about 20 s each)…",
            "\nAvis du modèle sur {count} dossiers inconnus (environ 20 s chacun)…"
        )
    );
    let opinions = advisor::advise(&items, &examples, &setup.models(), &scratch, |_| {});
    let _ = fs::remove_dir_all(&scratch);
    let opinions = opinions?;
    let reference = |path: &Path| {
        unknowns
            .iter()
            .find(|case| case.path == path)
            .map(|case| case.reference)
    };
    let (mut right, mut cautious, mut review_on_protected) = (0, 0, 0);
    for opinion in &opinions {
        let Some(reference) = reference(&opinion.path) else {
            continue;
        };
        let keep = opinion.advice == Advice::Keep;
        if (reference == Verdict::Protected) == keep {
            right += 1;
        } else if keep {
            cautious += 1;
        } else {
            review_on_protected += 1;
        }
    }
    let banned = unknowns
        .iter()
        .filter(|case| {
            case.reference == Verdict::Protected
                && case
                    .assessment
                    .as_ref()
                    .is_some_and(|a| a.verdict == Verdict::Protected)
        })
        .count();
    let asked = opinions.len();
    let baseline = unknowns
        .iter()
        .filter(|case| case.reference == Verdict::Protected)
        .count();
    let total = unknowns.len();
    println!(
        "{}",
        tr!(
            lang,
            "Model: {right} of {asked} opinions right; {cautious} kept although freeable (cautious); {review_on_protected} protected folders judged \"to review\". Bans: {banned} protected folders. Without the model: {baseline} of {total} right.",
            "Modèle : {right} avis justes sur {asked} ; {cautious} gardés alors que libérables (prudence) ; {review_on_protected} dossiers protégés jugés « à juger ». Interdits : {banned} dossiers protégés. Sans le modèle : {baseline} sur {total} justes."
        )
    );
    Ok(())
}

fn corpus(
    arbiter: &Arbiter,
    file: &Path,
    details: bool,
    advise: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let text = fs::read_to_string(file)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", file.display())))?;
    let cases = acceptance::replay(&text, arbiter, arbiter.home()).map_err(io::Error::other)?;
    let all_present: Vec<&Case> = cases
        .iter()
        .filter(|case| case.assessment.is_some())
        .collect();
    let (total, missing) = (cases.len(), cases.len() - all_present.len());
    // The unknown folders measure the model, not the rules: they are counted apart.
    let present: Vec<&Case> = all_present
        .iter()
        .copied()
        .filter(|case| !case.is_unknown_folder())
        .collect();
    let agreeing = present.iter().filter(|case| case.agrees()).count();
    let percent = 100 * agreeing / present.len().max(1);
    let checked = present.len();
    println!(
        "{}",
        tr!(
            lang,
            "Corpus: {total} items, {missing} no longer on disk.",
            "Corpus : {total} éléments, dont {missing} absents du disque."
        )
    );
    println!(
        "{}",
        tr!(
            lang,
            "Agreement with the reference: {agreeing} of {checked} ({percent} %).",
            "Accord avec la référence : {agreeing} sur {checked} ({percent} %)."
        )
    );
    for section in ["dossier", "cache", "piege"] {
        let in_section: Vec<&&Case> = present
            .iter()
            .filter(|case| case.section == section)
            .collect();
        let agree = in_section.iter().filter(|case| case.agrees()).count();
        let count = in_section.len();
        println!("  {section:8} {agree}/{count}");
    }
    let unknowns: Vec<&Case> = all_present
        .iter()
        .copied()
        .filter(|case| case.is_unknown_folder())
        .collect();
    if !unknowns.is_empty() {
        let count = unknowns.len();
        let verdict = |case: &Case| {
            case.assessment
                .as_ref()
                .map(|assessment| assessment.verdict)
        };
        let banned = unknowns
            .iter()
            .filter(|case| verdict(case) == Some(Verdict::Protected))
            .count();
        let shadow = unknowns
            .iter()
            .filter(|case| case.assessment.as_ref().is_some_and(|a| a.shadow.is_some()))
            .count();
        let left = unknowns
            .iter()
            .filter(|case| {
                case.assessment
                    .as_ref()
                    .is_some_and(|a| a.verdict == Verdict::Unknown && a.shadow.is_none())
            })
            .count();
        println!(
            "{}",
            tr!(
                lang,
                "  unknown  {count} folders: {banned} protected by a ban, {shadow} recognised by a rule in shadow, {left} left to the model",
                "  inconnu  {count} dossiers : {banned} protégés par un interdit, {shadow} reconnus par une règle en ombre, {left} laissés au modèle"
            )
        );
    }
    let disagreements: Vec<&&Case> = present.iter().filter(|case| !case.agrees()).collect();
    let limit = if details { disagreements.len() } else { 15 };
    if !disagreements.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "\nDisagreements (reference → Delest):",
                "\nDésaccords (référence → Delest) :"
            )
        );
        for case in disagreements.iter().take(limit) {
            print_case(case, arbiter.home(), lang);
        }
        if disagreements.len() > limit {
            let more = disagreements.len() - limit;
            println!(
                "{}",
                tr!(
                    lang,
                    "  … and {more} more (--details).",
                    "  … et {more} autres (--details)."
                )
            );
        }
    }
    if advise && !unknowns.is_empty() {
        score_the_model(arbiter, &unknowns, lang)?;
    }
    let blocking: Vec<&&Case> = all_present
        .iter()
        .filter(|case| case.is_blocking())
        .collect();
    if blocking.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "\nBlocking criterion met: no protected item comes out freeable.",
                "\nCritère bloquant respecté : aucun élément protégé n'est classé supprimable."
            )
        );
        Ok(ExitCode::SUCCESS)
    } else {
        let count = blocking.len();
        println!(
            "{}",
            tr!(
                lang,
                "\nBLOCKING: {count} protected items come out freeable:",
                "\nBLOQUANT : {count} éléments protégés sont classés supprimables :"
            )
        );
        for case in blocking {
            print_case(case, arbiter.home(), lang);
        }
        Ok(ExitCode::FAILURE)
    }
}

fn print_case(case: &Case, home: &Path, lang: Lang) {
    let Some(assessment) = &case.assessment else {
        return;
    };
    let reference = case.reference.label(lang);
    let got = assessment.verdict.label(lang);
    println!("  {reference} → {got}  {}", tilde(&case.path, home));
    println!("      {}", assessment.trace(lang));
}
