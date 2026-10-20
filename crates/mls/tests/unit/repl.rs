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
