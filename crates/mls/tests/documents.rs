//! The sample documents run through OpenRouter (Jev and the formalizer) and the Lean REPL. Needs the network,
//! an OpenRouter key and the Lean project built. Run with
//! `cargo test --release -p mathstral-mls --test documents -- --ignored --nocapture`.
//!
//! Each document runs `MLS_LIVE_RUNS` times (default 3), each time with a fresh worker and Mls. Raw
//! results go to `target/mls-live/<phase>/`: per document and run, a JSON record of every job and
//! reply, the final dots and the chart, plus the Mls twin. Environment variables:
//! `MLS_LIVE_RUNS` (positive integer), `MLS_LIVE_FILTER` (case-insensitive substring of a document
//! name), `MLS_LIVE_PHASE` (directory name under `target/mls-live`; default `unix-<seconds>`).

use mathstral_mls::{
    Dot, FORMALIZER_MODEL, JEV_MODEL, Job, Label, Mls, Paragraph, Reply, Status, Worker,
};

#[path = "fixtures/documents.rs"]
mod documents;

use documents::{DOCUMENTS, LIVE_DOCUMENTS, Observed, drive_observed, lean_project, lean_ready};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const DEFAULT_RUNS: usize = 3;

/// A document as this test runs it: each paragraph's text and the dot it must end with.
struct Case {
    name: &'static str,
    paragraphs: Vec<(&'static str, Option<Status>)>,
}

fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = DOCUMENTS
        .iter()
        .map(|document| Case {
            name: document.name,
            paragraphs: document
                .lines
                .iter()
                .map(|line| (line.text, line.expect))
                .collect(),
        })
        .collect();
    cases.extend(LIVE_DOCUMENTS.iter().map(|document| {
        Case {
            name: document.name,
            paragraphs: document
                .lines
                .iter()
                .map(|line| (line.text, line.expect))
                .collect(),
        }
    }));
    cases
}

fn runs() -> usize {
    match env::var("MLS_LIVE_RUNS") {
        Ok(value) => value
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|&runs| runs >= 1)
            .unwrap_or_else(|| {
                panic!("MLS_LIVE_RUNS must be a whole number of at least 1, got {value:?}")
            }),
        Err(env::VarError::NotPresent) => DEFAULT_RUNS,
        Err(error) => panic!("MLS_LIVE_RUNS is not readable: {error}"),
    }
}

fn selected(cases: Vec<Case>) -> Vec<Case> {
    let filter = match env::var("MLS_LIVE_FILTER") {
        Ok(filter) => Some(filter.to_lowercase()),
        Err(env::VarError::NotPresent) => None,
        Err(error) => panic!("MLS_LIVE_FILTER is not readable: {error}"),
    };
    let selected: Vec<Case> = cases
        .into_iter()
        .filter(|case| {
            filter
                .as_deref()
                .is_none_or(|filter| case.name.to_lowercase().contains(filter))
        })
        .collect();
    assert!(!selected.is_empty(), "MLS_LIVE_FILTER matches no document");
    selected
}

fn phase() -> String {
    match env::var("MLS_LIVE_PHASE") {
        Ok(phase) => {
            let allowed = phase
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte));
            assert!(
                allowed && !phase.is_empty() && !phase.starts_with('.'),
                "MLS_LIVE_PHASE may use only letters, digits, '-', '_' and '.', and not start with '.'"
            );
            phase
        }
        Err(env::VarError::NotPresent) => {
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is after 1970")
                .as_secs();
            format!("unix-{seconds}")
        }
        Err(error) => panic!("MLS_LIVE_PHASE is not readable: {error}"),
    }
}

/// Raw results for a phase: under the repository's `target/`, never in tracked source.
fn output_dir(phase: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/mls-live"
    ))
    .join(phase)
}

/// Lowercase file-name stem for a document name.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn status_json(status: Option<Status>) -> Value {
    status.map_or(Value::Null, |status| json!(format!("{status:?}")))
}

fn failure_json(failure: &mathstral_mls::Failure) -> Value {
    json!({"message": failure.message, "service": failure.service})
}

/// Each job and reply as its Debug text, plus the fields of a classification or formalization.
fn event_json(event: Observed<'_>) -> Value {
    match event {
        Observed::Job(job) => json!({"kind": "job", "debug": format!("{job:?}")}),
        Observed::Reply(reply) => {
            let mut value = json!({"kind": "reply", "debug": format!("{reply:?}")});
            match reply {
                Reply::Classified {
                    paragraph, result, ..
                } => {
                    value["classification"] = match result {
                        Ok(tag) => json!({
                            "paragraph": paragraph,
                            "label": tag.label.name(),
                            "joins": tag.joins,
                        }),
                        Err(failure) => json!({
                            "paragraph": paragraph,
                            "failure": failure_json(failure),
                        }),
                    };
                }
                Reply::Formalized {
                    block,
                    version,
                    result,
                    ..
                } => {
                    value["formalization"] = match result {
                        Ok(formal) => json!({
                            "block": block,
                            "version": version,
                            "lean": formal.lean,
                            "lean_name": formal.lean_name,
                            "assumptions": formal.assumptions,
                            "uses": formal.uses,
                        }),
                        Err(failure) => json!({
                            "block": block,
                            "version": version,
                            "failure": failure_json(failure),
                        }),
                    };
                }
                Reply::Checked { .. } | Reply::Split { .. } => {}
                Reply::Repaired { job, result } => {
                    if let Job::Repair {
                        block,
                        version,
                        failed,
                        error,
                        ..
                    } = job
                    {
                        value["repair"] = json!({
                            "block": block,
                            "version": version,
                            "failed_lean": failed,
                            "error": error,
                            "candidate": match result {
                                Ok(formal) => json!({
                                    "lean": formal.lean,
                                    "uses": formal.uses,
                                }),
                                Err(failure) => json!({"failure": failure_json(failure)}),
                            },
                        });
                    }
                }
            }
            value
        }
    }
}

fn dot_json(mls: &Mls, dot: &Dot) -> Value {
    let details = mls.details(dot.key).map(|details| {
        json!({
            "assumptions": details.assumptions,
            "lean": details.lean,
            "error": details.error,
            "service": details.service,
            "uses": details.uses,
            "note": details.note,
        })
    });
    json!({
        "paragraph": dot.paragraph,
        "last": dot.last,
        "key": dot.key,
        "status": format!("{:?}", dot.status),
        "details": details,
    })
}

/// The dots that differ from the expected ones, each as a message naming the paragraph.
fn compare(mls: &Mls, case: &Case) -> Vec<String> {
    let dots = mls.dots();
    case.paragraphs
        .iter()
        .enumerate()
        .filter_map(|(index, (text, expect))| {
            let actual = dots
                .iter()
                .find(|dot| dot.paragraph == index)
                .map(|dot| dot.status);
            (actual != *expect)
                .then(|| format!("paragraph {index} expected {expect:?} got {actual:?}: {text}"))
        })
        .collect()
}

fn save(dir: &Path, case: &Case, run: usize, record: &Value, twin: &str) {
    let stem = format!("{}-run{run}", slug(case.name));
    let json_path = dir.join(format!("{stem}.json"));
    let text = serde_json::to_string_pretty(record).expect("a JSON value serialises");
    fs::write(&json_path, text)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", json_path.display()));
    let twin_path = dir.join(format!("{stem}.mls.lean"));
    fs::write(&twin_path, twin)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", twin_path.display()));
}

#[test]
#[ignore = "calls OpenRouter: needs OPENROUTER_API_KEY and the Lean project in lean/"]
fn live_documents_end_with_their_dots() {
    // The key is only checked for presence here; it is never printed or logged.
    assert!(
        env::var("OPENROUTER_API_KEY").is_ok_and(|key| !key.trim().is_empty()),
        "OPENROUTER_API_KEY is not set: the live test calls OpenRouter"
    );
    let project = lean_project();
    assert!(
        lean_ready(&project),
        "the Lean REPL is not built in {} (run `lake build Mathlib repl` there)",
        project.display()
    );
    let runs = runs();
    let cases = selected(cases());
    let phase = phase();
    let dir = output_dir(&phase);
    fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("cannot create {}: {error}", dir.display()));

    let mut mismatches = Vec::new();
    for run in 1..=runs {
        for case in &cases {
            let worker = Worker::new(project.clone(), || {});
            worker.set_key(env::var("OPENROUTER_API_KEY").ok());
            let mut mls = Mls::new();
            let texts: Vec<&str> = case.paragraphs.iter().map(|(text, _)| *text).collect();
            let mut events = Vec::new();
            let result = drive_observed(&mut mls, &worker, &texts, &mut |event| {
                events.push(event_json(event));
            });

            let mut problems = Vec::new();
            if let Err(error) = &result {
                problems.push(format!("the run stopped: {error}"));
            }
            problems.extend(compare(&mls, case));

            let dots: Vec<Value> = mls.dots().iter().map(|dot| dot_json(&mls, dot)).collect();
            let chart: Vec<Value> = mls
                .chart()
                .iter()
                .map(|node| {
                    json!({
                        "key": node.key,
                        "paragraph": node.paragraph,
                        "label": node.label.name(),
                        "name": node.name,
                        "status": format!("{:?}", node.status),
                        "uses": node.uses,
                        "level": node.level,
                    })
                })
                .collect();
            let paragraphs: Vec<Value> = case
                .paragraphs
                .iter()
                .enumerate()
                .map(|(index, (text, expect))| {
                    json!({"index": index, "text": text, "expect": status_json(*expect)})
                })
                .collect();
            let record = json!({
                "document": case.name,
                "phase": phase,
                "run": run,
                "models": {"jev": JEV_MODEL, "formalizer": FORMALIZER_MODEL},
                "passed": problems.is_empty(),
                "problems": problems,
                "paragraphs": paragraphs,
                "events": events,
                "dots": dots,
                "chart": chart,
            });
            save(&dir, case, run, &record, &mls.twin());
            eprintln!(
                "{} run {run}: {} problem(s), saved to {}",
                case.name,
                problems.len(),
                dir.display()
            );
            mismatches.extend(
                problems
                    .iter()
                    .map(|problem| format!("{} run {run}: {problem}", case.name)),
            );
        }
    }
    eprintln!("raw results: {}", dir.display());
    assert!(
        mismatches.is_empty(),
        "{} mismatch(es):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
