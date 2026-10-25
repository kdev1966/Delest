"""Produit corpus/poste-reference-anonyme.toml à partir du corpus nominatif local.

Les noms de dossiers propres au développeur (projets, clients) deviennent dNNN ;
les noms d'artefacts, de fichiers pièges et les dossiers génériques sont gardés,
car ce sont eux que les règles de Delest examinent.

Usage : python3 scripts/anonymiser_corpus.py
"""

import pathlib
import re
import tomllib

RACINE = pathlib.Path(__file__).resolve().parent.parent
SOURCE = RACINE / "corpus" / "poste-reference.toml"
CIBLE = RACINE / "corpus" / "poste-reference-anonyme.toml"

GENERIQUES = {
    "~", "Desktop", "Library", "Caches", "Developer", "CoreSimulator", "Containers",
    "com.docker.docker", ".gradle", ".npm", ".cargo", "registry", ".cache", "huggingface", ".rustup",
    "node_modules", "target", ".venv", "venv", "__pycache__", "build", "dist", ".next", ".turbo",
    "Pods", ".dart_tool", "DerivedData",
    "apps", "packages", "android", "ios", "app", "src", "src-tauri", "backend", "frontend",
    "web", "mobile", "client", "server", "api", "config", "data", "db", "prisma", "instance",
    "Assets", "ProjectSettings", "binaries", "managed_components", "release", "Builds",
    "Bee", "PackageCache", "Gradle", "unityLibrary", "Samples~", "Documentation~", "Lib", "lib",
    "model", "tests",
}
FICHIER_PIEGE = re.compile(r"^(\.env.*|local\.properties|google-services\.json|.*\.(db|sqlite3?|pem|key))$")


def composants_prives(chemins):
    noms = set()
    for chemin in chemins:
        parties = chemin.split("/")
        for i, nom in enumerate(parties):
            dernier = i == len(parties) - 1
            if nom in GENERIQUES or (dernier and FICHIER_PIEGE.match(nom)):
                continue
            noms.add(nom)
    return {nom: f"d{i:03d}" for i, nom in enumerate(sorted(noms))}


# Fixed names are kept whole; a data file keeps only its extension, the part the rules
# examine: its stem may name a project (<project>.db).
NOM_FIXE = re.compile(r"^(\.env.*|local\.properties|google-services\.json)$")


def piege(nom):
    if NOM_FIXE.match(nom):
        return nom
    return "fichier" + nom[nom.rindex("."):]


def anonymiser(chemin, table):
    parties = chemin.split("/")
    return "/".join(table.get(nom, nom) if i < len(parties) - 1 or not FICHIER_PIEGE.match(nom) else piege(nom)
                    for i, nom in enumerate(parties))


def valeur(v):
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return str(v)


def main():
    corpus = tomllib.loads(SOURCE.read_text())
    table = composants_prives(e["chemin"] for entrees in corpus.values() for e in entrees)
    lignes = [
        "# Corpus d'acceptation de Delest, version anonymisée du poste de référence.",
        "# Généré par scripts/anonymiser_corpus.py ; ne pas modifier à la main.",
        "# Les noms propres au développeur sont remplacés par dNNN ; artefacts et pièges gardent leur nom.",
        "",
    ]
    for section, entrees in corpus.items():
        for e in entrees:
            lignes.append(f"[[{section}]]")
            for cle, v in e.items():
                if cle == "chemin":
                    v = anonymiser(v, table)
                lignes.append(f"{cle} = {valeur(v)}")
            lignes.append("")
    CIBLE.write_text("\n".join(lignes))
    print(f"{CIBLE.relative_to(RACINE)} : {sum(len(v) for v in corpus.values())} entrées, {len(table)} noms remplacés")


if __name__ == "__main__":
    main()
