# Corpus d'acceptation

Ce dossier contient de vrais dossiers du poste de référence, chacun avec son verdict. Le corpus est rejoué avant chaque changement de règle, de seuil ou de modèle. Critère bloquant : aucun élément dont la référence est `protege` ne doit être classé supprimable.

## Fichiers

- `poste-reference.toml` : relevé en lecture seule du poste de développement (macOS, Intel Core i7-4870HQ), fait le 20 octobre 2026. Il contient les vrais noms des projets, dont ceux de clients : il reste sur le poste et git l'ignore.
- `poste-reference-anonyme.toml` : la version publiée, produite par `python3 scripts/anonymiser_corpus.py`. Les noms propres au développeur y deviennent `dNNN` ; les noms d'artefacts, de pièges et de dossiers génériques (`apps`, `android`, `src-tauri`…) sont gardés, car ce sont eux que les règles examinent. Après toute modification du corpus local, relancer le script.

## Entrées

- `[[dossier]]` : artefacts de compilation et dépendances trouvés dans les projets.
- `[[cache]]` : caches globaux et outils de plateforme.
- `[[piege]]` : secrets, configuration locale et bases de données (`.env*`, `*.db`, `*.sqlite`, `*.pem`, `local.properties`). Ils ne sont pas suivis par git, donc souvent ignorés, et pourtant précieux. Leur référence est `protege` d'office. Seuls leurs chemins ont été relevés, jamais leur contenu.

## Verdicts

| Verdict | Sens |
|---|---|
| `protege` | Delest ne doit jamais le proposer. |
| `a_juger` | Delest ne le propose qu'avec l'accord explicite du développeur. |
| `supprimable_en_ligne` | Se reconstruit par le réseau, à l'identique grâce à un lockfile. Delest ne le propose pas hors ligne. |
| `supprimable` | Se reconstruit localement. |

`propose` est le verdict déduit des règles du document de conception. `reference` est le verdict fixé par le développeur le 20 octobre 2026 : il fait foi pour l'acceptation. Il diffère de `propose` pour 8 dossiers.

## Leçons du premier relevé

- **Des dossiers versionnés portent des noms d'artefacts.** Un `build/` contient les icônes d'electron-builder, suivies par git, et un `node_modules` est commité tel quel. Le nom seul ne suffit pas pour décider.
- **Le lockfile d'un monorepo est à la racine de l'espace de travail.** Au premier passage, les `node_modules` des paquets de trois monorepos paraissaient sans lockfile, parce que seul le dossier parent était examiné. Le relevé remonte désormais jusqu'à la racine du dépôt, et Delest devra faire de même.
- **Hors git, le manifeste voisin doit suffire.** Les 8 `build/` Flutter et Unity hors git étaient proposés à juger ; le développeur les a classés supprimables. Un `pubspec.yaml` ou un projet Unity à côté du dossier est un signal assez fort, même sans git.
- **L'interdit sur les fichiers non suivis et non ignorés est strict.** Il protège des `__pycache__` qu'aucun `.gitignore` ne couvre. C'est une erreur dans le sens prudent, donc acceptable, mais à surveiller.
- **Beaucoup de projets Python n'ont pas de lockfile**, seulement un `pyproject.toml` ou un `requirements.txt`. Leur `.venv` reste donc à juger.
