//! The inference engine: llama.cpp's server, started on demand for one task with
//! one model from the store, reached on the loopback, and stopped afterwards. No
//! process stays behind.

use std::env;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Overrides the server program.
pub const ENV_SERVER: &str = "DELEST_LLAMA_SERVER";
const STARTUP: Duration = Duration::from_secs(180);
const ANSWER: Duration = Duration::from_secs(300);

/// `$DELEST_LLAMA_SERVER`, else `<store>/bin/llama-server` (put there by hand), else the
/// build installed from the app, else an installed `llama-server`.
pub fn server_program(store: &Path) -> Option<PathBuf> {
    if let Some(path) = env::var_os(ENV_SERVER).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let bundled = store
        .join("bin")
        .join(crate::paths::executable_name("llama-server"));
    if bundled.is_file() {
        return Some(bundled);
    }
    crate::install::installed_engine(store).or_else(|| crate::paths::find_program("llama-server"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Embedding,
    Chat,
}

pub struct Engine {
    child: Child,
    port: u16,
}

fn free_port() -> io::Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}

impl Engine {
    /// Starts the server on `model` and waits until it answers.
    pub fn start(program: &Path, model: &Path, mode: Mode, threads: usize) -> io::Result<Engine> {
        let port = free_port()?;
        let mut command = Command::new(program);
        command
            .arg("-m")
            .arg(model)
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "-c",
                "4096",
            ])
            .args(["-t", &threads.max(1).to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if mode == Mode::Embedding {
            // A whole description must fit in one batch to be embedded.
            command.args(["--embedding", "-b", "2048", "-ub", "2048"]);
        }
        let child = command.spawn()?;
        let mut engine = Engine { child, port };
        let started = Instant::now();
        loop {
            if let Ok((200, _)) = engine.request("GET", "/health", None) {
                return Ok(engine);
            }
            if let Ok(Some(status)) = engine.child.try_wait() {
                return Err(io::Error::other(format!(
                    "llama-server stopped at start ({status})"
                )));
            }
            if started.elapsed() > STARTUP {
                return Err(io::Error::other("llama-server did not answer in time"));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// The text's vector, normalised.
    pub fn embed(&mut self, text: &str) -> io::Result<Vec<f32>> {
        let (status, body) =
            self.request("POST", "/embedding", Some(json!({ "content": text })))?;
        if status != 200 {
            return Err(io::Error::other(format!(
                "embedding failed ({status}): {body}"
            )));
        }
        let value: Value = serde_json::from_str(&body).map_err(io::Error::other)?;
        // Either [{"embedding": [[…]]}] or {"embedding": […]}, depending on the version.
        let embedding = match &value {
            Value::Array(items) => items.first().map(|item| &item["embedding"]),
            object => Some(&object["embedding"]),
        }
        .ok_or_else(|| io::Error::other("no embedding in the answer"))?;
        let numbers = match embedding {
            Value::Array(rows) if rows.first().is_some_and(Value::is_array) => &rows[0],
            other => other,
        };
        let vector: Vec<f32> = numbers
            .as_array()
            .ok_or_else(|| io::Error::other("the embedding is not a list"))?
            .iter()
            .filter_map(|number| number.as_f64().map(|n| n as f32))
            .collect();
        let norm = vector
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt()
            .max(f32::EPSILON);
        Ok(vector.into_iter().map(|x| x / norm).collect())
    }

    /// One answer to one message, held to `grammar` (GBNF) when given.
    pub fn chat(
        &mut self,
        message: &str,
        grammar: Option<&str>,
        max_tokens: u32,
    ) -> io::Result<String> {
        let mut body = json!({
            "messages": [{ "role": "user", "content": message }],
            "temperature": 0,
            "max_tokens": max_tokens,
            "chat_template_kwargs": { "enable_thinking": false }
        });
        if let Some(grammar) = grammar {
            body["grammar"] = json!(grammar);
        }
        let (status, text) = self.request("POST", "/v1/chat/completions", Some(body))?;
        if status != 200 {
            return Err(io::Error::other(format!("chat failed ({status}): {text}")));
        }
        let value: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
        value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| io::Error::other("no answer in the reply"))
    }

    /// A plain HTTP/1.1 exchange on the loopback; the answer's status and body.
    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> io::Result<(u16, String)> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(ANSWER))?;
        let payload = body.map(|body| body.to_string()).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        )?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        parse_response(&raw)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse_response(raw: &[u8]) -> io::Result<(u16, String)> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| io::Error::other("malformed HTTP answer"))?;
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| io::Error::other("no HTTP status"))?;
    let chunked = head.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("transfer-encoding:") && line.contains("chunked")
    });
    Ok((
        status,
        if chunked {
            dechunk(body)
        } else {
            body.to_string()
        },
    ))
}

fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_chunked_answers() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(plain).unwrap(), (200, "{}".into()));
        let chunked = b"HTTP/1.1 503 Service Unavailable\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(chunked).unwrap(), (503, "{\"a\":1}".into()));
    }
}
