//! The Lean REPL process: Mathlib imported once, then one command per block.

use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

use crate::{Checked, Failure, Outcome};

/// Where `lake build REPL` leaves the REPL binary, relative to the project.
const REPL_BINARY: &str = ".lake/packages/REPL/.lake/build/bin/repl";

/// A running REPL inside a Lake project, with Mathlib imported.
pub(crate) struct Repl {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
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
        let mut child = Command::new(lake())
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
            stdout: BufReader::new(stdout),
            base: 0,
        };
        let answer = repl.send(&json!({"cmd": "import Mathlib"}))?;
        repl.base = import_base(&answer)?;
        Ok(repl)
    }

    /// Checks `lean` on top of REPL environment `env`; 0 means the Mathlib base.
    /// There is no timeout: Lean's own maxHeartbeats bounds elaboration.
    pub(crate) fn check(&mut self, lean: &str, env: u64) -> Result<Checked, Failure> {
        let env = if env == 0 { self.base } else { env };
        let answer = self.send(&json!({"cmd": lean, "env": env}))?;
        parse_check(&answer)
    }

    /// Writes one command and reads the answer up to its terminating blank line.
    fn send(&mut self, command: &Value) -> Result<Value, Failure> {
        let mut request = command.to_string();
        request.push_str("\n\n");
        self.stdin.write_all(request.as_bytes()).map_err(stopped)?;
        self.stdin.flush().map_err(stopped)?;
        let mut text = String::new();
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line).map_err(stopped)? == 0 {
                return Err(service("Lean stopped.".to_string()));
            }
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
