//! The commands that change the disk (`clean`, `restore`) and `doctor`, which
//! checks what they rely on.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use delest_core::alerts;
use delest_core::arbiter::Arbiter;
use delest_core::capacities;
use delest_core::clean::{self, Skip};
use delest_core::index::Index;
use delest_core::journal::{self, Action, Record};
use delest_core::lang::{self, Lang};
use delest_core::memory::Memory;
use delest_core::processes;
use delest_core::quarantine::{self, Quarantined};
use delest_core::verdict::Assessment;
use delest_core::volume::{self, Level};
use delest_core::{paths, scan, tilde, timer, tr};

pub struct CleanOptions {
    pub caches: bool,
    /// Only these kinds of artefacts, by rule name (`target`, `build`, `.next`…); all when empty.
    pub only: Vec<String>,
    pub review: bool,
    pub offline: bool,
    pub dry_run: bool,
    pub yes: bool,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn total(items: &[Assessment]) -> u64 {
    items.iter().filter_map(|item| item.size).sum()
}

/// "1 item", "3 items".
fn plural(count: usize, lang: Lang, en: (&str, &str), fr: (&str, &str)) -> String {
    let (one, many) = match lang {
        Lang::En => en,
        Lang::Fr => fr,
    };
    format!("{count} {}", if count == 1 { one } else { many })
}

fn items(count: usize, lang: Lang) -> String {
    plural(count, lang, ("item", "items"), ("élément", "éléments"))
}

/// Asks a yes/no question; anything but yes is no.
pub fn confirm(question: &str, lang: Lang) -> io::Result<bool> {
    let hint = tr!(lang, "[y/N]", "[o/N]");
    print!("{question} {hint} ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_lowercase().as_str(),
        "y" | "yes" | "o" | "oui"
    ))
}

fn print_item(item: &Assessment, home: &Path, lang: Lang) {
    let size = item
        .size
        .map(|bytes| lang::size(bytes, lang))
        .unwrap_or_default();
    println!("  {size:>8}  {}", tilde(&item.path, home));
    println!("            {}", item.trace(lang));
}

pub fn clean(
    arbiter: &Arbiter,
    roots: Vec<PathBuf>,
    options: CleanOptions,
    lang: Lang,
) -> io::Result<ExitCode> {
    let interactive = io::stdin().is_terminal();
    if options.review && !interactive {
        let message = tr!(
            lang,
            "--review asks about each item one by one: it needs a terminal.",
            "--review demande l'accord pour chaque élément : il faut un terminal."
        );
        return Err(io::Error::other(message));
    }
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let mut memory = Memory::load(&data_dir)?;
    let online = !options.offline && clean::online();
    let mut found = scan::scan(&roots, arbiter, options.caches);
    remember_scan(&data_dir, &roots, &found, lang);
    if !options.only.is_empty() {
        found.retain(|item| {
            item.rule
                .as_ref()
                .is_some_and(|rule| options.only.contains(rule))
        });
    }
    let plan = clean::plan(found, online);

    let mut selected = plan.selected;
    if !selected.is_empty() {
        let size = lang::size(total(&selected), lang);
        println!(
            "{}",
            tr!(
                lang,
                "To move into the quarantine — {size}:",
                "À mettre en quarantaine — {size} :"
            )
        );
        for item in &selected {
            print_item(item, home, lang);
        }
    }
    if !plan.held_offline.is_empty() {
        let (count, size) = (
            items(plan.held_offline.len(), lang),
            lang::size(total(&plan.held_offline), lang),
        );
        println!(
            "{}",
            tr!(
                lang,
                "\nHeld back, no network: {count} ({size}) that need it to be rebuilt.",
                "\nRetenus faute de réseau : {count} ({size}) qui en ont besoin pour se reconstruire."
            )
        );
    }
    if !plan.kept_active.is_empty() {
        let (count, size) = (
            items(plan.kept_active.len(), lang),
            lang::size(total(&plan.kept_active), lang),
        );
        let days = delest_core::activity::INACTIVE_AFTER_DAYS;
        println!(
            "{}",
            tr!(
                lang,
                "\nKept: {count} ({size}) of projects worked on in the last {days} days.",
                "\nGardés : {count} ({size}) de projets travaillés ces {days} derniers jours."
            )
        );
    }
    if !plan.to_review.is_empty() && options.review {
        println!(
            "{}",
            tr!(lang, "\nTo review, one by one:", "\nÀ juger, un par un :")
        );
        let mut refused = false;
        for item in plan.to_review {
            print_item(&item, home, lang);
            if options.dry_run {
                continue;
            }
            let question = tr!(
                lang,
                "  Move it into the quarantine?",
                "  Le mettre en quarantaine ?"
            );
            if confirm(&question, lang)? {
                selected.push(item);
            } else {
                // Refused three times, it is no longer offered.
                memory.refuse(&item.path);
                refused = true;
            }
        }
        if refused {
            memory.save(&data_dir)?;
        }
    } else if !plan.to_review.is_empty() {
        let (count, size) = (
            items(plan.to_review.len(), lang),
            lang::size(total(&plan.to_review), lang),
        );
        println!(
            "{}",
            tr!(
                lang,
                "\nTo review: {count} ({size}), left out; --review asks about each one.",
                "\nÀ juger : {count} ({size}), laissés de côté ; --review demande pour chacun."
            )
        );
    }
    if selected.is_empty() {
        println!("{}", tr!(lang, "\nNothing to move.", "\nRien à déplacer."));
        return Ok(ExitCode::SUCCESS);
    }
    let (count, size) = (
        items(selected.len(), lang),
        lang::size(total(&selected), lang),
    );
    if options.dry_run {
        println!(
            "{}",
            tr!(
                lang,
                "\nPreview only: nothing was moved.",
                "\nAperçu seulement : rien n'a été déplacé."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    if !options.yes {
        if !interactive {
            let message = tr!(
                lang,
                "no terminal to confirm: add --yes.",
                "pas de terminal pour confirmer : ajoutez --yes."
            );
            return Err(io::Error::other(message));
        }
        let days = quarantine::RETENTION_DAYS;
        let question = tr!(
            lang,
            "\nMove {count} ({size}) into the quarantine? They can be restored for {days} days.",
            "\nMettre {count} ({size}) en quarantaine ? Ils restent restaurables {days} jours."
        );
        if !confirm(&question, lang)? {
            println!(
                "{}",
                tr!(lang, "Nothing was moved.", "Rien n'a été déplacé.")
            );
            return Ok(ExitCode::SUCCESS);
        }
    }

    let outcome = clean::execute(&selected, arbiter, &data_dir, now());
    let moved: u64 = outcome.moved.iter().map(Quarantined::size).sum();
    let (count, size) = (items(outcome.moved.len(), lang), lang::size(moved, lang));
    println!(
        "{}",
        tr!(
            lang,
            "\nMoved into the quarantine: {count} ({size}).",
            "\nMis en quarantaine : {count} ({size})."
        )
    );
    for (item, skip) in &outcome.skipped {
        let path = tilde(&item.path, home);
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
            Skip::InUse { program } => tr!(lang, "in use by {program}", "utilisé par {program}"),
            Skip::Failed(error) => error.text(lang),
        };
        println!(
            "{}",
            tr!(
                lang,
                "  Left in place: {path} ({why})",
                "  Laissé en place : {path} ({why})"
            )
        );
    }
    if !outcome.purged.is_empty() {
        let freed: u64 = outcome.purged.iter().map(Quarantined::size).sum();
        let (count, size) = (items(outcome.purged.len(), lang), lang::size(freed, lang));
        println!(
            "{}",
            tr!(
                lang,
                "Deleted from the quarantine (past their time or over the cap): {count} ({size}).",
                "Supprimés de la quarantaine (délai écoulé ou plafond atteint) : {count} ({size})."
            )
        );
    }
    if outcome.use_unchecked {
        eprintln!(
            "{}",
            tr!(
                lang,
                "delest: lsof did not answer: whether folders were in use could not be checked.",
                "delest : lsof n'a pas répondu : l'utilisation des dossiers n'a pas pu être vérifiée."
            )
        );
    }
    if outcome.journal_errors > 0 {
        let errors = outcome.journal_errors;
        let file = journal::path(&data_dir).display().to_string();
        eprintln!(
            "{}",
            tr!(
                lang,
                "delest: {errors} actions could not be journaled in {file}.",
                "delest : {errors} actions n'ont pas pu être journalisées dans {file}."
            )
        );
    }
    println!(
        "{}",
        tr!(
            lang,
            "To put something back: delest restore --list",
            "Pour remettre un élément en place : delest restore --list"
        )
    );
    Ok(
        if outcome
            .skipped
            .iter()
            .any(|(_, skip)| matches!(skip, Skip::Failed(_)))
        {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        },
    )
}

pub fn restore(
    arbiter: &Arbiter,
    id: Option<String>,
    list: bool,
    yes: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let entries = quarantine::list(home, &data_dir);
    let now = now();
    if list {
        if entries.is_empty() {
            println!(
                "{}",
                tr!(lang, "The quarantine is empty.", "La quarantaine est vide.")
            );
        }
        for entry in &entries {
            let (id, age, size) = (
                entry.id(),
                entry.age_days(now),
                lang::size(entry.size(), lang),
            );
            let path = tilde(&entry.entry.original, home);
            println!(
                "{}",
                tr!(
                    lang,
                    "  {id}  {size:>8}  {age} d  {path}",
                    "  {id}  {size:>8}  {age} j  {path}"
                )
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let chosen = match &id {
        Some(id) => entries.iter().find(|entry| &entry.id() == id),
        None => entries.first(),
    };
    let Some(chosen) = chosen else {
        let message = match id {
            Some(id) => tr!(
                lang,
                "no quarantine entry {id} (delest restore --list).",
                "aucune entrée {id} en quarantaine (delest restore --list)."
            ),
            None => tr!(lang, "the quarantine is empty.", "la quarantaine est vide."),
        };
        return Err(io::Error::other(message));
    };
    let path = tilde(&chosen.entry.original, home);
    if !yes {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other(tr!(
                lang,
                "no terminal to confirm: add --yes.",
                "pas de terminal pour confirmer : ajoutez --yes."
            )));
        }
        if !confirm(
            &tr!(lang, "Put {path} back?", "Remettre {path} en place ?"),
            lang,
        )? {
            return Ok(ExitCode::SUCCESS);
        }
    }
    quarantine::restore(chosen).map_err(|error| io::Error::other(error.text(lang)))?;
    // A restore is a mistake of the rule that offered the item: it makes Delest more cautious.
    let mut memory = Memory::load(&data_dir)?;
    memory.record_restore(
        &chosen.entry.original,
        chosen.entry.assessment.rule.as_deref(),
        now,
    );
    memory.save(&data_dir)?;
    let record = Record::now(
        Action::Restore,
        &chosen.entry.original,
        &chosen.dir,
        &chosen.entry.assessment,
    );
    if journal::append(&data_dir, &record).is_err() {
        eprintln!(
            "{}",
            tr!(
                lang,
                "delest: the restore could not be journaled.",
                "delest : la restauration n'a pas pu être journalisée."
            )
        );
    }
    println!(
        "{}",
        tr!(lang, "Put back: {path}", "Remis en place : {path}")
    );
    Ok(ExitCode::SUCCESS)
}

pub fn purge(
    arbiter: &Arbiter,
    id: Option<String>,
    dry_run: bool,
    yes: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let entries = quarantine::list(home, &data_dir);
    let chosen: Vec<&Quarantined> = match &id {
        Some(id) => {
            let Some(entry) = entries.iter().find(|entry| &entry.id() == id) else {
                return Err(io::Error::other(tr!(
                    lang,
                    "no quarantine entry {id} (delest restore --list).",
                    "aucune entrée {id} en quarantaine (delest restore --list)."
                )));
            };
            vec![entry]
        }
        None => entries.iter().collect(),
    };
    let (mut eligible, kept): (Vec<&Quarantined>, Vec<&Quarantined>) = chosen
        .into_iter()
        .partition(|entry| quarantine::may_purge_early(entry));
    eligible.sort_by_key(|entry| std::cmp::Reverse(entry.size()));
    for entry in &kept {
        let path = tilde(&entry.entry.original, home);
        let verdict = entry.entry.assessment.verdict.label(lang);
        let days = quarantine::RETENTION_DAYS;
        println!(
            "{}",
            tr!(
                lang,
                "  Kept for its {days} days: {path} ({verdict})",
                "  Gardé ses {days} jours : {path} ({verdict})"
            )
        );
    }
    if eligible.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "Nothing to delete now.",
                "Rien à supprimer maintenant."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    let freed: u64 = eligible.iter().map(|entry| entry.size()).sum();
    let (count, size) = (items(eligible.len(), lang), lang::size(freed, lang));
    println!(
        "{}",
        tr!(
            lang,
            "To delete for good — {count} ({size}), rebuilt locally:",
            "À supprimer définitivement — {count} ({size}), reconstruits localement :"
        )
    );
    for entry in &eligible {
        let size = lang::size(entry.size(), lang);
        println!("  {size:>8}  {}", tilde(&entry.entry.original, home));
    }
    if dry_run {
        println!(
            "{}",
            tr!(
                lang,
                "Preview only: nothing was deleted.",
                "Aperçu seulement : rien n'a été supprimé."
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
        let question = tr!(
            lang,
            "Delete them for good? They can no longer be restored.",
            "Les supprimer définitivement ? Ils ne pourront plus être restaurés."
        );
        if !confirm(&question, lang)? {
            println!(
                "{}",
                tr!(lang, "Nothing was deleted.", "Rien n'a été supprimé.")
            );
            return Ok(ExitCode::SUCCESS);
        }
    }
    let (mut failed, mut freed) = (0, 0);
    for entry in eligible {
        match quarantine::purge(entry) {
            Ok(()) => {
                freed += entry.size();
                let record = Record::now(
                    Action::Purge,
                    &entry.entry.original,
                    &entry.dir,
                    &entry.entry.assessment,
                );
                if journal::append(&data_dir, &record).is_err() {
                    eprintln!(
                        "{}",
                        tr!(
                            lang,
                            "delest: a deletion could not be journaled.",
                            "delest : une suppression n'a pas pu être journalisée."
                        )
                    );
                }
            }
            Err(error) => {
                failed += 1;
                let dir = entry.dir.display();
                eprintln!("delest: {dir}: {error}");
            }
        }
    }
    let size = lang::size(freed, lang);
    println!("{}", tr!(lang, "Freed: {size}.", "Libéré : {size}."));
    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Protects a path, with everything below it: an absolute ban, until `unprotect`.
pub fn protect(arbiter: &Arbiter, path: Option<PathBuf>, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let mut memory = Memory::load(&data_dir)?;
    let Some(path) = path else {
        if memory.protected.is_empty() {
            println!(
                "{}",
                tr!(
                    lang,
                    "Nothing is protected yet.",
                    "Rien n'est encore protégé."
                )
            );
        }
        for path in &memory.protected {
            println!("  {}", tilde(path, home));
        }
        return Ok(ExitCode::SUCCESS);
    };
    let shown = tilde(&path, home);
    if memory.protected.insert(path) {
        memory.save(&data_dir)?;
        println!(
            "{}",
            tr!(
                lang,
                "Protected: {shown}. Delest will never offer it.",
                "Protégé : {shown}. Delest ne le proposera jamais."
            )
        );
    } else {
        println!(
            "{}",
            tr!(
                lang,
                "Already protected: {shown}.",
                "Déjà protégé : {shown}."
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub fn unprotect(arbiter: &Arbiter, path: PathBuf, lang: Lang) -> io::Result<ExitCode> {
    let data_dir = paths::data_dir();
    let mut memory = Memory::load(&data_dir)?;
    let shown = tilde(&path, arbiter.home());
    if memory.protected.remove(&path) {
        memory.save(&data_dir)?;
        println!(
            "{}",
            tr!(
                lang,
                "No longer protected: {shown}.",
                "N'est plus protégé : {shown}."
            )
        );
        Ok(ExitCode::SUCCESS)
    } else {
        Err(io::Error::other(tr!(
            lang,
            "{shown} was not protected (delest protect lists them).",
            "{shown} n'était pas protégé (delest protect les liste)."
        )))
    }
}

/// Keeps the scan in the index; a failure only costs `status` its figures.
pub fn remember_scan(data_dir: &Path, roots: &[PathBuf], items: &[Assessment], lang: Lang) {
    let saved = Index::load(data_dir)
        .map(|previous| Index::update(previous, roots, items, now()))
        .and_then(|index| index.save(data_dir));
    if let Err(error) = saved {
        eprintln!(
            "{}",
            tr!(
                lang,
                "delest: the scan index could not be saved ({error}).",
                "delest : l'index du scan n'a pas pu être enregistré ({error})."
            )
        );
    }
}

/// Free space, what the last scan found and the quarantine, without scanning again.
pub fn status(arbiter: &Arbiter, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let index = Index::load(&data_dir)?;
    let totals = index.as_ref().map(Index::totals).unwrap_or_default();
    let now = now();

    let Some(space) = volume::space(home) else {
        return Err(io::Error::other(tr!(
            lang,
            "df did not report the free space.",
            "df n'a pas donné l'espace libre."
        )));
    };
    let _ = delest_core::history::record(&data_dir, space, now);
    let (available, total) = (
        lang::size(space.available, lang),
        lang::size(space.total, lang),
    );
    let percent = space.available * 100 / space.total.max(1);
    let level = volume::level(space, totals.rebuildable + totals.online);
    let state = match level {
        Level::Alert => tr!(
            lang,
            "alert: the disk is getting full",
            "alerte : le disque se remplit"
        ),
        Level::Notice => tr!(lang, "space could be freed", "de l'espace peut être libéré"),
        Level::Fine => tr!(lang, "fine", "rien à signaler"),
    };
    println!(
        "{}",
        tr!(
            lang,
            "Disk: {available} free of {total} ({percent} %), {state}.",
            "Disque : {available} libres sur {total} ({percent} %), {state}."
        )
    );

    match &index {
        None => println!(
            "{}",
            tr!(
                lang,
                "No scan yet: delest scan <folders>.",
                "Aucun scan encore : delest scan <dossiers>."
            )
        ),
        Some(index) => {
            let days = (now - index.scanned_at).max(0) / 86_400;
            let roots = index
                .roots
                .iter()
                .map(|root| tilde(root, home))
                .collect::<Vec<_>>()
                .join(", ");
            let (rebuildable, online, review) = (
                lang::size(totals.rebuildable, lang),
                lang::size(totals.online, lang),
                lang::size(totals.to_review, lang),
            );
            println!("{}", {
                let ago = lang::ago(days.max(0) as u64, lang);
                tr!(
                    lang,
                    "Last scan {ago}, of {roots}:",
                    "Dernier scan {ago}, de {roots} :"
                )
            });
            println!(
                "{}",
                tr!(
                    lang,
                    "  freeable {rebuildable}, freeable online {online}, to review {review}",
                    "  supprimable {rebuildable}, avec réseau {online}, à juger {review}"
                )
            );
        }
    }

    let entries = quarantine::list(home, &data_dir);
    if !entries.is_empty() {
        let count = plural(
            entries.len(),
            lang,
            ("entry", "entries"),
            ("entrée", "entrées"),
        );
        let size = lang::size(entries.iter().map(Quarantined::size).sum(), lang);
        let oldest = entries
            .iter()
            .map(|entry| entry.age_days(now))
            .max()
            .unwrap_or(0);
        let left = (quarantine::RETENTION_DAYS - oldest).max(0);
        println!(
            "{}",
            tr!(
                lang,
                "Quarantine: {count} ({size}); the oldest is deleted in {left} days.",
                "Quarantaine : {count} ({size}) ; la plus ancienne est supprimée dans {left} jours."
            )
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn found(program: &str, argument: &str) -> bool {
    Command::new(program)
        .arg(argument)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn doctor(arbiter: &Arbiter, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let mut healthy = true;
    let mark = |ok: bool| if ok { "✓" } else { "✗" };

    let git = found("git", "--version");
    healthy &= git;
    println!(
        "{} {}",
        mark(git),
        tr!(
            lang,
            "git answers (needed for the bans on tracked work)",
            "git répond (nécessaire aux interdits sur le travail suivi)"
        )
    );

    let data = data_dir.display().to_string();
    let records = journal::read(&data_dir).map(|records| records.len());
    let readable = records.is_ok();
    healthy &= readable;
    let count = plural(
        records.unwrap_or(0),
        lang,
        ("journaled action", "journaled actions"),
        ("action journalisée", "actions journalisées"),
    );
    println!(
        "{} {}",
        mark(readable),
        tr!(
            lang,
            "data folder {data}, {count}",
            "dossier de données {data}, {count}"
        )
    );

    let entries = quarantine::list(home, &data_dir);
    let now = now();
    let due = quarantine::due_for_purge(&entries, now, quarantine::volume_size).len();
    let size = lang::size(entries.iter().map(Quarantined::size).sum(), lang);
    let count = plural(
        entries.len(),
        lang,
        ("entry", "entries"),
        ("entrée", "entrées"),
    );
    println!(
        "✓ {}",
        tr!(
            lang,
            "quarantine: {count} ({size}), {due} to delete at the next clean",
            "quarantaine : {count} ({size}), {due} à supprimer au prochain nettoyage"
        )
    );

    let online = clean::online();
    let network = if online {
        tr!(
            lang,
            "network reachable: what rebuilds from it can be offered",
            "réseau joignable : ce qui s'en reconstruit peut être proposé"
        )
    } else {
        tr!(
            lang,
            "no network: what rebuilds from it is held back",
            "pas de réseau : ce qui s'en reconstruit est retenu"
        )
    };
    println!("{} {network}", if online { "✓" } else { "•" });

    // What this system lets Delest do: measure the disk, see the folders in use,
    // check on its own, tell the developer outside the agent's session.
    match volume::space(home) {
        Some(space) => {
            let (free, total) = (
                lang::size(space.available, lang),
                lang::size(space.total, lang),
            );
            println!(
                "✓ {}",
                tr!(
                    lang,
                    "free space measured: {free} of {total}",
                    "espace libre mesuré : {free} sur {total}"
                )
            );
        }
        None => {
            healthy = false;
            println!(
                "✗ {}",
                tr!(
                    lang,
                    "free space unreadable: no alert can be given",
                    "espace libre illisible : aucune alerte n'est possible"
                )
            );
        }
    }

    let in_use = processes::OpenFiles::snapshot().is_some();
    let open = if in_use {
        tr!(
            lang,
            "folders in use are seen (lsof): they stay in place",
            "les dossiers utilisés sont vus (lsof) : ils restent en place"
        )
    } else {
        tr!(
            lang,
            "folders in use are not seen: Delest warns before moving anything",
            "les dossiers utilisés ne sont pas vus : Delest prévient avant tout déplacement"
        )
    };
    println!("{} {open}", if in_use { "✓" } else { "•" });

    let timer_line = if !timer::supported() {
        tr!(
            lang,
            "periodic check: not available on this system yet",
            "contrôle périodique : pas encore disponible sur ce système"
        )
    } else if timer::enabled(home) {
        tr!(lang, "periodic check: on", "contrôle périodique : activé")
    } else {
        tr!(
            lang,
            "periodic check: off (delest timer on)",
            "contrôle périodique : désactivé (delest timer on)"
        )
    };
    println!(
        "{} {timer_line}",
        if timer::supported() { "✓" } else { "•" }
    );

    let notes = match alerts::notifier() {
        Some(program) => tr!(
            lang,
            "system notifications ({program})",
            "notifications du système ({program})"
        ),
        None => tr!(
            lang,
            "no system notifications: alerts reach the agent's session only",
            "pas de notifications du système : les alertes n'arrivent que dans la session de l'agent"
        ),
    };
    println!(
        "{} {notes}",
        if alerts::notifier().is_some() {
            "✓"
        } else {
            "•"
        }
    );

    match capacities::available_memory() {
        Some(bytes) => {
            let free = lang::size(bytes, lang);
            println!(
                "✓ {}",
                tr!(
                    lang,
                    "free memory: {free}, for the local model",
                    "mémoire libre : {free}, pour le modèle local"
                )
            );
        }
        None => println!(
            "• {}",
            tr!(
                lang,
                "free memory unreadable: the local model stays off",
                "mémoire libre illisible : le modèle local reste arrêté"
            )
        ),
    }

    let handoff = found("handoff", "--version");
    let context = if handoff {
        tr!(
            lang,
            "Handoff found: its memory will give project context",
            "Handoff trouvé : sa mémoire donnera le contexte des projets"
        )
    } else {
        tr!(
            lang,
            "Handoff not found: Delest works without it",
            "Handoff absent : Delest fonctionne sans lui"
        )
    };
    println!("{} {context}", if handoff { "✓" } else { "•" });

    for (ok, line) in crate::integration::doctor_lines(home, lang) {
        healthy &= ok;
        println!("{} {line}", if ok { "✓" } else { "✗" });
    }

    Ok(if healthy {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
