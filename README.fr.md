# Delest

*[English](README.md)* · Site : https://kdev1966.github.io/delest-site/

Delest repère sur le poste d'un développeur ce qui peut être supprimé sans altérer son travail (artefacts de compilation, dépendances, caches) et libère l'espace avec son accord. Chaque verdict est expliqué.

> **État** : phases 1 à 3 terminées et phase 4 (IA locale, dépôt de modèles) presque terminée, macOS d'abord. Voir le [document de conception](delest-document-de-conception.md).

## Installation

La commande :

```sh
cargo install --path crates/delest
```

L'application, avec la commande à l'intérieur, pour le système où elle est construite (sous Windows, depuis Git Bash) :

```sh
cargo install tauri-cli --version "^2" --locked   # une fois
rustup target add x86_64-apple-darwin aarch64-apple-darwin   # une fois, sous macOS : une application universelle
./scripts/package.sh                              # macOS : DMG ; Linux : .deb et AppImage ; Windows : installeur
```

Des paquets prêts à installer sont joints à chaque [release](https://github.com/kdev1966/Delest/releases). Sous Linux, l'application a besoin de WebKitGTK (`libwebkit2gtk-4.1`).

- **macOS** (Intel et Apple Silicon) : ouvrez le DMG et glissez Delest dans Applications. L'application n'est pas encore notariée : la première fois, ouvrez-la par clic droit → Ouvrir.
- **Linux** : `sudo apt install ./Delest_<version>_amd64.deb`, ou rendez l'AppImage exécutable et lancez-la.
- **Windows** : lancez `Delest_<version>_x64-setup.exe` ; il installe pour l'utilisateur courant. Il n'est pas encore signé : SmartScreen demande de confirmer (Informations complémentaires → Exécuter quand même).

## Usage

```sh
delest scan ~/Projets            # ce qui peut être libéré, et pourquoi (lecture seule)
delest explain ~/Projets/app/target
delest clean ~/Projets --dry-run # aperçu de ce qui irait en quarantaine
delest clean ~/Projets           # aperçu, confirmation, puis quarantaine
delest restore --list            # contenu de la quarantaine
delest restore                   # remet en place le dernier élément
delest purge                     # libère tout de suite ce qui se reconstruit localement
delest protect ~/Clients/archive # ne jamais proposer ce dossier
delest status                    # espace libre, récupérable d'après le dernier scan, quarantaine
delest install                   # hook dans Claude Code : un contrôle à chaque session
delest timer on                  # contrôle toutes les 4 heures (désactivé par défaut)
delest shadow                    # ce que proposeraient les nouvelles règles, encore en ombre
delest mcp                       # outils en lecture seule pour les agents (MCP)
delest doctor
```

## Ce que Delest ne touche jamais

- un fichier suivi par git, ou nouveau (ni suivi ni ignoré) ;
- un dossier qui contient des secrets, une configuration locale ou des données (`.env*`, `*.db`, `*.sqlite`, `*.pem`…) ;
- un dossier de compilation sans manifeste qui contient du code source ;
- un dossier protégé avec `delest protect` ;
- un lien symbolique.

Ces interdits passent avant toute règle et toute habitude.

## Verdicts

| Verdict | Sens |
|---|---|
| supprimable | se reconstruit localement |
| supprimable avec réseau | se reconstruit à l'identique par le réseau (lockfile) ; jamais proposé hors ligne |
| à juger | proposé seulement un par un, avec `clean --review` |
| protégé, inconnu | jamais proposé |

## Sécurité

- `clean` déplace vers une quarantaine sur le même volume, jamais de copie ; la fiche de chaque élément est écrite avant le déplacement.
- Juste avant d'agir, chaque dossier est réévalué ; il reste en place si son verdict a changé, s'il a été modifié depuis moins de dix minutes ou si un programme y a un fichier ouvert.
- La quarantaine garde chaque élément 7 jours, dans la limite de 10 % du volume ; `purge` la vide plus tôt, seulement pour ce qui se reconstruit localement.
- Les dépendances d'un projet travaillé ces 30 derniers jours sont gardées.
- Un élément refusé trois fois n'est plus proposé ; une restauration rend l'élément, puis sa règle, plus prudents.
- Chaque action est journalisée.

## Corpus d'acceptation

`delest corpus` rejoue un ensemble de vrais dossiers dont le verdict a été fixé par le développeur ; il échoue si un élément protégé sort supprimable. Voir [corpus/README.md](corpus/README.md).

## Contribuer

Voir [CONTRIBUTING.md](CONTRIBUTING.md) ; pour signaler une faille, [SECURITY.md](SECURITY.md).

## Licence

MIT © 2026 Othman Kaabaoui.
