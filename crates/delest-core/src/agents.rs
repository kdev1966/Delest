//! Installing Delest's hook in the developer's AI agents, so that Delest checks
//! the disk at the start of each session without a process of its own.
//!
//! Delest's entries are recognised by their command, `<path>/delest hook …`; the
//! developer's own entries are never changed. Every file is copied into the data
//! directory before its first change, and written atomically.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The agents Delest knows how to hook into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    ClaudeCode,
    Codex,
    Cursor,
    Gemini,
    VsCode,
    OpenCode,
}

pub const AGENTS: [Agent; 6] = [
    Agent::ClaudeCode,
    Agent::Codex,
    Agent::Cursor,
    Agent::Gemini,
    Agent::VsCode,
    Agent::OpenCode,
];

/// Seconds the agent waits for the hook; Delest only reads `df` and its index.
const TIMEOUT: u64 = 5;

/// `$<var>` when set and not empty, else `default`.
fn dir_from(var: &str, default: PathBuf) -> PathBuf {
    env::var_os(var)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Gemini => "Gemini CLI",
            Agent::VsCode => "VS Code (Copilot)",
            Agent::OpenCode => "OpenCode",
        }
    }

    /// The value of `--via`, which tells the hook how to answer.
    pub fn id(self) -> &'static str {
        match self {
            Agent::ClaudeCode => "claude",
            Agent::Codex => "codex",
            Agent::Cursor => "cursor",
            Agent::Gemini => "gemini",
            Agent::VsCode => "vscode",
            Agent::OpenCode => "opencode",
        }
    }

    pub fn from_id(id: &str) -> Option<Agent> {
        AGENTS.into_iter().find(|agent| agent.id() == id)
    }

    /// The agent's configuration folder.
    fn config_dir(self, home: &Path) -> PathBuf {
        match self {
            Agent::ClaudeCode => dir_from("CLAUDE_CONFIG_DIR", home.join(".claude")),
            Agent::Codex => dir_from("CODEX_HOME", home.join(".codex")),
            Agent::Cursor => home.join(".cursor"),
            Agent::Gemini => dir_from("GEMINI_CLI_HOME", home.to_path_buf()).join(".gemini"),
            Agent::VsCode => dir_from("COPILOT_HOME", home.join(".copilot")),
            Agent::OpenCode => dir_from("XDG_CONFIG_HOME", home.join(".config")).join("opencode"),
        }
    }

    /// The file that holds the agent's hooks. VS Code reads every file of
    /// `<copilot>/hooks/`, so Delest has one of its own there; OpenCode has no hook
    /// commands, only plugins, and Delest's is `<config>/plugins/delest.js`.
    pub fn settings(self, home: &Path) -> PathBuf {
        let dir = self.config_dir(home);
        match self {
            Agent::ClaudeCode | Agent::Gemini => dir.join("settings.json"),
            Agent::Codex | Agent::Cursor => dir.join("hooks.json"),
            Agent::VsCode => dir.join("hooks").join("delest.json"),
            Agent::OpenCode => dir.join("plugins").join("delest.js"),
        }
    }

    /// Whether the agent seems installed: its configuration folder exists.
    pub fn detected(self, home: &Path) -> bool {
        self.config_dir(home).is_dir()
    }

    /// Whether the whole file is Delest's, removed with Delest's entry.
    fn owns_file(self) -> bool {
        matches!(self, Agent::VsCode | Agent::OpenCode)
    }

    fn event(self) -> &'static str {
        match self {
            Agent::Cursor => "sessionStart",
            _ => "SessionStart",
        }
    }

    /// Delest's entry in the agent's list of session-start hooks.
    fn entry(self, command: &str) -> Value {
        match self {
            Agent::ClaudeCode | Agent::Codex => json!({
                "matcher": "startup|resume",
                "hooks": [{ "type": "command", "command": command, "timeout": TIMEOUT }]
            }),
            // Gemini CLI counts in milliseconds.
            Agent::Gemini => json!({
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "name": "delest",
                    "timeout": TIMEOUT * 1000
                }]
            }),
            Agent::Cursor => json!({ "command": command, "timeout": TIMEOUT }),
            Agent::VsCode => json!({ "type": "command", "command": command, "timeout": TIMEOUT }),
            Agent::OpenCode => Value::String(plugin(command)),
        }
    }

    /// What the hook prints: the alert as the agent expects it, or an answer that
    /// changes nothing. Claude Code accepts no output; the others expect JSON.
    /// OpenCode's plugin shows nothing: the system notification carries the alert.
    pub fn answer(self, alert: Option<&str>) -> String {
        match (self, alert) {
            (Agent::OpenCode, _) => String::new(),
            (Agent::Cursor, Some(text)) => json!({ "additional_context": text }).to_string(),
            // systemMessage is shown to the developer; additionalContext reaches the agent.
            (_, Some(text)) => json!({
                "systemMessage": text,
                "hookSpecificOutput": { "hookEventName": "SessionStart", "additionalContext": text }
            })
            .to_string(),
            (Agent::ClaudeCode, None) => String::new(),
            (_, None) => "{}".to_string(),
        }
    }
}

/// Marks the plugin file as Delest's.
const PLUGIN_MARKER: &str = "// Generated by Delest";

/// OpenCode's plugin: runs the hook command once, when OpenCode starts.
fn plugin(command: &str) -> String {
    let command = Value::String(command.to_string());
    format!(
        r#"{PLUGIN_MARKER}; removed by 'delest remove'.
// Runs Delest's check when OpenCode starts: Delest looks at the free space and says
// nothing unless an alert is due, which a system notification then shows.
const COMMAND = {command};

export const DelestPlugin = async ({{ $ }}) => {{
  try {{
    await $`sh -c ${{COMMAND}} < ${{new Response("{{}}")}}`.quiet().nothrow();
  }} catch {{
    // A hook never gets in the way.
  }}
  return {{}};
}};
"#
    )
}

/// The state of a file that is all Delest's: the plugin, recognised by its marker.
fn plugin_state(path: &Path, command: &str) -> Result<State, Error> {
    match fs::read_to_string(path) {
        Ok(text) if text == plugin(command) => Ok(State::Installed),
        Ok(text) if text.starts_with(PLUGIN_MARKER) => Ok(State::Outdated),
        Ok(_) => Err(Error::NotOurs(path.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(State::Absent),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Whether Delest's hook is in the agent's file, whatever program it names.
pub fn hooked(agent: Agent, home: &Path) -> bool {
    matches!(
        state(agent, &agent.settings(home), ""),
        Ok(State::Installed | State::Outdated)
    )
}

/// Kiro runs only the hooks of the selected agent, written in that agent's file;
/// Vario's agents carry Delest's (`--via kiro`). `None` without Kiro agents, else
/// whether one of them runs Delest's check.
pub fn kiro_hooked(home: &Path) -> Option<bool> {
    let entries = fs::read_dir(home.join(".kiro").join("agents")).ok()?;
    let mut any = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        any = true;
        let hooked = read(&path).is_ok_and(|agent| {
            agent["hooks"]["agentSpawn"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(entry_is_delest))
        });
        if hooked {
            return Some(true);
        }
    }
    any.then_some(false)
}

/// `<exe> hook session-start --via claude`, with the program quoted when needed: a
/// space, a quote or a backslash, which Git Bash (Claude Code's shell on Windows)
/// would otherwise swallow.
pub fn hook_command(exe: &Path, agent: Agent) -> String {
    let program = exe.display().to_string();
    let program = if program.contains([' ', '"', '\'', '\\']) {
        format!("\"{}\"", program.replace('"', "\\\""))
    } else {
        program
    };
    format!("{program} hook session-start --via {}", agent.id())
}

/// Whether a hook command runs Delest: `delest hook …`, `/usr/local/bin/delest hook …`.
pub fn is_delest_hook(command: &str) -> bool {
    command.match_indices(" hook ").any(|(at, _)| {
        let program = command[..at].trim_matches(['"', '\'']);
        let name = program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(program)
            .to_lowercase();
        name == "delest" || name == "delest.exe"
    })
}

/// Whether a hook command of Delest's is anywhere in the entry: Claude Code nests it
/// in `hooks[].command`, Cursor and VS Code put it at the top.
fn entry_is_delest(entry: &Value) -> bool {
    match entry {
        Value::String(text) => is_delest_hook(text),
        Value::Array(items) => items.iter().any(entry_is_delest),
        Value::Object(map) => map.values().any(entry_is_delest),
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Installed,
    /// An entry of Delest's is there, but not the expected one (another path).
    Outdated,
    Absent,
}

#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// The file is not a JSON object Delest can change safely; it was left as it was.
    NotAnObject(PathBuf),
    /// A file of that name exists but is not Delest's; it was left as it was.
    NotOurs(PathBuf),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::NotAnObject(path) => write!(
                f,
                "{}: not a JSON object; it was left unchanged",
                path.display()
            ),
            Error::NotOurs(path) => write!(
                f,
                "{}: not written by Delest; it was left unchanged",
                path.display()
            ),
        }
    }
}

impl std::error::Error for Error {}

fn read(path: &Path) -> Result<Map<String, Value>, Error> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(source) => {
            return Err(Error::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(Error::NotAnObject(path.to_path_buf())),
    }
}

/// The agent's list of session-start entries, created if missing; `None` if the
/// file holds something else there.
fn session_start(agent: Agent, settings: &mut Map<String, Value>) -> Option<&mut Vec<Value>> {
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let list = hooks
        .as_object_mut()?
        .entry(agent.event())
        .or_insert_with(|| json!([]));
    list.as_array_mut()
}

/// Whether the agent's file holds Delest's entry.
pub fn state(agent: Agent, path: &Path, command: &str) -> Result<State, Error> {
    if agent == Agent::OpenCode {
        return plugin_state(path, command);
    }
    let mut settings = read(path)?;
    let Some(list) = session_start(agent, &mut settings) else {
        return Err(Error::NotAnObject(path.to_path_buf()));
    };
    let ours: Vec<&Value> = list.iter().filter(|entry| entry_is_delest(entry)).collect();
    Ok(match ours.as_slice() {
        [] => State::Absent,
        [entry] if **entry == agent.entry(command) => State::Installed,
        _ => State::Outdated,
    })
}

/// What `install` or `remove` would write, without writing it.
pub struct Change {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// The settings with Delest's entry in place of any older one of Delest's.
pub fn plan_install(agent: Agent, path: &Path, command: &str) -> Result<Change, Error> {
    let before = fs::read_to_string(path).ok();
    if agent == Agent::OpenCode {
        plugin_state(path, command)?;
        return Ok(Change {
            path: path.to_path_buf(),
            before,
            after: Some(plugin(command)),
        });
    }
    let mut settings = read(path)?;
    if agent == Agent::Cursor {
        settings.entry("version").or_insert_with(|| json!(1));
    }
    let list = session_start(agent, &mut settings)
        .ok_or_else(|| Error::NotAnObject(path.to_path_buf()))?;
    list.retain(|entry| !entry_is_delest(entry));
    list.push(agent.entry(command));
    Ok(Change {
        path: path.to_path_buf(),
        before,
        after: Some(pretty(&settings)),
    })
}

/// The settings without Delest's entries; empty containers it leaves are removed,
/// and so is a file of Delest's own.
pub fn plan_remove(agent: Agent, path: &Path) -> Result<Change, Error> {
    let before = fs::read_to_string(path).ok();
    if agent == Agent::OpenCode {
        plugin_state(path, "")?;
        return Ok(Change {
            path: path.to_path_buf(),
            before,
            after: None,
        });
    }
    let mut settings = read(path)?;
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        if let Some(list) = hooks.get_mut(agent.event()).and_then(Value::as_array_mut) {
            list.retain(|entry| !entry_is_delest(entry));
            if list.is_empty() {
                hooks.remove(agent.event());
            }
        }
        if hooks.is_empty() {
            settings.remove("hooks");
        }
    }
    let after = if settings.is_empty() && (before.is_none() || agent.owns_file()) {
        None
    } else {
        Some(pretty(&settings))
    };
    Ok(Change {
        path: path.to_path_buf(),
        before,
        after,
    })
}

fn pretty(settings: &Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

impl Change {
    pub fn changes_anything(&self) -> bool {
        self.before != self.after
    }

    /// Copies the current file into `backups`, then writes the new one atomically,
    /// keeping its permissions.
    pub fn apply(&self, backups: &Path) -> Result<(), Error> {
        if !self.changes_anything() {
            return Ok(());
        }
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| Error::Io { path, source }
        };
        if let Some(before) = &self.before {
            fs::create_dir_all(backups).map_err(io(backups))?;
            // Several agents name their file settings.json or hooks.json.
            let agent = self
                .path
                .parent()
                .and_then(Path::file_name)
                .unwrap_or_default()
                .to_string_lossy();
            let name = self.path.file_name().unwrap_or_default().to_string_lossy();
            let backup = backups.join(format!("{}-{name}", agent.trim_start_matches('.')));
            fs::write(&backup, before).map_err(io(&backup))?;
        }
        let Some(after) = &self.after else {
            if self.before.is_some() {
                fs::remove_file(&self.path).map_err(io(&self.path))?;
            }
            return Ok(());
        };
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(io(parent))?;
        let temporary = self.path.with_extension("json.delest-tmp");
        fs::write(&temporary, after).map_err(io(&temporary))?;
        if let Ok(metadata) = fs::metadata(&self.path) {
            fs::set_permissions(&temporary, metadata.permissions()).map_err(io(&temporary))?;
        }
        fs::rename(&temporary, &self.path).map_err(io(&self.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: &str = "/usr/local/bin/delest hook session-start --via claude";

    fn home_with(settings: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::write(home.path().join(".claude/settings.json"), settings).unwrap();
        home
    }

    fn file(home: &Path) -> PathBuf {
        home.join(".claude/settings.json")
    }

    #[test]
    fn delest_hook_commands_are_recognised_by_their_program() {
        assert!(is_delest_hook("delest hook session-start --via claude"));
        assert!(is_delest_hook(
            "\"/Users/a b/bin/delest\" hook session-start"
        ));
        assert!(!is_delest_hook("/usr/local/bin/vario hook session-start"));
        assert!(!is_delest_hook("echo delest hook"));
        assert_eq!(
            hook_command(Path::new("/Users/a b/delest"), Agent::ClaudeCode),
            "\"/Users/a b/delest\" hook session-start --via claude"
        );
        assert_eq!(
            hook_command(Path::new(r"C:\Users\a\delest.exe"), Agent::ClaudeCode),
            r#""C:\Users\a\delest.exe" hook session-start --via claude"#
        );
        assert!(is_delest_hook(
            r#""C:\Users\a\delest.exe" hook session-start"#
        ));
    }

    #[test]
    fn install_keeps_the_developer_s_entries_and_replaces_older_ones() {
        let home = home_with(
            r#"{"model": "opus", "hooks": {"SessionStart": [
                {"hooks": [{"type": "command", "command": "handoff hook claude session-start"}]},
                {"matcher": "startup", "hooks": [{"type": "command", "command": "/old/delest hook session-start --via claude"}]}
            ]}}"#,
        );
        let home = home.path();
        assert_eq!(
            state(Agent::ClaudeCode, &file(home), COMMAND).unwrap(),
            State::Outdated
        );
        let change = plan_install(Agent::ClaudeCode, &file(home), COMMAND).unwrap();
        let backups = home.join("backups");
        change.apply(&backups).unwrap();
        assert_eq!(
            state(Agent::ClaudeCode, &file(home), COMMAND).unwrap(),
            State::Installed
        );
        let written: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        let list = written["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            list[0]["hooks"][0]["command"],
            "handoff hook claude session-start"
        );
        assert_eq!(written["model"], "opus");
        assert!(
            fs::read_to_string(backups.join("claude-settings.json"))
                .unwrap()
                .contains("/old/delest")
        );
        // A second install changes nothing.
        assert!(
            !plan_install(Agent::ClaudeCode, &file(home), COMMAND)
                .unwrap()
                .changes_anything()
        );
    }

    #[test]
    fn remove_leaves_only_what_was_there() {
        let home = home_with(r#"{"model": "opus"}"#);
        let home = home.path();
        plan_install(Agent::ClaudeCode, &file(home), COMMAND)
            .unwrap()
            .apply(&home.join("b1"))
            .unwrap();
        plan_remove(Agent::ClaudeCode, &file(home))
            .unwrap()
            .apply(&home.join("b2"))
            .unwrap();
        let written: Value =
            serde_json::from_str(&fs::read_to_string(home.join(".claude/settings.json")).unwrap())
                .unwrap();
        assert_eq!(written, json!({"model": "opus"}));
        assert_eq!(
            state(Agent::ClaudeCode, &file(home), COMMAND).unwrap(),
            State::Absent
        );
    }

    #[test]
    fn cursor_gets_its_own_flat_entry_beside_the_developer_s() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.json");
        fs::write(
            &path,
            r#"{"version": 1, "hooks": {"sessionStart": [{"command": "vario hook session-start --via cursor"}]}}"#,
        )
        .unwrap();
        let command = "/bin/delest hook session-start --via cursor";
        plan_install(Agent::Cursor, &path, command)
            .unwrap()
            .apply(&dir.path().join("b"))
            .unwrap();
        let written: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["version"], 1);
        assert_eq!(
            written["hooks"]["sessionStart"],
            json!([
                {"command": "vario hook session-start --via cursor"},
                {"command": command, "timeout": 5}
            ])
        );
        assert_eq!(
            state(Agent::Cursor, &path, command).unwrap(),
            State::Installed
        );
        // A new file gets the version Cursor requires.
        let fresh = dir.path().join("fresh.json");
        plan_install(Agent::Cursor, &fresh, command)
            .unwrap()
            .apply(&dir.path().join("b"))
            .unwrap();
        assert!(
            fs::read_to_string(&fresh)
                .unwrap()
                .contains("\"version\": 1")
        );
    }

    #[test]
    fn gemini_counts_in_milliseconds_and_codex_like_claude_code() {
        let gemini = Agent::Gemini.entry("delest hook session-start --via gemini");
        assert_eq!(gemini["hooks"][0]["timeout"], 5000);
        assert_eq!(gemini["hooks"][0]["name"], "delest");
        let codex = Agent::Codex.entry("delest hook session-start --via codex");
        assert_eq!(codex["matcher"], "startup|resume");
        assert_eq!(codex["hooks"][0]["timeout"], 5);
        assert!(entry_is_delest(&gemini) && entry_is_delest(&codex));
    }

    #[test]
    fn the_vs_code_file_is_delest_s_and_goes_with_its_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks/delest.json");
        let command = "/bin/delest hook session-start --via vscode";
        plan_install(Agent::VsCode, &path, command)
            .unwrap()
            .apply(&dir.path().join("b1"))
            .unwrap();
        assert_eq!(
            state(Agent::VsCode, &path, command).unwrap(),
            State::Installed
        );
        plan_remove(Agent::VsCode, &path)
            .unwrap()
            .apply(&dir.path().join("b2"))
            .unwrap();
        assert!(!path.exists());
        assert!(
            dir.path().join("b2/hooks-delest.json").exists(),
            "backed up first"
        );
    }

    #[test]
    fn opencode_gets_a_plugin_of_delest_s_own() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugins/delest.js");
        let command = "\"/Users/a b/delest\" hook session-start --via opencode";
        plan_install(Agent::OpenCode, &path, command)
            .unwrap()
            .apply(&dir.path().join("b1"))
            .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(
            r#"const COMMAND = "\"/Users/a b/delest\" hook session-start --via opencode";"#
        ));
        assert_eq!(
            state(Agent::OpenCode, &path, command).unwrap(),
            State::Installed
        );
        assert_eq!(
            state(Agent::OpenCode, &path, "other").unwrap(),
            State::Outdated
        );
        plan_remove(Agent::OpenCode, &path)
            .unwrap()
            .apply(&dir.path().join("b2"))
            .unwrap();
        assert!(!path.exists());
        // Someone else's plugin of that name is left alone.
        fs::write(&path, "export const Mine = 1;").unwrap();
        assert!(matches!(
            plan_install(Agent::OpenCode, &path, command),
            Err(Error::NotOurs(_))
        ));
        assert!(matches!(
            plan_remove(Agent::OpenCode, &path),
            Err(Error::NotOurs(_))
        ));
    }

    #[test]
    fn kiro_is_hooked_through_an_agent_file() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(kiro_hooked(home.path()), None);
        let agents = home.path().join(".kiro/agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("vario.json"),
            r#"{"hooks": {"agentSpawn": [{"command": "vario hook session-start --via kiro"}]}}"#,
        )
        .unwrap();
        assert_eq!(kiro_hooked(home.path()), Some(false));
        fs::write(
            agents.join("vario.json"),
            r#"{"hooks": {"agentSpawn": [{"command": "vario hook session-start --via kiro"}, {"command": "/bin/delest hook session-start --via kiro"}]}}"#,
        )
        .unwrap();
        assert_eq!(kiro_hooked(home.path()), Some(true));
    }

    #[test]
    fn each_agent_gets_the_answer_it_expects() {
        assert_eq!(Agent::ClaudeCode.answer(None), "");
        for agent in [Agent::Codex, Agent::Cursor, Agent::Gemini, Agent::VsCode] {
            assert_eq!(agent.answer(None), "{}", "{agent:?}");
        }
        let cursor: Value = serde_json::from_str(&Agent::Cursor.answer(Some("low"))).unwrap();
        assert_eq!(cursor, json!({"additional_context": "low"}));
        let codex: Value = serde_json::from_str(&Agent::Codex.answer(Some("low"))).unwrap();
        assert_eq!(codex["systemMessage"], "low");
        assert_eq!(codex["hookSpecificOutput"]["additionalContext"], "low");
    }

    #[test]
    fn a_file_delest_cannot_read_safely_is_left_alone() {
        let home = home_with("{ // a comment\n}");
        let home = home.path();
        assert!(matches!(
            plan_install(Agent::ClaudeCode, &file(home), COMMAND),
            Err(Error::NotAnObject(_))
        ));
        let home2 = home_with(r#"{"hooks": {"SessionStart": "not a list"}}"#);
        assert!(matches!(
            plan_install(Agent::ClaudeCode, &file(home2.path()), COMMAND),
            Err(Error::NotAnObject(_))
        ));
    }
}
