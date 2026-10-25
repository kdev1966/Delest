//! `delest mcp`: a Model Context Protocol server on standard input and output, for
//! the developer's agents. Its tools only read: moving or deleting stays a command
//! the developer runs, after a preview.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use delest_core::arbiter::Arbiter;
use delest_core::index::Index;
use delest_core::lang::{self, Lang};
use delest_core::quarantine::{self, Quarantined};
use delest_core::verdict::Assessment;
use delest_core::{paths, scan, volume};
use serde_json::{Value, json};

const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub fn serve(arbiter: &Arbiter, lang: Lang) -> io::Result<ExitCode> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Some(reply) = handle(arbiter, &line, lang) else {
            continue;
        };
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The answer to one JSON-RPC message; `None` for a notification.
fn handle(arbiter: &Arbiter, line: &str, lang: Lang) -> Option<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Some(error(Value::Null, -32700, "parse error"));
    };
    let id = message.get("id").cloned()?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match message["method"].as_str().unwrap_or_default() {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or_default();
            let version = VERSIONS
                .iter()
                .find(|version| **version == asked)
                .unwrap_or(&VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "delest", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Delest tells what can be freed on this disk and why. Its tools only read. To free space, ask the developer to run `delest clean --dry-run`, then `delest clean`; never delete build folders yourself."
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => match call(arbiter, &params, lang) {
            Ok(value) => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
            Err(message) => {
                json!({ "content": [{ "type": "text", "text": message }], "isError": true })
            }
        },
        _ => return Some(error(id, -32601, "method not found")),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools() -> Value {
    let read_only = json!({ "readOnlyHint": true, "destructiveHint": false });
    json!([
        {
            "name": "delest_status",
            "description": "Free space on the disk, what the last scan found freeable (by verdict) and what the quarantine holds. Fast: does not scan.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only
        },
        {
            "name": "delest_explain",
            "description": "Delest's verdict on one folder (protected, unknown, to review, freeable online, freeable) with the reasons, and its size.",
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string", "description": "Absolute path of the folder" } },
                "required": ["path"]
            },
            "annotations": read_only
        },
        {
            "name": "delest_scan",
            "description": "Scan folders for build outputs, dependencies and caches; returns every item found with its verdict, size and reasons. Can take tens of seconds on a large folder.",
            "inputSchema": {
                "type": "object",
                "properties": { "paths": { "type": "array", "items": { "type": "string" }, "description": "Absolute folder paths" } },
                "required": ["paths"]
            },
            "annotations": read_only
        }
    ])
}

fn call(arbiter: &Arbiter, params: &Value, lang: Lang) -> Result<Value, String> {
    let arguments = &params["arguments"];
    match params["name"].as_str().unwrap_or_default() {
        "delest_status" => Ok(status(arbiter, lang)),
        "delest_explain" => {
            let path = arguments["path"].as_str().ok_or("path is required")?;
            let path =
                paths::canonical(Path::new(path)).map_err(|error| format!("{path}: {error}"))?;
            let mut assessment = arbiter.assess(&path);
            assessment.size = Some(scan::disk_usage(&path));
            Ok(item(&assessment, lang))
        }
        "delest_scan" => {
            let roots = arguments["paths"]
                .as_array()
                .ok_or("paths is required")?
                .iter()
                .map(|path| {
                    let path = path.as_str().ok_or("paths must be strings")?;
                    paths::canonical(Path::new(path)).map_err(|error| format!("{path}: {error}"))
                })
                .collect::<Result<Vec<PathBuf>, String>>()?;
            let found = scan::scan(&roots, arbiter, false);
            crate::actions::remember_scan(&paths::data_dir(), &roots, &found, lang);
            Ok(json!({ "items": found.iter().map(|found| item(found, lang)).collect::<Vec<_>>() }))
        }
        name => Err(format!("unknown tool {name}")),
    }
}

fn item(assessment: &Assessment, lang: Lang) -> Value {
    json!({
        "path": assessment.path,
        "verdict": assessment.verdict,
        "category": assessment.category,
        "size_bytes": assessment.size,
        "size": assessment.size.map(|bytes| lang::size(bytes, lang)),
        "reasons": assessment.trace(lang),
    })
}

fn status(arbiter: &Arbiter, lang: Lang) -> Value {
    let home = arbiter.home();
    let data_dir = paths::data_dir();
    let index = Index::load(&data_dir).ok().flatten();
    let totals = index.as_ref().map(Index::totals).unwrap_or_default();
    let space = volume::space(home);
    let entries = quarantine::list(home, &data_dir);
    json!({
        "free_bytes": space.map(|space| space.available),
        "total_bytes": space.map(|space| space.total),
        "free": space.map(|space| lang::size(space.available, lang)),
        "level": space.map(|space| format!("{:?}", volume::level(space, totals.rebuildable + totals.online)).to_lowercase()),
        "last_scan": index.as_ref().map(|index| json!({ "at": index.scanned_at, "roots": index.roots })),
        "freeable_bytes": totals.rebuildable,
        "freeable_online_bytes": totals.online,
        "to_review_bytes": totals.to_review,
        "quarantine": { "entries": entries.len(), "bytes": entries.iter().map(Quarantined::size).sum::<u64>() },
        "to_free_space": "ask the developer to run: delest clean --dry-run"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arbiter() -> Arbiter {
        Arbiter::new(PathBuf::from("/nonexistent-home"))
    }

    #[test]
    fn initialize_lists_tools_and_ignores_notifications() {
        let reply = handle(&arbiter(), r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#, Lang::En).unwrap();
        assert_eq!(reply["result"]["protocolVersion"], "2025-03-26");
        assert!(
            handle(
                &arbiter(),
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                Lang::En
            )
            .is_none()
        );
        let tools = handle(
            &arbiter(),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            Lang::En,
        )
        .unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["delest_status", "delest_explain", "delest_scan"]);
        assert!(
            tools["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| tool["annotations"]["readOnlyHint"] == true)
        );
    }

    #[test]
    fn explain_answers_with_the_verdict_and_bad_calls_are_tool_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("__pycache__")).unwrap();
        let path = dir.path().join("__pycache__").display().to_string();
        let request = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"delest_explain","arguments":{"path": path}}});
        let reply = handle(&arbiter(), &request.to_string(), Lang::En).unwrap();
        let text: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text["verdict"], "rebuildable");
        let bad = handle(&arbiter(), r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"delest_explain","arguments":{}}}"#, Lang::En).unwrap();
        assert_eq!(bad["result"]["isError"], true);
        let unknown = handle(
            &arbiter(),
            r#"{"jsonrpc":"2.0","id":5,"method":"nope"}"#,
            Lang::En,
        )
        .unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }
}
