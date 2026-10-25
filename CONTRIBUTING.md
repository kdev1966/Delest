# Contributing to Delest

Thank you for helping. Delest deletes files on developers' machines, so one rule comes
before any other: **a change must never make a protected item deletable.**

## Before you open a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tests include the synthetic acceptance corpus (`corpus/synthetique.toml`): the
situations of a real machine, rebuilt in a temporary folder and replayed. A new rule, or
a change to a rule or a ban, comes with its cases there. The CI runs everything on
macOS, Linux and Windows.

## How the code is laid out

- `crates/delest-core`: the rules, the bans, the Arbiter, the quarantine, the local AI.
- `crates/delest`: the command.
- `crates/delest-gui`: the app (Tauri 2), in plain HTML and JavaScript.
- `delest-document-de-conception.md`: the design and its decisions (in French).

A new rule starts in shadow (`shadow: true`): it recognises its folders without
proposing them, until the corpus accepts it.

## Texts

Every text shown to people exists in English and French (`tr!` in Rust, the dictionaries
at the top of the app's scripts). A test checks that the app's texts are all translated.

## Licence

By contributing, you agree that your contribution is published under the
[MIT licence](LICENSE), like the rest of Delest.
