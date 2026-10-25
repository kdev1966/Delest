# Delest

*[Français](README.fr.md)* · Site: https://kdev1966.github.io/delest-site/

Delest finds what can be deleted on a developer's machine without harming their work (build artefacts, dependencies, caches) and frees the space with their consent. Every verdict comes with its reasons.

> **Status**: phases 1 to 3 done and phase 4 (local AI, model store) nearly done, macOS first. See the [design document](delest-document-de-conception.md) (in French).

## Install

The command:

```sh
cargo install --path crates/delest
```

The app, with the command inside it, for the system it is built on (on Windows, from Git Bash):

```sh
cargo install tauri-cli --version "^2" --locked   # once
rustup target add x86_64-apple-darwin aarch64-apple-darwin   # once, on macOS: a universal app
./scripts/package.sh                              # macOS: DMG; Linux: .deb and AppImage; Windows: installer
```

Ready-made packages are attached to each [release](https://github.com/kdev1966/Delest/releases). On Linux the app needs WebKitGTK (`libwebkit2gtk-4.1`).

- **macOS** (Intel and Apple Silicon): open the DMG and drag Delest into Applications. The app is not notarised yet: the first time, open it with right-click → Open.
- **Linux**: `sudo apt install ./Delest_<version>_amd64.deb`, or make the AppImage executable and run it.
- **Windows**: run `Delest_<version>_x64-setup.exe`; it installs for the current user. It is not signed yet: SmartScreen asks to confirm (More info → Run anyway).

## Usage

```sh
delest scan ~/Projects            # what can be freed, and why (read only)
delest explain ~/Projects/app/target
delest clean ~/Projects --dry-run # preview of what would go into the quarantine
delest clean ~/Projects           # preview, confirmation, then quarantine
delest restore --list             # what the quarantine holds
delest restore                    # put the latest item back
delest purge                      # free at once what rebuilds locally
delest protect ~/Clients/archive  # never offer this folder
delest status                     # free space, what the last scan found, the quarantine
delest install                    # hook in Claude Code: a check at each session
delest timer on                   # a check every 4 hours (off by default)
delest shadow                     # what the new rules, still in shadow, would offer
delest mcp                        # read-only tools for agents (MCP)
delest doctor
```

## What Delest never touches

- a file tracked by git, or new to it (neither tracked nor ignored);
- a folder holding secrets, local configuration or data (`.env*`, `*.db`, `*.sqlite`, `*.pem`…);
- a build folder without a manifest that holds source code;
- a folder protected with `delest protect`;
- a symbolic link.

These bans come before every rule and every habit.

## Verdicts

| Verdict | Meaning |
|---|---|
| freeable | rebuilt locally |
| freeable online | rebuilt identically from the network (lockfile); never offered offline |
| to review | offered only one by one, with `clean --review` |
| protected, unknown | never offered |

## Safety

- `clean` moves items into a quarantine on the same volume, never copies them; each item's record is written before the move.
- Right before acting, each folder is assessed again; it stays in place if its verdict changed, if it was modified in the last ten minutes, or if a program has a file open in it.
- The quarantine keeps each item 7 days, within 10 % of the volume; `purge` empties it sooner, only for what rebuilds locally.
- The dependencies of a project worked on in the last 30 days are kept.
- An item refused three times is no longer offered; a restore makes the item, then its rule, more cautious.
- Every action is journaled.

## Acceptance corpus

`delest corpus` replays real folders whose verdict the developer set; it fails if a protected item comes out freeable. See [corpus/README.md](corpus/README.md) (in French).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md); to report a vulnerability, [SECURITY.md](SECURITY.md).

## License

MIT © 2026 Othman Kaabaoui.
