"""Banc d'essai des modèles embarqués sur le corpus local (phase 0).

Chaque entrée du corpus est décrite comme Delest la décrirait à un modèle : nom,
marqueurs du dossier parent, noms de premier niveau, état git. Aucun contenu de
fichier n'est lu. Le banc compare le verdict du modèle à la référence.

Usage :
  python3 scripts/banc_modeles.py decrire SORTIE.jsonl
  python3 scripts/banc_modeles.py classer DESCRIPTIONS.jsonl URL_SERVEUR SORTIE.jsonl
  python3 scripts/banc_modeles.py voisins DESCRIPTIONS.jsonl URL_SERVEUR SORTIE.jsonl
  python3 scripts/banc_modeles.py dedoublonner DESCRIPTIONS.jsonl SORTIE.jsonl
  python3 scripts/banc_modeles.py contexte DESCRIPTIONS.jsonl URL_EMBEDDINGS SORTIE.jsonl
  python3 scripts/banc_modeles.py bilan RESULTATS.jsonl [DESCRIPTIONS.jsonl]
"""

import json
import os
import pathlib
import random
import sys
import time
import tomllib
import urllib.request

RACINE = pathlib.Path(__file__).resolve().parent.parent
CORPUS = RACINE / "corpus" / "poste-reference.toml"
VERDICTS = ["protege", "a_juger", "supprimable_en_ligne", "supprimable"]
SUPPRIMABLES = {"supprimable_en_ligne", "supprimable"}
MARQUEURS = {
    "package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock", "bun.lockb", "Cargo.toml",
    "Cargo.lock", "pyproject.toml", "uv.lock", "poetry.lock", "requirements.txt", "Pipfile.lock",
    "pubspec.yaml", "pubspec.lock", "build.gradle", "build.gradle.kts", "settings.gradle", "Podfile",
    "Podfile.lock", "tsconfig.json", "next.config.js", "next.config.mjs", "turbo.json",
    "electron-builder.yml", "tauri.conf.json", "setup.py", "Makefile", "CMakeLists.txt", "Assets",
    "ProjectSettings", ".gitignore",
}

CONSIGNE = """Tu es le classeur de Delest, un outil qui libère l'espace disque d'un développeur sans jamais altérer son travail.
Classe l'élément décrit avec un seul verdict :
- protege : ne jamais proposer (code source, fichiers suivis par git, secrets, configuration locale, bases de données, livrables).
- a_juger : peut-être supprimable, mais rien ne garantit une reconstruction identique ; demander au développeur.
- supprimable_en_ligne : dépendances ou caches qui se reconstruisent à l'identique par le réseau (lockfile présent).
- supprimable : artefact de compilation qui se reconstruit localement.
Dans le doute, choisis le verdict le plus prudent.

Élément :
{description}

Réponds en JSON."""

# Seconde consigne : les voisins trouvés par les embeddings servent d'exemples,
# et le verdict a_juger est rappelé comme la réponse attendue en cas d'incertitude.
CONSIGNE_EXEMPLES = """Tu es le classeur de Delest, un outil qui libère l'espace disque d'un développeur sans jamais altérer son travail.
Classe l'élément décrit avec un seul verdict :
- protege : ne jamais proposer (code source, fichiers suivis par git, secrets, configuration locale, bases de données, livrables).
- a_juger : la suppression est peut-être possible, mais rien dans la description ne garantit une reconstruction identique : pas de lockfile pour des dépendances, pas de dépôt git ni de manifeste qui confirme un artefact, contenu hétérogène. C'est la bonne réponse dès qu'un doute subsiste.
- supprimable_en_ligne : dépendances ou caches qui se reconstruisent à l'identique par le réseau, parce qu'un lockfile est présent.
- supprimable : artefact de compilation qui se reconstruit localement à partir des sources du projet.

Voici des éléments proches, déjà jugés par le développeur :
{exemples}

Élément à classer :
{description}

Réponds en JSON."""

GRAMMAIRE = (
    'root ::= "{" ws "\\"verdict\\":" ws verdict "," ws "\\"motif\\":" ws motif ws "}"\n'
    'verdict ::= "\\"protege\\"" | "\\"a_juger\\"" | "\\"supprimable_en_ligne\\"" | "\\"supprimable\\""\n'
    'motif ::= "\\"" [^"\\n]{1,160} "\\""\n'
    'ws ::= [ ]?\n'
)


def decrire(entree, section):
    chemin = pathlib.Path(os.path.expanduser(entree["chemin"]))
    parent = chemin.parent
    try:
        voisins = sorted(n for n in os.listdir(parent) if n in MARQUEURS)
    except OSError:
        voisins = []
    lignes = [
        f"type : {'fichier' if section == 'piege' else 'dossier'}",
        f"nom : {chemin.name}",
        f"dossier parent : {parent.name}",
        f"marqueurs dans le dossier parent : {', '.join(voisins) or 'aucun'}",
    ]
    if chemin.is_dir():
        try:
            noms = sorted(os.listdir(chemin))
        except OSError:
            noms = []
        lignes.append(f"contenu de premier niveau ({len(noms)} éléments) : {', '.join(noms[:20])}")
    for cle, titre in (("taille_mo", "taille en Mo"), ("git", "état git"), ("lockfile", "lockfile"),
                       ("dernier_commit_jours", "jours depuis le dernier commit")):
        if cle in entree:
            lignes.append(f"{titre} : {entree[cle]}")
    return "\n".join(lignes)


def echantillon():
    corpus = tomllib.loads(CORPUS.read_text())
    hasard = random.Random(20261020)
    dossiers = corpus["dossier"]
    choisis = [e for e in dossiers if e["reference"] in ("protege", "a_juger")]
    for verdict in SUPPRIMABLES:
        groupe = [e for e in dossiers if e["reference"] == verdict]
        choisis += hasard.sample(groupe, min(20, len(groupe)))
    pieges = hasard.sample(corpus["piege"], 30)
    return ([("dossier", e) for e in choisis] + [("cache", e) for e in corpus["cache"]]
            + [("piege", e) for e in pieges])


def appeler(url, chemin, charge):
    requete = urllib.request.Request(url + chemin, json.dumps(charge).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(requete, timeout=600) as reponse:
        return json.load(reponse)


def cmd_decrire(sortie):
    with open(sortie, "w") as f:
        for i, (section, e) in enumerate(echantillon()):
            f.write(json.dumps({"id": i, "section": section, "reference": e["reference"],
                                "description": decrire(e, section)}, ensure_ascii=False) + "\n")


def cmd_classer(descriptions, url, sortie):
    with open(sortie, "w") as f:
        for ligne in open(descriptions):
            d = json.loads(ligne)
            debut = time.monotonic()
            if "exemples" in d:
                exemples = "\n\n".join(f"Exemple {i + 1} (verdict : {e['reference']}) :\n{e['description']}"
                                         for i, e in enumerate(d["exemples"]))
                consigne = CONSIGNE_EXEMPLES.format(exemples=exemples, description=d["description"])
            else:
                consigne = CONSIGNE.format(description=d["description"])
            r = appeler(url, "/v1/chat/completions", {
                "messages": [{"role": "user", "content": consigne}],
                "grammar": GRAMMAIRE, "temperature": 0, "max_tokens": 96, "cache_prompt": False,
                "chat_template_kwargs": {"enable_thinking": False}})
            duree = time.monotonic() - debut
            contenu = r["choices"][0]["message"]["content"]
            try:
                verdict = json.loads(contenu)["verdict"]
            except (ValueError, KeyError):
                verdict = "invalide"
            f.write(json.dumps({"id": d["id"], "section": d["section"], "reference": d["reference"],
                                "verdict": verdict, "secondes": round(duree, 2), "brut": contenu,
                                "jetons_invite": r["usage"]["prompt_tokens"],
                                "jetons_sortie": r["usage"]["completion_tokens"]}, ensure_ascii=False) + "\n")
            f.flush()


def vectoriser(donnees, url):
    vecteurs, durees = [], []
    for d in donnees:
        debut = time.monotonic()
        r = appeler(url, "/embedding", {"content": d["description"]})
        durees.append(time.monotonic() - debut)
        v = r[0]["embedding"] if isinstance(r, list) else r["embedding"]
        v = v[0] if isinstance(v[0], list) else v
        norme = sum(x * x for x in v) ** 0.5
        vecteurs.append([x / norme for x in v])
    return vecteurs, durees


def plus_proches(vecteurs, i, nombre=3):
    scores = sorted(((sum(a * b for a, b in zip(vecteurs[i], vecteurs[j])), j)
                     for j in range(len(vecteurs)) if j != i), reverse=True)
    return [j for _, j in scores[:nombre]]


def cmd_dedoublonner(descriptions, sortie):
    # Deux entrées sont des doublons si elles ne diffèrent que par le nom du parent,
    # la taille, l'âge du dernier commit ou les noms de premier niveau.
    variables = ("dossier parent :", "taille en Mo :", "jours depuis", "contenu de premier niveau")
    vues, gardees = set(), []
    for d in map(json.loads, open(descriptions)):
        cle = (d["reference"], tuple(l for l in d["description"].splitlines() if not l.startswith(variables)))
        if cle not in vues:
            vues.add(cle)
            gardees.append(d)
    with open(sortie, "w") as f:
        for d in gardees:
            f.write(json.dumps(d, ensure_ascii=False) + "\n")
    print(f"{len(gardees)} entrées gardées")


def cmd_contexte(descriptions, url, sortie):
    donnees = [json.loads(l) for l in open(descriptions)]
    vecteurs, _ = vectoriser(donnees, url)
    with open(sortie, "w") as f:
        for i, d in enumerate(donnees):
            exemples = [{"description": donnees[j]["description"], "reference": donnees[j]["reference"]}
                        for j in plus_proches(vecteurs, i)]
            f.write(json.dumps({**d, "exemples": exemples}, ensure_ascii=False) + "\n")


def cmd_voisins(descriptions, url, sortie):
    donnees = [json.loads(l) for l in open(descriptions)]
    vecteurs, durees = vectoriser(donnees, url)
    with open(sortie, "w") as f:
        for i, d in enumerate(donnees):
            votes = [donnees[j]["reference"] for j in plus_proches(vecteurs, i)]
            verdict = min(set(votes), key=lambda v: (-votes.count(v), VERDICTS.index(v)))
            f.write(json.dumps({"id": d["id"], "section": d["section"], "reference": d["reference"],
                                "verdict": verdict, "secondes": round(durees[i], 3)}) + "\n")


def cmd_bilan(resultats, descriptions=None):
    r = [json.loads(l) for l in open(resultats)]
    justes = sum(x["verdict"] == x["reference"] for x in r)
    dangereux = [x for x in r if x["reference"] == "protege" and x["verdict"] in SUPPRIMABLES]
    risques = [x for x in r if x["reference"] == "a_juger" and x["verdict"] in SUPPRIMABLES]
    secondes = sorted(x["secondes"] for x in r)
    print(f"entrées : {len(r)} ; justes : {justes} ({100 * justes / len(r):.0f} %)")
    print(f"protégés classés supprimables : {len(dangereux)} ; à juger classés supprimables : {len(risques)}")
    print(f"invalides : {sum(x['verdict'] == 'invalide' for x in r)}")
    print(f"secondes par entrée : médiane {secondes[len(secondes) // 2]}, max {secondes[-1]}")
    for ref in VERDICTS:
        ligne = [sum(1 for x in r if x["reference"] == ref and x["verdict"] == v) for v in VERDICTS]
        print(f"  référence {ref:22} -> {ligne}")
    for x in dangereux:
        print(f"  DANGER id {x['id']} ({x['section']}) : {x.get('brut', '')[:120]}")
    if descriptions:
        # Arbitre : les interdits absolus passent avant le modèle, qui ne voit que le reste.
        descriptions = {d["id"]: d for d in map(json.loads, open(descriptions))}
        def interdit(x):
            d = descriptions[x["id"]]
            return d["section"] == "piege" or "état git : suivi" in d["description"] \
                or "état git : non_suivi_non_ignore" in d["description"]
        reste = [x for x in r if not interdit(x)]
        justes = sum(x["verdict"] == x["reference"] for x in reste)
        dangereux = [x for x in reste if x["reference"] == "protege" and x["verdict"] in SUPPRIMABLES]
        print(f"après les interdits : {len(reste)} entrées laissées au modèle ; justes : {justes} "
              f"({100 * justes / len(reste):.0f} %) ; protégés classés supprimables : {len(dangereux)}")


if __name__ == "__main__":
    commande, *args = sys.argv[1:]
    {"decrire": cmd_decrire, "classer": cmd_classer, "voisins": cmd_voisins, "bilan": cmd_bilan,
     "dedoublonner": cmd_dedoublonner, "contexte": cmd_contexte}[commande](*args)
