//! The Lean REPL process: Mathlib imported once, then one command per block.

use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{Checked, Failure, Outcome};

/// Where `lake build REPL` leaves the REPL binary, relative to the project.
const REPL_BINARY: &str = ".lake/packages/REPL/.lake/build/bin/repl";
/// The only environment variables the REPL inherits.
const REPL_ENV: [&str; 10] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "ELAN_HOME",
    "ELAN_TOOLCHAIN",
];
/// Bounds the import of Mathlib, which can take minutes.
const IMPORT_TIMEOUT: Duration = Duration::from_secs(600);
/// Bounds one block's check. A check that runs longer fails, and its REPL is dropped.
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// A running REPL inside a Lake project, with Mathlib imported.
pub(crate) struct Repl {
    child: Child,
    stdin: ChildStdin,
    /// The lines of the REPL's output, read by `spawn_reader`.
    lines: Receiver<String>,
    /// The environment right after `import Mathlib`.
    base: u64,
}

impl Repl {
    /// Starts the REPL in the Lake project at `project` and imports Mathlib.
    /// Importing Mathlib takes several seconds and a few GB of memory, so this is
    /// done once per process.
    pub(crate) fn start(project: &Path) -> Result<Repl, Failure> {
        if !project.join("lakefile.toml").is_file() {
            return Err(service(format!(
                "No Lean project at {}. See docs/MLS_STATE.md.",
                project.display()
            )));
        }
        if !project.join(REPL_BINARY).is_file() {
            return Err(service(format!(
                "The Lean REPL is not built. Run `lake build Mathlib REPL` in {}.",
                project.display()
            )));
        }
        let mut command = Command::new(lake());
        // Model-written Lean runs in this process, so it gets no API key or other secret.
        clear_env(&mut command);
        let mut child = command
            .arg("env")
            .arg(project.join(REPL_BINARY))
            .current_dir(project)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| service(format!("Cannot start Lean: {error}")))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        // If the import fails, dropping `repl` kills the child.
        let mut repl = Repl {
            child,
            stdin,
            lines: spawn_reader(stdout),
            base: 0,
        };
        let late = service("Mathlib took too long to load.".to_string());
        let answer = repl.send(&json!({"cmd": "import Mathlib"}), IMPORT_TIMEOUT, late)?;
        repl.base = import_base(&answer)?;
        Ok(repl)
    }

    /// Checks `lean` on top of REPL environment `env`; 0 means the Mathlib base. A check that
    /// runs longer than `CHECK_TIMEOUT` fails. Its answer may still arrive later and would then
    /// be read as the reply to the next command, so the caller must drop this REPL.
    pub(crate) fn check(&mut self, lean: &str, env: u64) -> Result<Checked, Failure> {
        let env = if env == 0 { self.base } else { env };
        let late = Failure {
            message: format!(
                "Lean took longer than {} s on this block.",
                CHECK_TIMEOUT.as_secs()
            ),
            service: false,
        };
        let answer = self.send(&json!({"cmd": lean, "env": env}), CHECK_TIMEOUT, late)?;
        parse_check(&answer)
    }

    /// Writes one command and reads the answer up to its terminating blank line. An answer that
    /// does not arrive within `timeout` fails with `late`.
    fn send(
        &mut self,
        command: &Value,
        timeout: Duration,
        late: Failure,
    ) -> Result<Value, Failure> {
        let mut request = command.to_string();
        request.push_str("\n\n");
        self.stdin.write_all(request.as_bytes()).map_err(stopped)?;
        self.stdin.flush().map_err(stopped)?;
        let deadline = Instant::now() + timeout;
        let mut text = String::new();
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(wait) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => return Err(late),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(service("Lean stopped.".to_string()));
                }
            };
            if matches!(line.as_str(), "\n" | "\r\n") {
                if text.is_empty() {
                    continue;
                }
                break;
            }
            text.push_str(&line);
        }
        serde_json::from_str(&text).map_err(|_| service("Lean sent invalid output.".to_string()))
    }
}

impl Drop for Repl {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The `lake` executable. Apps launched from Finder do not have elan on PATH,
/// so prefer elan's copy when it is installed.
fn lake() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".elan/bin/lake"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("lake"))
}

/// Clears the inherited environment, then passes through the `REPL_ENV` variables that are set.
fn clear_env(command: &mut Command) {
    command.env_clear();
    for name in REPL_ENV {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

/// Reads the REPL's stdout on its own thread, line by line, so that `send` can wait with a
/// deadline. The thread ends at end of file, or when the `Repl` drops the receiver.
fn spawn_reader(stdout: ChildStdout) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("mathstral-lean-output".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                // Zero bytes is end of file; a read error ends the stream the same way.
                if !matches!(reader.read_line(&mut line), Ok(bytes) if bytes > 0) {
                    return;
                }
                if tx.send(line).is_err() {
                    return;
                }
            }
        })
        .expect("MLS Lean reader");
    rx
}

fn service(message: String) -> Failure {
    Failure {
        message,
        service: true,
    }
}

fn stopped(error: io::Error) -> Failure {
    service(format!("Lean stopped: {error}"))
}

/// Every message in an answer, whatever its severity.
fn messages(answer: &Value) -> impl Iterator<Item = &Value> {
    answer["messages"].as_array().into_iter().flatten()
}

fn errors(answer: &Value) -> impl Iterator<Item = &Value> {
    messages(answer).filter(|message| message["severity"] == "error")
}

/// The environment that `import Mathlib` made, or why Mathlib did not load.
fn import_base(answer: &Value) -> Result<u64, Failure> {
    let detail = answer["message"].as_str().map(str::to_string).or_else(|| {
        errors(answer)
            .next()
            .map(|error| error["data"].as_str().unwrap_or("").to_string())
    });
    if let Some(detail) = detail {
        return Err(service(format!("Mathlib did not load: {detail}")));
    }
    answer["env"]
        .as_u64()
        .ok_or_else(|| service("Mathlib did not load: no environment.".to_string()))
}

/// Reads the answer to one checked command. Errors win over sorry.
pub(crate) fn parse_check(answer: &Value) -> Result<Checked, Failure> {
    if let Some(message) = answer["message"].as_str() {
        return Err(service(format!("Lean: {message}")));
    }
    let Some(env) = answer["env"].as_u64() else {
        return Err(service("Lean sent no environment.".to_string()));
    };
    let failures: Vec<String> = errors(answer)
        .map(|error| {
            let line = error["pos"]["line"].as_u64().unwrap_or(0);
            let column = error["pos"]["column"].as_u64().unwrap_or(0);
            let data = error["data"].as_str().unwrap_or("");
            format!("{line}:{column}: {data}")
        })
        .collect();
    if !failures.is_empty() {
        return Ok(Checked {
            env,
            outcome: Outcome::Failed(failures.join("\n")),
        });
    }
    let has_sorry = answer["sorries"]
        .as_array()
        .is_some_and(|sorries| !sorries.is_empty())
        || messages(answer).any(|message| {
            message["data"]
                .as_str()
                .is_some_and(|data| data.contains("declaration uses 'sorry'"))
        });
    let outcome = if has_sorry {
        Outcome::Sorry
    } else {
        Outcome::Verified
    };
    Ok(Checked { env, outcome })
}

#[cfg(test)]
#[path = "../tests/unit/repl.rs"]
mod tests;
