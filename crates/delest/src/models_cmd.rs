//! `delest models`: the shared model store.

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use delest_core::arbiter::Arbiter;
use delest_core::index::Index;
use delest_core::lang::{self, Lang};
use delest_core::models::{self, CATALOG, Kept, Manifest};
use delest_core::processes::OpenFiles;
use delest_core::{paths, tilde, tr, volume};

use crate::actions::confirm;

pub fn list(arbiter: &Arbiter, lang: Lang) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let store = models::store_dir(home);
    let manifest = Manifest::load(&store)?;
    let folder = tilde(&store, home);
    println!(
        "{}",
        tr!(lang, "Model store: {folder}", "Dépôt de modèles : {folder}")
    );
    for known in &CATALOG {
        let (name, role, license) = (known.name, known.role, known.license);
        let state = match (manifest.models.get(name), manifest.path(&store, name)) {
            (Some(installed), Some(_)) => {
                let size = lang::size(installed.size, lang);
                tr!(lang, "installed, {size}", "installé, {size}")
            }
            _ => tr!(lang, "not installed", "pas installé"),
        };
        println!("  {name:18} {role:12} {license:11} {state}");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn path(arbiter: &Arbiter, name: &str, lang: Lang) -> io::Result<ExitCode> {
    let store = models::store_dir(arbiter.home());
    match Manifest::load(&store)?.path(&store, name) {
        Some(path) => {
            println!("{}", path.display());
            Ok(ExitCode::SUCCESS)
        }
        None => Err(io::Error::other(tr!(
            lang,
            "{name} is not installed (delest models add {name}).",
            "{name} n'est pas installé (delest models add {name})."
        ))),
    }
}

/// Installs a model: an identical file already on the disk is cloned first;
/// otherwise it is downloaded, after a confirmation.
pub fn add(
    arbiter: &Arbiter,
    name: &str,
    search: Vec<PathBuf>,
    yes: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let Some(known) = models::known(name) else {
        let names = CATALOG
            .iter()
            .map(|known| known.name)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(io::Error::other(tr!(
            lang,
            "unknown model {name}; known: {names}.",
            "modèle {name} inconnu ; connus : {names}."
        )));
    };
    let store = models::store_dir(home);
    if let Some(path) = Manifest::load(&store)?.path(&store, name) {
        let path = tilde(&path, home);
        println!(
            "{}",
            tr!(lang, "Already installed: {path}", "Déjà installé : {path}")
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut roots = models::known_caches(home);
    roots.extend(search);
    if let Ok(Some(index)) = Index::load(&paths::data_dir()) {
        roots.extend(index.roots);
    }
    if let Some(copy) = models::find_copy(known, &roots) {
        let shown = tilde(&copy, home);
        println!(
            "{}",
            tr!(
                lang,
                "Found on the disk: {shown}; cloned into the store, no download.",
                "Trouvé sur le disque : {shown} ; cloné dans le dépôt, sans téléchargement."
            )
        );
        let installed = models::add_local(&store, known, &copy)?;
        let size = lang::size(installed.size, lang);
        let sha = &installed.sha256[..12];
        println!(
            "{}",
            tr!(
                lang,
                "Installed: {name}, {size}, sha256 {sha}…",
                "Installé : {name}, {size}, sha256 {sha}…"
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    let (url, license) = (known.url, known.license);
    println!(
        "{}",
        tr!(
            lang,
            "Not found on the disk. To download: {url} (licence {license}).",
            "Introuvable sur le disque. À télécharger : {url} (licence {license})."
        )
    );
    if !yes {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other(tr!(
                lang,
                "no terminal to confirm: add --yes.",
                "pas de terminal pour confirmer : ajoutez --yes."
            )));
        }
        if !confirm(&tr!(lang, "Download it?", "Le télécharger ?"), lang)? {
            return Ok(ExitCode::SUCCESS);
        }
    }
    let installed = models::download(&store, known, true)?;
    let size = lang::size(installed.size, lang);
    println!(
        "{}",
        tr!(
            lang,
            "Installed: {name}, {size}.",
            "Installé : {name}, {size}."
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// Lists identical model files; with `replace`, makes the copies clones of one
/// reference after a preview and a confirmation.
pub fn dedupe(
    arbiter: &Arbiter,
    roots: Vec<PathBuf>,
    replace: bool,
    yes: bool,
    lang: Lang,
) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let store = models::store_dir(home);
    let mut roots = if roots.is_empty() {
        models::known_caches(home)
    } else {
        roots
    };
    roots.push(store.clone());
    let groups = models::duplicates(&roots)?;
    if groups.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "No duplicate model file.",
                "Aucun fichier de modèle en double."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    let mut wasted = 0;
    for group in &groups {
        wasted += group.size * (group.paths.len() as u64 - 1);
        let count = group.paths.len();
        let shown = lang::size(group.size, lang);
        println!(
            "{}",
            tr!(
                lang,
                "{count} identical copies of {shown}:",
                "{count} copies identiques de {shown} :"
            )
        );
        let reference = group.reference(&store);
        for path in &group.paths {
            let role = if !replace {
                ""
            } else if path == reference {
                &tr!(lang, "keep   ", "garder ")
            } else {
                &tr!(lang, "clone  ", "cloner ")
            };
            println!("  {role}{}", tilde(path, home));
        }
    }
    let wasted = lang::size(wasted, lang);
    if !replace {
        println!(
            "{}",
            tr!(
                lang,
                "At most {wasted} taken by the copies (copies that are already APFS clones share their space). Nothing was changed; to make them clones: delest models dedupe --replace.",
                "Au plus {wasted} pris par les copies (celles qui sont déjà des clones APFS partagent leur espace). Rien n'a été modifié ; pour en faire des clones : delest models dedupe --replace."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!(
        "{}",
        tr!(
            lang,
            "Each copy marked \"clone\" will be replaced by an APFS clone of the one kept: same bytes, same name, same permissions and date, its space shared. Its fingerprint is read again just before; a copy that changed, is open or cannot be cloned is left as it is. At most {wasted} freed.",
            "Chaque copie marquée « cloner » sera remplacée par un clone APFS de celle gardée : mêmes octets, même nom, mêmes droits et même date, son espace partagé. Son empreinte est relue juste avant ; une copie modifiée, ouverte ou impossible à cloner est laissée telle quelle. Au plus {wasted} libérés."
        )
    );
    if !yes {
        if !io::stdin().is_terminal() {
            return Err(io::Error::other(tr!(
                lang,
                "no terminal to confirm: add --yes.",
                "pas de terminal pour confirmer : ajoutez --yes."
            )));
        }
        if !confirm(
            &tr!(lang, "Replace the copies?", "Remplacer les copies ?"),
            lang,
        )? {
            return Ok(ExitCode::SUCCESS);
        }
    }
    let Some(open) = OpenFiles::snapshot() else {
        return Err(io::Error::other(tr!(
            lang,
            "lsof cannot tell which files are open: nothing was replaced.",
            "lsof ne peut pas dire quels fichiers sont ouverts : rien n'a été remplacé."
        )));
    };
    let before = volume::space(&store).map(|space| space.available);
    let (mut replaced, mut left) = (0, 0);
    for group in &groups {
        for (copy, kept) in models::replace_by_clones(group, &store, &open)? {
            let shown = tilde(&copy, home);
            let Some(kept) = kept else {
                replaced += 1;
                println!("{}", tr!(lang, "  cloned  {shown}", "  cloné   {shown}"));
                continue;
            };
            left += 1;
            let why = match kept {
                Kept::SameFile => tr!(lang, "already the same file", "déjà le même fichier"),
                Kept::OtherVolume => tr!(lang, "on another volume", "sur un autre volume"),
                Kept::OtherNames => tr!(
                    lang,
                    "it has other names (hard links)",
                    "il a d'autres noms (liens physiques)"
                ),
                Kept::InUse(program) => tr!(lang, "open in {program}", "ouvert dans {program}"),
                Kept::Changed => tr!(lang, "changed since", "modifié depuis"),
                Kept::NoClone => tr!(
                    lang,
                    "the volume cannot clone",
                    "le volume ne sait pas cloner"
                ),
            };
            println!(
                "{}",
                tr!(
                    lang,
                    "  left    {shown}: {why}",
                    "  laissé  {shown} : {why}"
                )
            );
        }
    }
    let after = volume::space(&store).map(|space| space.available);
    let freed = match (before, after) {
        (Some(before), Some(after)) => lang::size(after.saturating_sub(before), lang),
        _ => "?".to_string(),
    };
    println!(
        "{}",
        tr!(
            lang,
            "{replaced} copies replaced by clones, {left} left as they were. Free space measured on the volume: +{freed} (copies that were already clones free nothing).",
            "{replaced} copies remplacées par des clones, {left} laissées telles quelles. Espace libre mesuré sur le volume : +{freed} (les copies qui étaient déjà des clones ne libèrent rien)."
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// Copies a llama.cpp server into the store, so Delest does not depend on where it was built.
pub fn engine(
    arbiter: &Arbiter,
    program: Option<&std::path::Path>,
    lang: Lang,
) -> io::Result<ExitCode> {
    let home = arbiter.home();
    let store = models::store_dir(home);
    let target = match program {
        Some(program) => {
            let target = store
                .join("bin")
                .join(paths::executable_name("llama-server"));
            std::fs::create_dir_all(target.parent().unwrap_or(&target))?;
            let temporary = target.with_extension("part");
            std::fs::copy(program, &temporary)?;
            std::fs::rename(&temporary, &target)?;
            target
        }
        None => {
            if let Err(why) = delest_core::install::engine_build() {
                return Err(io::Error::other(why.text(lang)));
            }
            let release = delest_core::install::ENGINE_RELEASE;
            println!(
                "{}",
                tr!(
                    lang,
                    "Downloading llama.cpp {release} for this system…",
                    "Téléchargement de llama.cpp {release} pour ce système…"
                )
            );
            delest_core::install::install_engine(&store)?
        }
    };
    let shown = tilde(&target, home);
    println!(
        "{}",
        tr!(
            lang,
            "Engine installed: {shown}",
            "Moteur installé : {shown}"
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// The model's opinion on the unknown folders of the last scan.
pub fn advise(arbiter: &Arbiter, refresh: bool, lang: Lang) -> io::Result<ExitCode> {
    use delest_core::advisor::{self, Advice, Opinions};
    use delest_core::verdict::Verdict;

    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let Some(index) = Index::load(&data_dir)? else {
        return Err(io::Error::other(tr!(
            lang,
            "no scan yet: delest scan <folders>.",
            "aucun scan encore : delest scan <dossiers>."
        )));
    };
    // Rules in shadow already recognise their folders: only the true unknowns.
    let unknowns: Vec<_> = index
        .items
        .iter()
        .filter(|item| {
            item.verdict == Verdict::Unknown && item.shadow.is_none() && item.path.is_dir()
        })
        .cloned()
        .collect();
    if unknowns.is_empty() {
        println!(
            "{}",
            tr!(
                lang,
                "No unknown folder in the last scan.",
                "Aucun dossier inconnu dans le dernier scan."
            )
        );
        return Ok(ExitCode::SUCCESS);
    }
    if refresh {
        let mut opinions = Opinions::load(&data_dir);
        for item in &unknowns {
            opinions.folders.remove(&item.path);
        }
        opinions.save(&data_dir)?;
    }
    let setup = advisor::setup(home).map_err(|missing| io::Error::other(missing.text(lang)))?;
    let models = setup.models();
    let label = setup.reasoning_name;
    let count = unknowns.len();
    println!(
        "{}",
        tr!(
            lang,
            "Opinion of {label} on {count} unknown folders…",
            "Avis de {label} sur {count} dossiers inconnus…"
        )
    );
    let examples: Vec<_> = index
        .items
        .iter()
        .filter(|item| item.verdict != Verdict::Unknown)
        .cloned()
        .collect();
    advisor::advise(&unknowns, &examples, &models, &data_dir, |opinion| {
        let advice = match opinion.advice {
            Advice::Keep => tr!(lang, "keep", "à garder"),
            Advice::Review => tr!(lang, "to review", "à juger"),
        };
        let (path, nature, reason, seconds) = (
            tilde(&opinion.path, home),
            &opinion.nature,
            &opinion.reason,
            opinion.seconds,
        );
        println!("\n  {path} — {advice} ({nature}, {seconds:.0} s)\n    {reason}");
    })?;
    println!(
        "\n{}",
        tr!(
            lang,
            "Opinions kept in the journal; none of them makes a folder freeable.",
            "Avis gardés dans le journal ; aucun ne rend un dossier supprimable."
        )
    );
    Ok(ExitCode::SUCCESS)
}

/// A free question to the local model about one folder; it changes nothing.
pub fn ask(
    arbiter: &Arbiter,
    path: &std::path::Path,
    question: &str,
    lang: Lang,
) -> io::Result<ExitCode> {
    use delest_core::advisor;
    let mut item = Index::load(&paths::data_dir())?
        .and_then(|index| index.items.into_iter().find(|item| item.path == path))
        .unwrap_or_else(|| arbiter.assess(path));
    if item.size.is_none() {
        item.size = Some(delest_core::scan::disk_usage(path));
    }
    let setup =
        advisor::setup(arbiter.home()).map_err(|missing| io::Error::other(missing.text(lang)))?;
    let (answer, seconds) = advisor::ask(&advisor::question_prompt(&item, question, lang), &setup)?;
    let by = setup.reasoning_name;
    println!(
        "{answer}\n\n{}",
        tr!(
            lang,
            "({by}, {seconds:.0} s; nothing was changed)",
            "({by}, {seconds:.0} s ; rien n'a été modifié)"
        )
    );
    Ok(ExitCode::SUCCESS)
}
