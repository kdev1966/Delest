# Changelog

All notable changes to Delest are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.1.1] - 2026-10-25

Delest is now open source, under the MIT licence.

### Added

- **Install the local AI from the app** (Settings › AI): the llama.cpp engine built for
  the system and the two models, on a click, after a summary of what will be downloaded,
  with a progress bar. A copy of a model already in another tool's cache is cloned
  rather than downloaded. `delest models engine` with no path does the same for the
  engine.
- Every model and engine file is checked against its published SHA-256 fingerprint
  before it is kept; an engine that does not start on the system is not kept.
- `CONTRIBUTING.md` and `SECURITY.md`.

### Known limits

- llama.cpp's builds need macOS 13.3 or later, and on Windows a Visual C++ runtime 14.40
  or later; Delest says so and gives the way to get one, and still installs the models.

## [0.1.0] - 2026-10-24

The first version: Delest finds what can be deleted on a developer's machine without
harming their work, and frees the space with their consent. Every verdict comes with
its reasons.

### Added

- **The command `delest`**: `scan`, `explain`, `clean` (preview, confirmation, then
  quarantine), `restore`, `purge`, `protect`, `status`, `doctor`, `corpus`, `shadow`,
  `models`, `advise`, `ask`, `install`/`remove` and `timer`, in English and French.
- **The Arbiter**: rules for build artefacts, dependencies and caches, with absolute
  bans on untracked work, files tracked by git, secrets and source code, and project
  parts no build recreates. New rules run in shadow until the corpus accepts them.
- **The quarantine**: seven days on the item's own volume, capped at a tenth of it,
  restorable; each folder is checked again just before it moves, and one in use stays.
- **The agents' hooks**: a disk check at each session of Claude Code, Codex, Cursor,
  Gemini CLI, VS Code (Copilot) and OpenCode, and Kiro through Vario's agents; alerts
  in the session and as a system notification, rare by design. A read-only MCP server.
- **An optional timer**, off by default: launchd, systemd or the Task Scheduler.
- **The app**: a floating gauge in Vario's style and a main window (disk, proposals,
  quarantine, settings), the language chosen in the settings.
- **Local AI**: EmbeddingGemma 2 recognises, Gemma 4 E2B reasons on the unknowns and
  answers questions, only towards caution; Claude Code can be asked on a click. A
  shared model store, with duplicates replaced by APFS clones.
- **macOS, Linux and Windows**: the CI tests every system and builds the packages; an
  acceptance corpus rebuilt on each system gives the expected verdicts on all three.

### Packages

- macOS: `Delest_0.1.0_universal.dmg`, for Intel and Apple Silicon Macs (macOS 12 or
  later). Not notarised: open it the first time with right-click → Open.
- Linux: `Delest_0.1.0_amd64.deb` and `Delest_0.1.0_amd64.AppImage` (needs WebKitGTK).
- Windows: `Delest_0.1.0_x64-setup.exe`, for the current user. Not signed: SmartScreen
  asks to confirm (More info → Run anyway).

### Known limits

- On Windows, folders in use by a program are not seen yet: Delest warns before
  moving anything.
- Model clones exist on macOS (APFS) only.
- The app has been tried on an Intel Mac and on Windows 11, not yet on an Apple
  Silicon Mac nor on a Linux desktop.

[0.1.1]: https://github.com/kdev1966/Delest/releases/tag/v0.1.1
[0.1.0]: https://github.com/kdev1966/delest-site/releases/tag/v0.1.0
