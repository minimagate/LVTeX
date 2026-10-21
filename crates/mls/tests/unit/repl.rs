use super::*;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

/// One REPL message at `line:column`.
fn message(severity: &str, line: u64, column: u64, data: &str) -> Value {
    json!({"severity": severity, "pos": {"line": line, "column": column}, "data": data})
}

#[test]
fn parse_check_reads_each_answer_shape() {
    let cases = [
        (
            json!({"env": 4}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Verified,
            }),
        ),
        (
            json!({"env": 4, "messages": [message("error", 1, 8, "unknown identifier 'x'")]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Failed("1:8: unknown identifier 'x'".to_string()),
            }),
        ),
        (
            json!({"env": 4, "messages": [
                message("error", 1, 8, "a"),
                message("error", 2, 0, "b"),
            ]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Failed("1:8: a\n2:0: b".to_string()),
            }),
        ),
        (
            json!({"env": 4, "messages": [message("warning", 1, 0, "declaration uses 'sorry'")]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Sorry,
            }),
        ),
        (
            json!({"env": 4, "sorries": [{"pos": {"line": 1, "column": 22}, "goal": "⊢ True", "proofState": 0}]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Sorry,
            }),
        ),
        (
            json!({"env": 4, "sorries": []}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Verified,
            }),
        ),
        (
            json!({"env": 4, "messages": [message("info", 1, 0, "1")]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Verified,
            }),
        ),
        (
            json!({"env": 4, "messages": [
                message("error", 1, 0, "x"),
                message("warning", 1, 0, "declaration uses 'sorry'"),
            ], "sorries": [{"pos": {"line": 1, "column": 0}, "goal": "", "proofState": 0}]}),
            Ok(Checked {
                env: 4,
                outcome: Outcome::Failed("1:0: x".to_string()),
            }),
        ),
        (
            json!({"message": "Unknown environment."}),
            Err(Failure {
                message: "Lean: Unknown environment.".to_string(),
                service: true,
            }),
        ),
        (
            json!({"messages": []}),
            Err(Failure {
                message: "Lean sent no environment.".to_string(),
                service: true,
            }),
        ),
    ];
    for (answer, expected) in cases {
        assert_eq!(parse_check(&answer), expected, "{answer}");
    }
}

/// A fresh directory under the system temp dir, named for one test.
fn scratch_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mathstral-repl-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[test]
fn start_fails_without_a_lakefile() {
    let dir = scratch_dir("no-lakefile");
    let failure = Repl::start(&dir).err().expect("start should fail");
    fs::remove_dir_all(&dir).expect("remove scratch dir");
    assert!(failure.service);
    assert!(
        failure.message.contains("No Lean project"),
        "{}",
        failure.message
    );
}

#[test]
fn start_fails_when_the_repl_is_not_built() {
    let dir = scratch_dir("not-built");
    fs::write(dir.join("lakefile.toml"), "").expect("write lakefile");
    let failure = Repl::start(&dir).err().expect("start should fail");
    fs::remove_dir_all(&dir).expect("remove scratch dir");
    assert!(failure.service);
    assert!(failure.message.contains("not built"), "{}", failure.message);
}

/// A REPL whose child is `sh -c script`, so the reply loop can be tested without Lake.
fn fake_repl(script: &str) -> Repl {
    let mut child = Command::new("sh")
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("sh runs");
    let stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    Repl {
        child,
        stdin,
        lines: spawn_reader(stdout),
        base: 0,
    }
}

#[test]
fn an_answer_that_is_late_fails_with_the_late_failure() {
    let mut repl = fake_repl("exec sleep 30");
    let late = Failure {
        message: "too late".to_string(),
        service: false,
    };
    let answer = repl.send(
        &json!({"cmd": "x"}),
        Duration::from_millis(100),
        late.clone(),
    );
    assert_eq!(answer, Err(late));
}

#[test]
fn a_repl_that_exits_without_answering_is_stopped() {
    let mut repl = fake_repl("read line; exit 0");
    let late = Failure {
        message: "too late".to_string(),
        service: false,
    };
    let answer = repl.send(&json!({"cmd": "x"}), Duration::from_secs(10), late);
    assert_eq!(answer, Err(service("Lean stopped.".to_string())));
}

#[test]
fn the_repl_inherits_only_the_allowed_environment() {
    let mut command = Command::new("env");
    clear_env(&mut command);
    let output = command.output().expect("env runs");
    let printed = String::from_utf8(output.stdout).expect("env prints UTF-8");
    for line in printed.lines() {
        let name = line.split('=').next().unwrap_or_default();
        assert!(REPL_ENV.contains(&name), "{name} is inherited");
    }
}

#[test]
fn a_missing_lake_says_lean_is_not_installed() {
    let lake = Path::new("/nonexistent/mathstral-test/lake");
    let failure = lake_env(lake, Path::new("."), "LEAN_PATH").expect_err("lake_env should fail");
    assert!(failure.service);
    assert_eq!(
        failure.message,
        "Lean is not installed: `lake` was not found. Install elan and build the Lean project (see README)."
    );
}
