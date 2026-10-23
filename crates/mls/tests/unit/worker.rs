//! Worker tests. The scripted document test needs the Lean project built; run it with
//! `cargo test --release -p lvtex-mls scripted_documents -- --ignored --nocapture`.

use super::{Ask, Worker};
use crate::openrouter::{CHAT_URL, DECISIONS_URL, formalize_request, refute_request};
use crate::{
    Checked, Failure, Formal, Job, Label, Mls, Outcome, Paragraph, Reply, Segment, SplitContext,
    Status, Tag, rules,
};
use documents::{DOCUMENTS, Line, assert_outcome, drive, lean_project, lean_ready};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[path = "../fixtures/documents.rs"]
mod documents;

const TIMEOUT: Duration = Duration::from_secs(10);
const NOT_SPLIT: &str =
    "Invalid labeled segmentation: items must reproduce the whole paragraph in order.";

fn reply(worker: &Worker) -> Reply {
    worker
        .rx
        .recv_timeout(TIMEOUT)
        .expect("a reply within 10 seconds")
}

/// Jev's answers: the label, whether the paragraph continues the one above, and that it holds one
/// item.
fn jev(label: Label, joins: bool) -> Value {
    let joins = if joins { "continues" } else { "new" };
    json!({"answers": {
        "kind": {"type": "choice", "choice": label.name()},
        "joins": {"type": "choice", "choice": joins},
        "independent_items": {"type": "choice", "choice": "one"},
    }})
}

/// A formalization chat completion whose content is the formalization `lean`.
fn formalization_reply(lean: &str) -> Value {
    let content = json!({"lean": lean, "lean_name": null, "assumptions": ["A plain sentence."]});
    json!({"choices": [{"finish_reason": "stop", "message": {"content": content.to_string()}}]})
}

fn formalize(label: Label, text: &str) -> Job {
    Job::Formalize {
        block: 3,
        version: 9,
        label,
        text: text.to_string(),
        above: None,
        lean_above: Vec::new(),
        names_above: Vec::new(),
        statement: None,
        base: 0,
    }
}

/// A worker whose requests get `replies` in turn, and the channel of the request bodies it sent.
/// A request past the last reply fails as an unreachable service would.
fn worker_replying(replies: Vec<Result<Value, Failure>>) -> (Worker, mpsc::Receiver<Value>) {
    let (sent, bodies) = mpsc::channel();
    let mut replies = replies.into_iter();
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        move |_, body| {
            let _ = sent.send(body.clone());
            replies.next().unwrap_or_else(|| Err(unreachable()))
        },
    );
    (worker, bodies)
}

fn unreachable() -> Failure {
    Failure {
        message: "OpenRouter is unreachable: no route".to_string(),
        service: true,
    }
}

/// The text of the last message of a request: the block, with any retry after it.
fn prompt(body: &Value) -> &str {
    body["messages"][1]["content"]
        .as_array()
        .and_then(|parts| parts.last())
        .and_then(|part| part["text"].as_str())
        .unwrap_or_default()
}

/// The request for each ask about the definition "Let d be 1.", as `run` builds it.
fn definition_ask(ask: Ask<'_>) -> Value {
    match ask {
        Ask::Formalize(retry) => {
            formalize_request(Label::Definition, "Let d be 1.", None, &[], None, retry)
        }
        Ask::Refute { .. } => panic!("a definition is never refuted"),
    }
}

/// A request that fails the way an unreachable service does.
fn no_request(_url: &str, _body: &Value) -> Result<Value, Failure> {
    Err(Failure {
        message: "no request expected".to_string(),
        service: true,
    })
}

/// The statement of the regression: a proof whose `nlinarith` failed after a redundant `simp`.
const STATEMENT: &str = "theorem quadraticEnergy_add_of_areOrthogonal (u v : ℝ × ℝ) (h : AreOrthogonal u v) : quadraticEnergy (u+v)=quadraticEnergy u+quadraticEnergy v := by sorry";
const HEADER: &str = "theorem quadraticEnergy_add_of_areOrthogonal (u v : ℝ × ℝ) (h : AreOrthogonal u v) : quadraticEnergy (u+v)=quadraticEnergy u+quadraticEnergy v :=";
const PROSE: &str = "Expand both sides in coordinates and use the orthogonality of $u$ and $v$. ∎";
/// The candidate that Lean rejected, and Lean's error for its line 7.
const FAILED: &str = "theorem quadraticEnergy_add_of_areOrthogonal (u v : ℝ × ℝ) (h : AreOrthogonal u v) : quadraticEnergy (u+v)=quadraticEnergy u+quadraticEnergy v := by\n  obtain ⟨a, b⟩ := u\n  obtain ⟨c, d⟩ := v\n  unfold AreOrthogonal scalarProductB at h\n  unfold quadraticEnergy\n  simp only [Prod.mk_add_mk] at *\n  simp only at h ⊢\n  nlinarith [h]";
const ERROR: &str = "7:2: `simp` made no progress";
/// The corrected candidate: the redundant `simp` is gone.
const CORRECTED: &str = "theorem quadraticEnergy_add_of_areOrthogonal (u v : ℝ × ℝ) (h : AreOrthogonal u v) : quadraticEnergy (u+v)=quadraticEnergy u+quadraticEnergy v := by\n  obtain ⟨a, b⟩ := u\n  obtain ⟨c, d⟩ := v\n  unfold AreOrthogonal scalarProductB at h\n  unfold quadraticEnergy\n  simp only [Prod.mk_add_mk] at *\n  nlinarith [h]";
/// The definitions above the regression's proof.
const DEFINITIONS: [&str; 3] = [
    "def scalarProductB (u v : ℝ × ℝ) : ℝ := u.1 * v.1 + u.2 * v.2",
    "def AreOrthogonal (u v : ℝ × ℝ) : Prop := scalarProductB u v = 0",
    "def quadraticEnergy (u : ℝ × ℝ) : ℝ := u.1^2 + u.2^2",
];

/// The job that formalizes the regression's proof, with the definitions above it, checked on base 7.
fn proof_job() -> Job {
    Job::Formalize {
        block: 4,
        version: 12,
        label: Label::Proof,
        text: format!("*Proof.* {PROSE}"),
        above: Some("We keep the notation of the definitions above.".to_string()),
        lean_above: DEFINITIONS
            .iter()
            .enumerate()
            .map(|(index, lean)| (index as u64 + 1, lean.to_string()))
            .collect(),
        names_above: vec![
            "scalarProductB".to_string(),
            "AreOrthogonal".to_string(),
            "quadraticEnergy".to_string(),
        ],
        statement: Some((5, STATEMENT.to_string())),
        base: 7,
    }
}

#[test]
fn a_rejected_formalization_is_asked_again_with_the_lean_and_the_reason() {
    let (worker, bodies) = worker_replying(vec![
        Ok(formalization_reply("axiom t : True")),
        Ok(formalization_reply("theorem t : True := by sorry")),
    ]);
    worker.submit(formalize(Label::Statement, "Then t holds."));
    let Reply::Formalized { result, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    let requests: Vec<Value> = bodies.try_iter().collect();
    assert_eq!(requests.len(), 2);
    let retry = prompt(&requests[1]);
    assert!(retry.contains("axiom t : True"), "{retry}");
    assert!(retry.contains("The Lean declares an axiom."), "{retry}");
    assert_eq!(
        result.map(|formal| formal.lean),
        Ok("theorem t : True := by sorry".to_string())
    );
}

#[test]
fn a_malformed_reply_is_asked_again_with_its_reason() {
    let cut_off = json!({"choices": [{"finish_reason": "length",
        "message": {"content": "{\"lean\": \"theorem t : Tr"}}]});
    let (worker, bodies) = worker_replying(vec![
        Ok(cut_off),
        Ok(formalization_reply("theorem t : True := by sorry")),
    ]);
    worker.submit(formalize(Label::Statement, "Then t holds."));
    let Reply::Formalized { result, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    let requests: Vec<Value> = bodies.try_iter().collect();
    assert_eq!(requests.len(), 2);
    let retry = prompt(&requests[1]);
    assert!(retry.contains("(no usable answer)"), "{retry}");
    assert!(retry.contains("The formalization was cut off."), "{retry}");
    assert!(
        retry.contains("Keep the Lean as short as the block allows."),
        "{retry}"
    );
    assert_eq!(
        result.map(|formal| formal.lean),
        Ok("theorem t : True := by sorry".to_string())
    );
}

#[test]
fn a_rejected_proof_is_asked_again_with_its_statement_context_and_the_reason() {
    let (worker, bodies) = worker_replying(vec![
        Ok(formalization_reply("axiom extra : True")),
        Ok(formalization_reply(CORRECTED)),
    ]);
    worker.submit(proof_job());
    let Reply::Formalized { result, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    let requests: Vec<Value> = bodies.try_iter().collect();
    assert_eq!(requests.len(), 2);
    let body = requests[1].to_string();
    for part in [
        PROSE,
        STATEMENT,
        DEFINITIONS[2],
        "axiom extra : True",
        "The Lean declares an axiom.",
        "Label: proof",
        "Your previous formalization of this block was not accepted",
        "keep the statement's name, binders and type exactly",
    ] {
        assert!(body.contains(part), "the retry lacks {part:?}");
    }
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn formalization_stops_after_three_rejected_attempts() {
    let (worker, bodies) = worker_replying(vec![Ok(formalization_reply("axiom t : True")); 3]);
    worker.submit(formalize(Label::Statement, "Then t holds."));
    let Reply::Formalized { result, check, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    assert_eq!(bodies.try_iter().count(), 3);
    assert_eq!(
        result,
        Err(Failure {
            message: "The Lean declares an axiom.".to_string(),
            service: false,
        })
    );
    assert_eq!(check, None);
}

#[test]
fn a_service_failure_stops_the_attempts_at_once() {
    let (worker, bodies) = worker_replying(vec![
        Err(unreachable()),
        Ok(formalization_reply("theorem t : True := by sorry")),
    ]);
    worker.submit(formalize(Label::Statement, "Then t holds."));
    let Reply::Formalized { result, check, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    assert_eq!(bodies.try_iter().count(), 1);
    assert_eq!(result, Err(unreachable()));
    assert_eq!(check, None);
}

#[test]
fn a_blank_definition_is_not_checked() {
    let (worker, bodies) = worker_replying(vec![Ok(formalization_reply(""))]);
    worker.submit(formalize(
        Label::Definition,
        "Throughout, n is a natural number.",
    ));
    let Reply::Formalized { result, check, .. } = reply(&worker) else {
        panic!("expected a formalization")
    };
    assert_eq!(bodies.try_iter().count(), 1);
    assert_eq!(result.map(|formal| formal.lean), Ok(String::new()));
    assert_eq!(check, None);
}

#[test]
fn an_accepted_proof_is_echoed_with_its_block_and_the_names_it_uses() {
    let (worker, _bodies) = worker_replying(vec![Ok(formalization_reply(CORRECTED))]);
    worker.submit(proof_job());
    match reply(&worker) {
        Reply::Formalized {
            block,
            version,
            statement_version,
            result,
            check,
        } => {
            assert_eq!((block, version, statement_version), (4, 12, Some(5)));
            assert_eq!(
                result.map(|formal| formal.uses),
                Ok(vec![
                    "scalarProductB".to_string(),
                    "AreOrthogonal".to_string(),
                    "quadraticEnergy".to_string(),
                ])
            );
            // No Lean project is set up here, so the check is the service failure it reports.
            assert!(
                matches!(check, Some((7, Err(Failure { service: true, .. })))),
                "{check:?}"
            );
        }
        other => panic!("expected a formalization, got {other:?}"),
    }
}

/// The result of formalizing the regression's proof when every reply is `lean`.
fn proof_result(lean: &str) -> Result<Formal, Failure> {
    let (worker, _bodies) = worker_replying(vec![Ok(formalization_reply(lean)); super::ATTEMPTS]);
    worker.submit(proof_job());
    match reply(&worker) {
        Reply::Formalized { result, .. } => result,
        other => panic!("expected a formalization, got {other:?}"),
    }
}

#[test]
fn a_proof_is_checked_by_the_same_rules_as_a_formalization() {
    let cases = [
        (format!("{HEADER} by\n  sorry"), "The proof uses sorry."),
        (
            format!("{HEADER} by\n  native_decide"),
            "The Lean uses `native_decide`, which is not allowed.",
        ),
        (
            "theorem other_name (u v : ℝ × ℝ) : True := by\n  trivial".to_string(),
            "The proof changes the statement above.",
        ),
    ];
    for (lean, message) in cases {
        assert_eq!(
            proof_result(&lean),
            Err(Failure {
                message: message.to_string(),
                service: false,
            }),
            "{lean}"
        );
    }
}

#[test]
fn a_lean_error_is_asked_again_and_the_accepted_attempt_is_checked_on_the_base() {
    let mut requests = Vec::new();
    let mut replies = vec![
        formalization_reply("def d := 1"),
        formalization_reply("def d := 2"),
    ]
    .into_iter();
    let mut request = |_: &str, body: &Value| -> Result<Value, Failure> {
        requests.push(body.clone());
        Ok(replies.next().expect("a reply for each request"))
    };
    let mut checks = Vec::new();
    let mut outcomes =
        vec![Outcome::Failed("1:0: error".to_string()), Outcome::Verified].into_iter();
    let mut check = |lean: &str, base: u64| -> Result<Checked, Failure> {
        checks.push((lean.to_string(), base));
        Ok(Checked {
            env: 5,
            outcome: outcomes.next().expect("an outcome for each check"),
        })
    };
    let (result, checked) = super::formalize(
        &mut request,
        &mut check,
        definition_ask,
        Label::Definition,
        &[],
        None,
        7,
    );
    assert_eq!(requests.len(), 2);
    let retry = prompt(&requests[1]);
    assert!(retry.contains("def d := 1"), "{retry}");
    assert!(retry.contains("1:0: error"), "{retry}");
    assert_eq!(
        checks,
        [("def d := 1".to_string(), 7), ("def d := 2".to_string(), 7)]
    );
    assert_eq!(
        result.map(|formal| formal.lean),
        Ok("def d := 2".to_string())
    );
    assert_eq!(
        checked,
        Some((
            7,
            Ok(Checked {
                env: 5,
                outcome: Outcome::Verified,
            })
        ))
    );
}

#[test]
fn a_timed_out_check_is_returned_at_once_without_asking_again() {
    let mut requests = 0;
    let mut request = |_: &str, _: &Value| -> Result<Value, Failure> {
        requests += 1;
        Ok(formalization_reply("def d := 1"))
    };
    let timeout = Failure {
        message: "The Lean check timed out.".to_string(),
        service: false,
    };
    let mut check = |_: &str, _: u64| -> Result<Checked, Failure> { Err(timeout.clone()) };
    let (result, checked) = super::formalize(
        &mut request,
        &mut check,
        definition_ask,
        Label::Definition,
        &[],
        None,
        7,
    );
    assert_eq!(requests, 1);
    assert_eq!(
        result.map(|formal| formal.lean),
        Ok("def d := 1".to_string())
    );
    assert_eq!(checked, Some((7, Err(timeout))));
}

#[test]
fn classify_reports_the_label_jev_chose() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        move |url, body| {
            assert_eq!(url, DECISIONS_URL);
            assert_eq!(body["state"]["paragraph"], "Lemma. Every group is nice.");
            assert_eq!(body["state"]["block_above"], "Let G be a group.");
            match counted.fetch_add(1, Ordering::SeqCst) {
                0 => {
                    assert_eq!(body["questions"].as_object().unwrap().len(), 1);
                    assert!(body["questions"].get("independent_items").is_some());
                    Ok(json!({"answers": {"independent_items": {"choice": "one"}}}))
                }
                1 => {
                    assert_eq!(body["questions"].as_object().unwrap().len(), 2);
                    assert!(body["questions"].get("kind").is_some());
                    Ok(jev(Label::Statement, true))
                }
                _ => panic!("only gate and kind/joins are requested"),
            }
        },
    );
    worker.submit(Job::Classify {
        paragraph: 7,
        previous: Some("Let G be a group.".to_string()),
        text: "Lemma. Every group is nice.".to_string(),
        proof_open: false,
    });
    match reply(&worker) {
        Reply::Classified {
            paragraph,
            previous,
            text,
            proof_open,
            result,
        } => {
            assert_eq!(paragraph, 7);
            assert_eq!(previous.as_deref(), Some("Let G be a group."));
            assert_eq!(text, "Lemma. Every group is nice.");
            assert!(!proof_open);
            assert_eq!(
                result,
                Ok(Tag {
                    label: Label::Statement,
                    joins: true,
                    split: false,
                })
            );
        }
        other => panic!("expected a classification, got {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn several_bypasses_kind_and_joins_and_gate_failures_stop_downstream_calls() {
    for response in [
        Ok(json!({"answers": {"independent_items": {"choice": "several"}}})),
        Ok(json!({"answers": {"independent_items": {"choice": "maybe"}}})),
        Err(Failure {
            message: "offline".into(),
            service: true,
        }),
    ] {
        let several = response
            .as_ref()
            .is_ok_and(|response| response["answers"]["independent_items"]["choice"] == "several");
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let worker = Worker::with_request(
            PathBuf::new(),
            || {},
            move |url, body| {
                assert_eq!(url, DECISIONS_URL);
                assert_eq!(
                    counted.fetch_add(1, Ordering::SeqCst),
                    0,
                    "no kind/joins or chat after this gate"
                );
                assert_eq!(body["questions"].as_object().unwrap().len(), 1);
                response.clone()
            },
        );
        worker.submit(Job::Classify {
            paragraph: 1,
            previous: None,
            text: "A paragraph.".into(),
            proof_open: false,
        });
        let Reply::Classified { result, .. } = reply(&worker) else {
            panic!("classification reply")
        };
        if several {
            assert!(result.unwrap().split);
        } else {
            assert!(result.unwrap_err().service);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn production_worker_routes_first_mixed_parity_paragraph_to_paired_lean_checks() {
    let case = &documents::ROUTING_CASES[0];
    let pieces = [
        Segment {
            id: 1,
            label: Label::Statement,
            text: "**Lemma.** For every natural number $n$, $n^2+n$ is even.".into(),
            proves: None,
            proves_previous: false,
        },
        Segment {
            id: 2,
            label: Label::Proof,
            text: "Indeed, $n^2+n=n(n+1)$ and one of $n$ and $n+1$ is even. ∎".into(),
            proves: Some(1),
            proves_previous: false,
        },
    ];
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        move |url, body| match counted.fetch_add(1, Ordering::SeqCst) {
            0 => {
                assert_eq!(url, DECISIONS_URL);
                assert!(body["questions"].get("independent_items").is_some());
                Ok(json!({"answers": {"independent_items": {"choice": "several"}}}))
            }
            1 => {
                assert_eq!(url, CHAT_URL);
                Ok(
                    json!({"choices": [{"finish_reason": "stop", "message": {"content": crate::openrouter::segments_json(&pieces).to_string()}}]}),
                )
            }
            2 => Ok(formalization_reply(
                "theorem parity (n : ℕ) : Even (n^2+n) := by sorry",
            )),
            3 => Ok(formalization_reply(
                "theorem parity (n : ℕ) : Even (n^2+n) := by\n  exact Nat.even_mul_succ_self n",
            )),
            _ => panic!("pieces must not be classified or split again"),
        },
    );
    let mut mls = Mls::new();
    mls.sync(&[Paragraph {
        text: case.text.into(),
        open: false,
    }]);
    // Each formalization is checked on its base. The worker cannot check Lean here, so the test
    // supplies the check, with the outcome that Lean gives: a statement ends in `sorry`.
    let mut checks = Vec::new();
    for _ in 0..8 {
        let Some(job) = mls.next_job() else {
            break;
        };
        let Job::Formalize {
            block,
            version,
            base,
            ..
        } = &job
        else {
            worker.submit(job);
            assert!(mls.apply(reply(&worker)));
            continue;
        };
        let (block, version, base) = (*block, *version, *base);
        checks.push((block, base, version));
        worker.submit(job);
        let Reply::Formalized {
            statement_version,
            result,
            ..
        } = reply(&worker)
        else {
            panic!("expected a formalization")
        };
        let outcome = match &result {
            Ok(formal) if formal.lean.ends_with("sorry") => Outcome::Sorry,
            _ => Outcome::Verified,
        };
        assert!(mls.apply(Reply::Formalized {
            block,
            version,
            statement_version,
            result,
            check: Some((
                base,
                Ok(Checked {
                    env: version + 100,
                    outcome,
                })
            )),
        }));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(checks.len(), 2);
    assert_eq!(
        checks[1].1,
        checks[0].2 + 100,
        "the proof checks on the environment that the statement left"
    );
    assert!(mls.next_job().is_none());
    assert_eq!(mls.dots()[0].status, Status::Verified);
    assert_eq!(
        mls.details(mls.dots()[0].key)
            .iter()
            .filter_map(|detail| detail.label)
            .collect::<Vec<_>>(),
        case.labels
    );
}

/// Real production worker requests, without requiring Lean. Requests and raw replies contain no
/// headers or key; each case stops as soon as the production plan asks to formalize/check.
#[test]
#[ignore = "calls OpenRouter: needs OPENROUTER_API_KEY; writes target/mls-live-routing artifacts"]
fn live_labeled_routing() {
    use std::{env, fs};
    let key = env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .expect("OPENROUTER_API_KEY is required");
    let dir = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/mls-live-routing"
    ))
    .join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));
    fs::create_dir_all(&dir).expect("create raw artifact directory");
    for case in &documents::ROUTING_CASES {
        let raw = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = raw.clone();
        let key = key.clone();
        let worker = Worker::with_request(
            PathBuf::new(),
            || {},
            move |url, body| {
                let result = crate::openrouter::post(url, body, Some(&key));
                captured.lock().unwrap().push(json!({"url": url, "request": body, "response": match &result {
                Ok(value) => value.clone(), Err(failure) => json!({"failure": failure.message, "service": failure.service}),
            }}));
                result
            },
        );
        let mut mls = Mls::new();
        let mut lines: Vec<Paragraph> = case
            .previous
            .into_iter()
            .map(|text| Paragraph {
                text: text.into(),
                open: false,
            })
            .collect();
        lines.push(Paragraph {
            text: case.text.into(),
            open: false,
        });
        mls.sync(&lines);
        let mut events = Vec::new();
        for _ in 0..16 {
            let Some(job) = mls.next_job() else {
                break;
            };
            events.push(format!("{job:?}"));
            if !matches!(job, Job::Classify { .. } | Job::Split { .. }) {
                break;
            }
            worker.submit(job);
            let reply = worker
                .rx
                .recv_timeout(Duration::from_secs(300))
                .expect("routing reply within five minutes");
            events.push(format!("{reply:?}"));
            assert!(mls.apply(reply));
        }
        let labels: Vec<Label> = mls
            .dots()
            .iter()
            .filter(|dot| dot.paragraph == lines.len() - 1)
            .flat_map(|dot| mls.details(dot.key))
            .filter_map(|detail| detail.label)
            .collect();
        let record = json!({"case": case.name, "events": events, "raw": *raw.lock().unwrap(), "twin": mls.twin()});
        fs::write(
            dir.join(format!("{}.json", case.name.replace(' ', "-"))),
            serde_json::to_string_pretty(&record).unwrap(),
        )
        .expect("save safe routing artifacts");
        assert_eq!(
            labels,
            case.labels,
            "{}; artifacts: {}",
            case.name,
            dir.display()
        );
    }
}

#[test]
fn split_reports_the_pieces_of_the_paragraph_as_parts_of_its_text() {
    let text = "Lemma. A. Proof. B.";
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        |url, body| {
            assert_eq!(url, CHAT_URL);
            let input: Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            assert_eq!(input["paragraph"], "Lemma. A. Proof. B.");
            let content = json!({"items": [
                {"id": 1, "role": "statement", "text": "Lemma. A.", "proves": null, "proves_previous": false},
                {"id": 2, "role": "proof", "text": "Proof.  B.", "proves": 1, "proves_previous": false}
            ]});
            Ok(json!({"choices": [{"finish_reason": "stop",
                "message": {"content": content.to_string()}}]}))
        },
    );
    worker.submit(Job::Split {
        paragraph: 4,
        text: text.to_string(),
        context: SplitContext::default(),
    });
    match reply(&worker) {
        Reply::Split {
            paragraph,
            text: echoed,
            result,
            context,
        } => {
            assert_eq!((paragraph, echoed.as_str()), (4, text));
            assert_eq!(context, SplitContext::default());
            assert_eq!(
                result,
                Ok(vec![
                    Segment {
                        id: 1,
                        label: Label::Statement,
                        text: "Lemma. A.".into(),
                        proves: None,
                        proves_previous: false
                    },
                    Segment {
                        id: 2,
                        label: Label::Proof,
                        text: "Proof. B.".into(),
                        proves: Some(1),
                        proves_previous: false
                    },
                ])
            );
        }
        other => panic!("expected a split, got {other:?}"),
    }
}

/// One item of a split reply, as the model writes it.
fn split_item(id: u64, role: &str, text: &str, proves: Option<u64>) -> Value {
    json!({"id": id, "role": role, "text": text, "proves": proves, "proves_previous": false})
}

/// A split chat completion whose content lists `items`.
fn split_answer(items: Vec<Value>) -> Result<Value, Failure> {
    let content = json!({"items": items});
    Ok(json!({"choices": [{"finish_reason": "stop",
        "message": {"content": content.to_string()}}]}))
}

fn split_job(text: &str) -> Job {
    Job::Split {
        paragraph: 4,
        text: text.to_string(),
        context: SplitContext::default(),
    }
}

/// The result of the next split reply.
fn split_result(worker: &Worker) -> Result<Vec<Segment>, Failure> {
    match reply(worker) {
        Reply::Split { result, .. } => result,
        other => panic!("expected a split, got {other:?}"),
    }
}

#[test]
fn an_invalid_segmentation_is_asked_again_with_its_reason() {
    let (worker, bodies) = worker_replying(vec![
        split_answer(vec![
            split_item(1, "statement", "Lemma. A.", None),
            split_item(2, "proof", "Proof. C.", Some(1)),
        ]),
        split_answer(vec![
            split_item(1, "statement", "Lemma. A.", None),
            split_item(2, "proof", "Proof. B.", Some(1)),
        ]),
    ]);
    worker.submit(split_job("Lemma. A. Proof. B."));
    assert_eq!(
        split_result(&worker),
        Ok(vec![
            Segment {
                id: 1,
                label: Label::Statement,
                text: "Lemma. A.".into(),
                proves: None,
                proves_previous: false,
            },
            Segment {
                id: 2,
                label: Label::Proof,
                text: "Proof. B.".into(),
                proves: Some(1),
                proves_previous: false,
            },
        ])
    );
    let requests: Vec<Value> = bodies.try_iter().collect();
    assert_eq!(requests.len(), 2);
    let first = requests[0]["messages"][1]["content"].as_str().unwrap();
    assert!(!first.contains("rejected_answer_reason"), "{first}");
    let second = requests[1]["messages"][1]["content"].as_str().unwrap();
    assert!(second.contains("rejected_answer_reason"), "{second}");
    assert!(second.contains(NOT_SPLIT), "{second}");
}

#[test]
fn a_segmentation_invalid_twice_is_a_structural_failure() {
    let invalid = || {
        split_answer(vec![
            split_item(1, "statement", "Lemma. A.", None),
            split_item(2, "proof", "Proof. C.", Some(1)),
        ])
    };
    let (worker, bodies) = worker_replying(vec![invalid(), invalid()]);
    worker.submit(split_job("Lemma. A. Proof. B."));
    assert_eq!(
        split_result(&worker),
        Err(Failure {
            message: NOT_SPLIT.to_string(),
            service: false,
        })
    );
    assert_eq!(bodies.try_iter().count(), 2);
}

#[test]
fn a_service_failure_on_a_split_is_not_asked_again() {
    let (worker, bodies) = worker_replying(vec![Err(unreachable())]);
    worker.submit(split_job("Lemma. A. Proof. B."));
    assert_eq!(split_result(&worker), Err(unreachable()));
    assert_eq!(bodies.try_iter().count(), 1);
}

#[test]
fn formalize_rejects_an_axiom_without_a_service_failure() {
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        |_, _| Ok(formalization_reply("axiom answer : ℕ")),
    );
    worker.submit(formalize(Label::Definition, "Assume an answer exists."));
    match reply(&worker) {
        Reply::Formalized {
            block,
            version,
            statement_version,
            result,
            check,
        } => {
            assert_eq!((block, version, statement_version), (3, 9, None));
            assert_eq!(check, None);
            assert_eq!(
                result,
                Err(Failure {
                    message: "The Lean declares an axiom.".to_string(),
                    service: false,
                })
            );
        }
        other => panic!("expected a formalization, got {other:?}"),
    }
}

#[test]
fn formalize_passes_an_accepted_formalization_through() {
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        |_, _| Ok(formalization_reply("def answer : ℕ := 42")),
    );
    worker.submit(formalize(Label::Definition, "Let the answer be 42."));
    match reply(&worker) {
        Reply::Formalized { result, .. } => assert_eq!(
            result,
            Ok(Formal {
                lean: "def answer : ℕ := 42".to_string(),
                lean_name: None,
                assumptions: vec!["A plain sentence.".to_string()],
                uses: Vec::new(),
            })
        ),
        other => panic!("expected a formalization, got {other:?}"),
    }
}

#[test]
fn formalize_records_the_names_above_that_the_lean_uses() {
    let worker = Worker::with_request(
        PathBuf::new(),
        || {},
        |_, _| Ok(formalization_reply("def f := d1")),
    );
    worker.submit(Job::Formalize {
        block: 3,
        version: 9,
        label: Label::Definition,
        text: "Let f be d1.".to_string(),
        above: None,
        lean_above: Vec::new(),
        names_above: vec!["d1".into(), "t2".into()],
        statement: None,
        base: 0,
    });
    match reply(&worker) {
        Reply::Formalized { result, .. } => {
            assert_eq!(result.map(|formal| formal.uses), Ok(vec!["d1".to_string()]));
        }
        other => panic!("expected a formalization, got {other:?}"),
    }
}

#[test]
fn a_request_failure_is_passed_through_as_a_service_failure() {
    let unreachable = Failure {
        message: "OpenRouter is unreachable: no route".to_string(),
        service: true,
    };
    let failure = unreachable.clone();
    let worker = Worker::with_request(PathBuf::new(), || {}, move |_, _| Err(failure.clone()));
    worker.submit(Job::Classify {
        paragraph: 1,
        previous: None,
        text: "Anything.".to_string(),
        proof_open: false,
    });
    match reply(&worker) {
        Reply::Classified { result, .. } => assert_eq!(result, Err(unreachable)),
        other => panic!("expected a classification, got {other:?}"),
    }
}

#[test]
fn the_key_is_never_shown_by_debug() {
    let worker = Worker::with_request(PathBuf::new(), || {}, no_request);
    worker.set_key(Some("sk-or-secret".into()));
    assert!(!format!("{worker:?}").contains("sk-or-secret"));
}

#[test]
fn check_without_a_lean_project_is_a_service_failure() {
    let worker = Worker::with_request(
        PathBuf::from("/nonexistent/lvtex-mls-project"),
        || {},
        no_request,
    );
    worker.submit(Job::Check {
        block: 1,
        version: 1,
        lean: "def answer : ℕ := 42".to_string(),
        base: 0,
    });
    match reply(&worker) {
        Reply::Checked {
            block: 1,
            version: 1,
            base: 0,
            result: Err(failure),
        } => {
            assert!(failure.service, "{}", failure.message);
            assert!(
                failure.message.starts_with("No Lean project"),
                "{}",
                failure.message
            );
        }
        other => panic!("expected a failed check, got {other:?}"),
    }
}

#[test]
fn replies_come_back_in_submission_order_with_one_wake_each() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let counted = wakes.clone();
    let worker = Worker::with_request(
        PathBuf::new(),
        move || {
            counted.fetch_add(1, Ordering::SeqCst);
        },
        |_, _| Ok(jev(Label::Skip, false)),
    );
    for paragraph in 1..=3u64 {
        worker.submit(Job::Classify {
            paragraph,
            previous: None,
            text: format!("Paragraph {paragraph}."),
            proof_open: false,
        });
    }
    let order: Vec<u64> = (0..3)
        .map(|_| match reply(&worker) {
            Reply::Classified { paragraph, .. } => paragraph,
            other => panic!("expected a classification, got {other:?}"),
        })
        .collect();
    assert_eq!(order, [1, 2, 3]);
    // The wake follows each send, so it may land a moment after the reply is received.
    let deadline = Instant::now() + TIMEOUT;
    while wakes.load(Ordering::SeqCst) < 3 {
        assert!(Instant::now() < deadline, "wake was not called three times");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(wakes.load(Ordering::SeqCst), 3);
}

/// The lean name of a formalization: the word after its declaration keyword.
fn lean_name(lean: &str) -> Option<&str> {
    lean.split_whitespace().nth(1)
}

fn line_with_text(lines: &'static [Line], text: &str) -> Result<&'static Line, Failure> {
    lines
        .iter()
        .find(|line| line.text == text)
        .ok_or_else(|| Failure {
            message: format!("the scripted model has no paragraph {text:?}"),
            service: true,
        })
}

/// The scripted model for one document: Jev's label for each paragraph, and the formalizer's Lean for the
/// block whose first paragraph is in the prompt.
fn scripted(
    lines: &'static [Line],
) -> impl FnMut(&str, &Value) -> Result<Value, Failure> + Send + 'static {
    move |url, body| {
        if url == DECISIONS_URL {
            let text = body["state"]["paragraph"].as_str().unwrap_or_default();
            let line = line_with_text(lines, text)?;
            Ok(jev(line.label, line.joins))
        } else if url == CHAT_URL {
            let prompt = body["messages"][1]["content"]
                .as_array()
                .and_then(|parts| parts.last())
                .and_then(|part| part["text"].as_str())
                .unwrap_or_default();
            let block = prompt.split_once("Block:\n").map_or("", |(_, block)| block);
            let first = block.split("\n\n").next().unwrap_or_default();
            let line = line_with_text(lines, first)?;
            let lean = line.lean.ok_or_else(|| Failure {
                message: format!("the scripted model has no Lean for {first:?}"),
                service: true,
            })?;
            let content = json!({
                "lean": lean,
                "lean_name": lean_name(lean),
                "assumptions": line.assumptions,
            });
            Ok(json!({"choices": [{"finish_reason": "stop",
                "message": {"content": content.to_string()}}]}))
        } else {
            no_request(url, body)
        }
    }
}

#[test]
#[ignore = "needs the Lean project in lean/ built (lake build Mathlib repl)"]
fn scripted_documents_end_with_their_dots() {
    let project = lean_project();
    if !lean_ready(&project) {
        eprintln!(
            "skipping: the Lean REPL is not built in {} (run `lake build Mathlib repl` there)",
            project.display()
        );
        return;
    }
    for document in &DOCUMENTS {
        let worker = Worker::with_request(project.clone(), || {}, scripted(document.lines));
        let mut mls = Mls::new();
        let lines: Vec<&str> = document.lines.iter().map(|line| line.text).collect();
        drive(&mut mls, &worker, &lines)
            .unwrap_or_else(|error| panic!("{}: {error}", document.name));
        assert_outcome(&mls, document);
        if document.name == "group theory" {
            let chart = mls.chart();
            let shape: Vec<(&str, usize)> = chart
                .iter()
                .map(|node| (node.name.as_str(), node.level))
                .collect();
            assert_eq!(
                shape,
                [
                    ("IsInvolution", 0),
                    ("left_identity_unique", 0),
                    ("one_is_involution", 1),
                ],
                "{chart:?}"
            );
            assert_eq!(chart[2].uses, [chart[0].key], "{chart:?}");
        }
    }
}

#[test]
#[ignore = "needs the Lean project in lean/ built (lake build Mathlib repl)"]
fn the_regression_fails_at_the_redundant_simp_and_the_corrected_proof_checks_on_the_same_base() {
    let project = lean_project();
    if !lean_ready(&project) {
        eprintln!(
            "skipping: the Lean REPL is not built in {}",
            project.display()
        );
        return;
    }
    // The definitions above the proof, checked on the base the statement's proof is checked on.
    let definitions = DEFINITIONS.join("\n\n");
    let mut repl = None;
    let base = super::check(&mut repl, &project, &definitions, 0).expect("the definitions check");
    assert_eq!(base.outcome, Outcome::Verified, "{definitions}");

    let failed = super::check(&mut repl, &project, FAILED, base.env).expect("the candidate checks");
    match failed.outcome {
        Outcome::Failed(message) => {
            assert!(message.contains(ERROR), "{message}")
        }
        other => panic!("the candidate should fail, got {other:?}"),
    }
    let corrected =
        super::check(&mut repl, &project, CORRECTED, base.env).expect("the corrected proof checks");
    assert_eq!(corrected.outcome, Outcome::Verified);
}

/// The statement of the refutation tests, and its name.
const POWER_NAME: &str = "powerSum_two_eq_zero";
const POWER_STATEMENT: &str = "theorem powerSum_two_eq_zero (p : ℕ) [Fact p.Prime] (hodd : Odd p) : powerSum p 2 = 0 := by sorry";
const POWER_PROSE: &str = "The squares of the residues mod p sum to zero for every odd prime p.";
/// The refutation of `POWER_STATEMENT` at p = 3, with the counterexample the formalizer gives.
const REFUTATION: &str = "-- Counterexample: p = 3, where p - 1 = 2 divides 2, so S_2 = -1.\nexample : ¬ type_of% @powerSum_two_eq_zero := by\n  intro h\n  have := @h 3 ⟨by norm_num⟩ ⟨1, rfl⟩\n  revert this\n  decide";
const FIRST_ERROR: &str = "1:2: `simp` made no progress";
const SECOND_ERROR: &str = "1:2: unsolved goals";
const THIRD_ERROR: &str = "1:2: `decide` failed to reduce";
const REFUTATION_ERROR: &str = "2:2: `decide` failed to reduce";

/// The proof of `POWER_STATEMENT` with the tactic `tactic`.
fn power_proof(tactic: &str) -> String {
    format!(
        "theorem powerSum_two_eq_zero (p : ℕ) [Fact p.Prime] (hodd : Odd p) : powerSum p 2 = 0 := by\n  {tactic}"
    )
}

/// A formalization whose content is `lean`, as a reply to one request.
fn lean_reply(lean: &str) -> Result<Value, Failure> {
    Ok(formalization_reply(lean))
}

/// The request for each ask about the proof of `POWER_STATEMENT`, as `run` builds it.
fn power_ask(ask: Ask<'_>) -> Value {
    match ask {
        Ask::Formalize(retry) => formalize_request(
            Label::Proof,
            POWER_PROSE,
            None,
            &[],
            Some(POWER_STATEMENT),
            retry,
        ),
        Ask::Refute { failed, reason } => {
            refute_request(POWER_PROSE, None, &[], POWER_STATEMENT, failed, reason)
        }
    }
}

/// What one formalization of the proof of `POWER_STATEMENT` sent, checked and returned.
struct PowerRun {
    requests: Vec<Value>,
    checks: Vec<(String, u64)>,
    result: Result<Formal, Failure>,
    check: Option<(u64, Result<Checked, Failure>)>,
}

/// Formalizes the proof of `POWER_STATEMENT` on base 7. Each request gets the next of `replies`, and
/// each check the next of `outcomes`; a request past the replies fails as an unreachable service
/// would.
fn power_run(replies: Vec<Result<Value, Failure>>, outcomes: Vec<Outcome>) -> PowerRun {
    let mut requests = Vec::new();
    let mut replies = replies.into_iter();
    let mut request = |_: &str, body: &Value| -> Result<Value, Failure> {
        requests.push(body.clone());
        replies.next().unwrap_or_else(|| Err(unreachable()))
    };
    let mut checks = Vec::new();
    let mut outcomes = outcomes.into_iter();
    let mut lean_check = |lean: &str, base: u64| -> Result<Checked, Failure> {
        checks.push((lean.to_string(), base));
        Ok(Checked {
            env: 5,
            outcome: outcomes.next().expect("an outcome for each check"),
        })
    };
    let names_above = vec![POWER_NAME.to_string()];
    let (result, check) = super::formalize(
        &mut request,
        &mut lean_check,
        power_ask,
        Label::Proof,
        &names_above,
        Some(POWER_STATEMENT),
        7,
    );
    PowerRun {
        requests,
        checks,
        result,
        check,
    }
}

/// Three proofs that Lean rejects, each with its error, as the first three attempts.
fn three_rejected_proofs() -> (Vec<Result<Value, Failure>>, Vec<Outcome>) {
    (
        vec![
            lean_reply(&power_proof("simp [powerSum]")),
            lean_reply(&power_proof("simp [powerSum, Finset.sum_comm]")),
            lean_reply(&power_proof("decide")),
        ],
        vec![
            Outcome::Failed(FIRST_ERROR.to_string()),
            Outcome::Failed(SECOND_ERROR.to_string()),
            Outcome::Failed(THIRD_ERROR.to_string()),
        ],
    )
}

#[test]
fn a_refutation_is_asked_once_after_the_last_proof_attempt_and_its_verified_check_is_the_result() {
    let (mut replies, mut outcomes) = three_rejected_proofs();
    replies.push(lean_reply(REFUTATION));
    outcomes.push(Outcome::Verified);
    let run = power_run(replies, outcomes);

    assert_eq!(run.requests.len(), 4);
    let asked = prompt(&run.requests[3]);
    assert!(
        asked.contains("example : ¬ type_of% @powerSum_two_eq_zero :="),
        "{asked}"
    );
    assert!(asked.contains(&power_proof("decide")), "{asked}");
    assert!(asked.contains(THIRD_ERROR), "{asked}");
    assert_eq!(run.checks.len(), 4);
    assert_eq!(run.checks[3], (REFUTATION.to_string(), 7));
    let formal = run.result.expect("the refutation is the result");
    assert_eq!(formal.lean, REFUTATION);
    assert_eq!(
        formal.uses,
        Vec::<String>::new(),
        "the statement's own name is not a use"
    );
    assert_eq!(
        run.check,
        Some((
            7,
            Ok(Checked {
                env: 5,
                outcome: Outcome::Verified,
            })
        ))
    );
}

#[test]
fn an_empty_or_unanswered_refutation_leaves_the_last_proof_attempt_as_the_result() {
    for refusal in [lean_reply(""), Err(unreachable())] {
        let (mut replies, outcomes) = three_rejected_proofs();
        replies.push(refusal);
        let run = power_run(replies, outcomes);

        assert_eq!(run.requests.len(), 4);
        assert_eq!(run.checks.len(), 3);
        assert_eq!(
            run.result.map(|formal| formal.lean),
            Ok(power_proof("decide"))
        );
        assert_eq!(
            run.check,
            Some((
                7,
                Ok(Checked {
                    env: 5,
                    outcome: Outcome::Failed(THIRD_ERROR.to_string()),
                })
            ))
        );
    }
}

#[test]
fn a_refutation_the_rules_reject_leaves_the_last_proof_attempt_as_the_result() {
    let without_comment = REFUTATION.replacen(
        "-- Counterexample: p = 3, where p - 1 = 2 divides 2, so S_2 = -1.\n",
        "",
        1,
    );
    for refutation in [REFUTATION.replace("decide", "sorry"), without_comment] {
        let (mut replies, outcomes) = three_rejected_proofs();
        replies.push(lean_reply(&refutation));
        let run = power_run(replies, outcomes);

        assert_eq!(run.requests.len(), 4, "{refutation}");
        assert_eq!(run.checks.len(), 3, "{refutation}");
        assert_eq!(
            run.result.map(|formal| formal.lean),
            Ok(power_proof("decide")),
            "{refutation}"
        );
    }
}

#[test]
fn a_refutation_that_lean_does_not_verify_leaves_the_last_proof_attempt_as_the_result() {
    let (mut replies, mut outcomes) = three_rejected_proofs();
    replies.push(lean_reply(REFUTATION));
    outcomes.push(Outcome::Failed(REFUTATION_ERROR.to_string()));
    let run = power_run(replies, outcomes);

    assert_eq!(run.checks.len(), 4);
    assert_eq!(run.checks[3], (REFUTATION.to_string(), 7));
    assert_eq!(
        run.result.map(|formal| formal.lean),
        Ok(power_proof("decide"))
    );
    assert_eq!(
        run.check,
        Some((
            7,
            Ok(Checked {
                env: 5,
                outcome: Outcome::Failed(THIRD_ERROR.to_string()),
            })
        ))
    );
}

#[test]
fn a_proof_gets_no_refutation_request_when_it_is_accepted_or_its_service_fails_or_a_statement_fails()
 {
    let accepted = power_run(
        vec![lean_reply(&power_proof("simp [powerSum]"))],
        vec![Outcome::Verified],
    );
    assert_eq!(accepted.requests.len(), 1);

    let unanswered = power_run(vec![Err(unreachable())], vec![]);
    assert_eq!(unanswered.requests.len(), 1);
    assert_eq!(unanswered.result, Err(unreachable()));

    let mut statement_requests = 0;
    let mut request = |_: &str, _: &Value| -> Result<Value, Failure> {
        statement_requests += 1;
        Ok(formalization_reply("axiom t : True"))
    };
    let mut check = |_: &str, _: u64| -> Result<Checked, Failure> {
        panic!("a rejected statement is not checked")
    };
    let (result, _) = super::formalize(
        &mut request,
        &mut check,
        |ask| match ask {
            Ask::Formalize(retry) => {
                formalize_request(Label::Statement, "Then t holds.", None, &[], None, retry)
            }
            Ask::Refute { .. } => panic!("a statement is never refuted"),
        },
        Label::Statement,
        &[],
        None,
        7,
    );
    assert_eq!(statement_requests, super::ATTEMPTS);
    assert_eq!(
        result,
        Err(Failure {
            message: "The Lean declares an axiom.".to_string(),
            service: false,
        })
    );
}

#[test]
#[ignore = "needs the Lean project in lean/ built (lake build Mathlib repl)"]
fn the_refutation_of_a_statement_checks_after_it_and_passes_the_rules() {
    let project = lean_project();
    if !lean_ready(&project) {
        eprintln!(
            "skipping: the Lean REPL is not built in {}",
            project.display()
        );
        return;
    }
    let definition = "def powerSum (p : ℕ) [Fact p.Prime] (k : ℕ) : ZMod p := ∑ x : ZMod p, x ^ k";
    let mut repl = None;
    let base = super::check(&mut repl, &project, definition, 0).expect("the definition checks");
    assert_eq!(base.outcome, Outcome::Verified, "{definition}");

    let statement =
        super::check(&mut repl, &project, POWER_STATEMENT, base.env).expect("the statement checks");
    assert_eq!(statement.outcome, Outcome::Sorry, "{POWER_STATEMENT}");

    let refutation = super::check(&mut repl, &project, REFUTATION, statement.env)
        .expect("the refutation checks");
    assert_eq!(refutation.outcome, Outcome::Verified, "{REFUTATION}");
    assert_eq!(
        rules::reject_refutation(REFUTATION, POWER_STATEMENT),
        Ok(())
    );
}
