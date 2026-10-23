use super::*;
use crate::{Checked, Failure, Formal, Job, Label, Outcome, Reply, Status, Tag};

const MISSING_KEY: &str = "No OpenRouter key";
const REPL_DOWN: &str = "the Lean REPL is not running";

/// The mock model's tags. "Def" is a definition, "Lemma" and "Then" a statement, "Proof" starts a
/// proof, a lowercase line continues an open proof, and anything else is prose. A "Then" line
/// and a "+" line continue the paragraph above, and a line with " / " in it holds several items.
fn tag_for(_previous: Option<&str>, text: &str, proof_open: bool) -> Tag {
    let label = if text.starts_with("Def") {
        Label::Definition
    } else if text.starts_with("Lemma") || text.starts_with("Then") {
        Label::Statement
    } else if text.starts_with("Proof") || (proof_open && text.starts_with(char::is_lowercase)) {
        Label::Proof
    } else {
        Label::Skip
    };
    Tag {
        label,
        joins: text.starts_with("Then") || text.starts_with('+'),
        split: text.contains(" / "),
    }
}

/// The mock worker. A split cuts after each " / ", which stays at the end of its piece so that the
/// pieces reproduce the paragraph, and text containing UNSPLIT cannot be cut. Lean is
/// deterministic: a statement ends in `sorry`, a proof replaces that
/// with `trivial`, and text containing FALSE produces Lean that the REPL rejects. A repair of a
/// proof whose text also contains FIXABLE produces Lean that checks, so the first repair is green,
/// and any other repair stays FALSE. A check's environment is 100 plus the block version, so each
/// check has a predictable environment.
fn answer(job: &Job) -> Reply {
    match job {
        Job::Classify {
            paragraph,
            previous,
            text,
            proof_open,
        } => Reply::Classified {
            paragraph: *paragraph,
            previous: previous.clone(),
            text: text.clone(),
            proof_open: *proof_open,
            result: Ok(tag_for(previous.as_deref(), text, *proof_open)),
        },
        Job::Split {
            paragraph,
            text,
            context,
        } => Reply::Split {
            paragraph: *paragraph,
            text: text.clone(),
            context: context.clone(),
            result: if text.contains("UNSPLIT") {
                Err(Failure {
                    message: "The paragraph could not be split into its items.".to_string(),
                    service: false,
                })
            } else {
                let mut statement = None;
                Ok(text
                    .split_inclusive(" / ")
                    .enumerate()
                    .map(|(index, piece)| {
                        let label = tag_for(None, piece.trim(), false).label;
                        let id = index as u64 + 1;
                        let proves = if label == Label::Proof {
                            statement
                        } else {
                            None
                        };
                        if label == Label::Statement {
                            statement = Some(id);
                        }
                        Segment {
                            id,
                            label,
                            text: piece.trim().to_string(),
                            proves,
                            proves_previous: label == Label::Proof
                                && proves.is_none()
                                && context.statement.is_some(),
                        }
                    })
                    .collect())
            },
        },
        Job::Formalize {
            block,
            version,
            label,
            text,
            statement,
            names_above,
            ..
        } => {
            let mut lean = match label {
                Label::Definition => format!("def d{block} := 1"),
                Label::Statement => format!("theorem t{block} : True := by sorry"),
                Label::Proof => statement
                    .as_ref()
                    .expect("a proof is formalized against its statement")
                    .1
                    .replace("sorry", "trivial"),
                Label::Skip => String::new(),
            };
            if text.contains("FALSE") {
                lean.push_str("\n-- FALSE");
            }
            // A block that only fixes notation declares nothing, so its Lean is blank.
            if text.contains("NOTATION") {
                lean.clear();
            }
            // A block whose text contains USES mentions every name above, as Lean that relies on
            // them would. `uses` then finds the ones it does not declare itself. A statement must end
            // in its `sorry` (see `rules::reject`), so its example goes above it.
            if text.contains("USES") {
                let example = format!("example := [{}]", names_above.join(", "));
                if *label == Label::Statement {
                    lean = format!("{example}\n{lean}");
                } else {
                    lean.push_str(&format!("\n{example}"));
                }
            }
            let uses = crate::rules::uses(&lean, names_above);
            Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: statement.as_ref().map(|(version, _)| *version),
                result: Ok(Formal {
                    lean,
                    lean_name: None,
                    assumptions: Vec::new(),
                    uses,
                }),
            }
        }
        Job::Repair {
            text,
            statement,
            names_above,
            ..
        } => {
            let mut lean = statement.1.replace("sorry", "trivial");
            if text.contains("USES") {
                lean.push_str(&format!("\nexample := [{}]", names_above.join(", ")));
            }
            if text.contains("FALSE") && !text.contains("FIXABLE") {
                lean.push_str("\n-- FALSE");
            }
            let uses = crate::rules::uses(&lean, names_above);
            Reply::Repaired {
                job: job.clone(),
                result: Ok(Formal {
                    lean,
                    lean_name: None,
                    assumptions: Vec::new(),
                    uses,
                }),
            }
        }
        Job::Check {
            block,
            version,
            lean,
            base,
        } => {
            let outcome = if lean.contains("FALSE") {
                Outcome::Failed("1:0: error".to_string())
            } else if lean.contains("sorry") {
                Outcome::Sorry
            } else {
                Outcome::Verified
            };
            Reply::Checked {
                block: *block,
                version: *version,
                base: *base,
                result: Ok(Checked {
                    env: 100 + *version,
                    outcome,
                }),
            }
        }
    }
}

fn paras(lines: &[&str]) -> Vec<Paragraph> {
    lines
        .iter()
        .map(|text| Paragraph {
            text: text.to_string(),
            open: false,
        })
        .collect()
}

/// `paras` with the paragraph at `open` still being typed.
fn paras_open(lines: &[&str], open: usize) -> Vec<Paragraph> {
    let mut out = paras(lines);
    out[open].open = true;
    out
}

/// Answers every job until none is left, and returns the jobs in order.
fn run(mls: &mut Mls, mut respond: impl FnMut(&Job) -> Reply) -> Vec<Job> {
    let mut jobs = Vec::new();
    while let Some(job) = mls.next_job() {
        let reply = respond(&job);
        assert!(mls.apply(reply), "a reply to a job in flight is accepted");
        jobs.push(job);
    }
    jobs
}

/// Answers jobs until one matches `stop`, and returns that job unanswered, so a test can
/// reply to it after the document has changed.
fn run_until(mls: &mut Mls, stop: impl Fn(&Job) -> bool) -> Job {
    loop {
        let job = mls
            .next_job()
            .expect("the job the test waits for is produced");
        if stop(&job) {
            return job;
        }
        assert!(mls.apply(answer(&job)));
    }
}

/// The texts of the formalize jobs, in order.
fn formalize_texts(jobs: &[Job]) -> Vec<&str> {
    jobs.iter()
        .filter_map(|job| match job {
            Job::Formalize { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn kinds(jobs: &[Job]) -> Vec<&'static str> {
    jobs.iter()
        .map(|job| match job {
            Job::Classify { .. } => "classify",
            Job::Split { .. } => "split",
            Job::Formalize { .. } => "formalize",
            Job::Check { .. } => "check",
            Job::Repair { .. } => "repair",
        })
        .collect()
}

fn statuses(mls: &Mls) -> Vec<(usize, Status)> {
    mls.dots()
        .iter()
        .map(|dot| (dot.paragraph, dot.status))
        .collect()
}

/// The details of the dot with `key`, which must hold one item.
fn item(mls: &Mls, key: u64) -> Details {
    let mut items = mls.details(key);
    assert_eq!(items.len(), 1, "the dot holds one item");
    items.remove(0)
}

fn id_of(mls: &Mls, index: usize) -> u64 {
    mls.paras[index].id
}

#[test]
fn prose_gets_only_classify_jobs_and_no_dots() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello there.", "Another line."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify", "classify"]);
    assert!(mls.dots().is_empty());
}

/// A proof pairs only with the statement directly above it, so this covers the adjacent case: the
/// proof is checked on the environment that its statement left, and the twin keeps the proof in
/// theorem form, with no example, because it replaces the statement's Lean.
#[test]
fn a_proof_after_another_block_is_checked_after_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Trivially. ∎"]));
    let jobs = run(&mut mls, answer);
    let (statement, proof) = (id_of(&mls, 0), id_of(&mls, 1));
    let Job::Check { base, .. } = &jobs[5] else {
        panic!("the proof is checked")
    };
    assert_eq!(*base, 100 + version_of(&mls, statement));
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
    let file = mls.twin();
    assert!(
        file.contains(&format!(
            "\n-- lvtex:block {proof} proof\ntheorem t{statement} : True := by trivial\n"
        )),
        "{file}"
    );
    assert!(!file.contains("example"), "{file}");
}

#[test]
fn a_statement_then_its_proof_is_formalized_and_checked_in_order() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Trivially. ∎"]));
    let jobs = run(&mut mls, answer);
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "classify",
            "formalize",
            "check",
            "formalize",
            "check"
        ]
    );
    let (statement, proof) = (id_of(&mls, 0), id_of(&mls, 1));

    let Job::Formalize { block, .. } = &jobs[2] else {
        panic!("the statement is formalized")
    };
    assert_eq!(*block, statement);
    let Job::Check { base, .. } = &jobs[3] else {
        panic!("the statement is checked")
    };
    assert_eq!(*base, 0);

    let Job::Formalize {
        block,
        statement: above,
        lean_above,
        names_above,
        ..
    } = &jobs[4]
    else {
        panic!("the proof is formalized")
    };
    assert_eq!(*block, proof);
    assert!(above.is_some());
    assert_eq!(
        lean_above,
        &vec![(
            statement,
            format!("theorem t{statement} : True := by sorry")
        )]
    );
    assert_eq!(names_above, &vec![format!("t{statement}")]);

    let Job::Check {
        block, base, lean, ..
    } = &jobs[5]
    else {
        panic!("the proof is checked")
    };
    assert_eq!(*block, proof);
    assert_eq!(
        *base,
        100 + version_of(&mls, statement),
        "a proof is checked after its statement, on the environment that statement left"
    );
    assert!(lean.starts_with("example"), "{lean}");

    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
    let file = mls.twin();
    assert!(
        file.contains(&format!(
            "\n-- lvtex:block {proof} proof\ntheorem t{statement} : True := by trivial\n"
        )),
        "{file}"
    );
    let lean = file
        .split(MEMORY_OPEN)
        .next()
        .expect("the Lean comes first");
    assert!(!lean.contains("by sorry"), "{file}");
}

#[test]
fn a_lone_statement_is_sorry_and_shows_checking_until_each_reply() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t."]));

    let classify = mls.next_job().expect("the paragraph is classified");
    assert!(mls.apply(answer(&classify)));
    assert_eq!(statuses(&mls), [(0, Status::Checking)]);

    let formalize = mls.next_job().expect("the statement is formalized");
    assert!(matches!(formalize, Job::Formalize { .. }));
    assert!(mls.apply(answer(&formalize)));
    assert_eq!(statuses(&mls), [(0, Status::Checking)]);

    let check = mls.next_job().expect("the statement is checked");
    assert!(mls.apply(answer(&check)));
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);
    assert!(mls.next_job().is_none());
}

#[test]
fn a_false_proof_fails_with_the_lean_error_and_the_statement_stays_sorry() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎"]));
    run(&mut mls, answer);
    let proof = id_of(&mls, 1);

    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
    let details = item(&mls, proof);
    assert_eq!(details.error.as_deref(), Some("1:0: error"));
    assert!(!details.service);
}

#[test]
fn a_proof_continues_over_lowercase_paragraphs_until_its_end_mark() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Start.", "continue. ∎"]));
    let jobs = run(&mut mls, answer);
    let proof = id_of(&mls, 1);

    assert!(jobs.iter().any(|job| matches!(
        job,
        Job::Classify { paragraph, proof_open: true, .. } if *paragraph == id_of(&mls, 2)
    )));
    assert!(jobs.iter().any(|job| matches!(
        job,
        Job::Formalize { block, text, .. }
            if *block == proof && text == "Proof. Start.\n\ncontinue. ∎"
    )));
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn a_proof_without_an_end_mark_at_the_end_is_checked() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Never ends."]));
    run(&mut mls, answer);

    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn a_proof_ending_in_a_colon_at_the_end_waits_for_its_next_paragraph() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. We compute:"]));
    let jobs = run(&mut mls, answer);
    let proof = mls.blocks[1].id;

    assert!(jobs.iter().all(|job| !matches!(
        job,
        Job::Formalize { block, .. } | Job::Check { block, .. } if *block == proof
    )));
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);
}

/// The shape of a document written without end marks: each proof ends where the next statement
/// starts, and the last one is checked too.
#[test]
fn proofs_without_end_marks_each_get_a_dot() {
    let mut mls = Mls::new();
    mls.sync(&paras(&[
        "Lemma a.",
        "Proof. Argue.",
        "Lemma b.",
        "Proof. Argue again.",
    ]));
    run(&mut mls, answer);

    assert_eq!(
        statuses(&mls),
        [
            (0, Status::Verified),
            (1, Status::Verified),
            (2, Status::Verified),
            (3, Status::Verified),
        ]
    );
}

#[test]
fn a_definition_that_declares_nothing_gets_no_dot_and_the_blocks_below_go_on() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def NOTATION.", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    let notation = id_of(&mls, 0);

    assert!(jobs.iter().all(|job| !matches!(
        job,
        Job::Check { block, .. } if *block == notation
    )));
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);
}

#[test]
fn a_proof_with_no_statement_above_fails_without_a_formalize() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Proof. Orphan. ∎"]));
    let jobs = run(&mut mls, answer);
    let proof = mls.blocks[1].id;

    assert!(jobs.iter().all(|job| !matches!(
        job,
        Job::Formalize { block, .. } | Job::Check { block, .. } if *block == proof
    )));
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Failed)]);
    let details = item(&mls, proof);
    assert_eq!(
        details.error.as_deref(),
        Some("There is no statement directly above this proof.")
    );
}

#[test]
fn a_reference_reads_the_kind_and_label_a_paragraph_starts_with() {
    let cases = [
        ("**Theorem 2.1.** Let", Some((false, "theorem", "2.1"))),
        (
            "*Proof of Theorem 2.1.* Let",
            Some((true, "theorem", "2.1")),
        ),
        ("Proof of the Lemma A.", Some((true, "lemma", "a"))),
        ("**Lemma 3 (monotonicity).**", Some((false, "lemma", "3"))),
        ("**Theorem (Banach).**", None),
        ("Proof. Let", None),
        ("*Proof.* By Theorem 2.1", None),
        ("Corollary 4.", Some((false, "corollary", "4"))),
        ("**Claim.**", None),
    ];
    for (text, expected) in cases {
        let got = reference(text);
        let got = got
            .as_ref()
            .map(|(proof, kind, label)| (*proof, kind.as_str(), label.as_str()));
        assert_eq!(got, expected, "{text}");
    }
}

/// A proof that names its statement pairs with that statement wherever it is above, not only with
/// the block directly above it.
#[test]
fn a_proof_of_a_named_lemma_proves_it_from_further_down() {
    let mut mls = Mls::new();
    mls.sync(&paras(&[
        "Lemma 1. one",
        "Lemma 2. two",
        "Proof of Lemma 1. done ∎",
    ]));
    let jobs = run(&mut mls, answer);

    assert!(jobs.iter().all(|job| !matches!(job, Job::Split { .. })));
    assert_eq!(mls.blocks[2].statement, Some(id_of(&mls, 0)));
    assert_eq!(
        statuses(&mls),
        [
            (0, Status::Verified),
            (1, Status::Sorry),
            (2, Status::Verified)
        ]
    );
}

#[test]
fn a_proof_naming_a_missing_statement_gets_the_recovery() {
    let mut mls = Mls::new();
    mls.sync(&paras(&[
        "Lemma 1. one",
        "Def d.",
        "Proof of Lemma 7. done ∎",
    ]));
    let jobs = run(&mut mls, answer);
    let proof = id_of(&mls, 2);

    assert!(jobs.iter().any(|job| matches!(
        job,
        Job::Split { paragraph, .. } if *paragraph == proof
    )));
}

#[test]
fn an_unnamed_proof_still_proves_the_statement_directly_above() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma 1. one", "Lemma 2. two", "Proof. done ∎"]));
    run(&mut mls, answer);

    assert_eq!(mls.blocks[2].statement, Some(id_of(&mls, 1)));
}

#[test]
fn a_block_gets_the_whole_document_above_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Just words.", "Def d.", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    let above: Vec<Option<String>> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Formalize { above, .. } => Some(above.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        above,
        [
            Some("Just words.".to_string()),
            Some("Just words.\n\nDef d.".to_string()),
        ]
    );
}

#[test]
fn a_heading_is_part_of_the_document_above() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["# Title", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    let above = jobs.iter().find_map(|job| match job {
        Job::Formalize { above, .. } => Some(above.clone()),
        _ => None,
    });
    assert_eq!(above, Some(Some("# Title".to_string())));
}

#[test]
fn editing_a_middle_statement_rechecks_the_tail_without_formalizing_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a.", "Lemma b."]));
    run(&mut mls, answer);
    let (a, b) = (id_of(&mls, 1), id_of(&mls, 2));

    mls.sync(&paras(&["Def d.", "Lemma a2.", "Lemma b."]));
    let jobs = run(&mut mls, answer);

    // The statement below was tagged against the old text above it, so it is tagged again.
    assert_eq!(
        kinds(&jobs),
        ["classify", "classify", "formalize", "check", "check"]
    );
    assert!(matches!(
        &jobs[0],
        Job::Classify { paragraph, text, .. } if *paragraph == a && text == "Lemma a2."
    ));
    assert!(matches!(
        &jobs[1],
        Job::Classify { text, previous, .. } if text == "Lemma b." && previous.as_deref() == Some("Lemma a2.")
    ));
    let Job::Check {
        block: checked_a,
        version: a_version,
        ..
    } = &jobs[3]
    else {
        panic!("the edited statement is checked")
    };
    assert_eq!(*checked_a, a);
    let Job::Check { block, base, .. } = &jobs[4] else {
        panic!("the statement below is checked")
    };
    assert_eq!(*block, b);
    assert_eq!(
        *base,
        100 + *a_version,
        "the tail checks on the edited statement's environment"
    );
    assert_eq!(
        statuses(&mls),
        [
            (0, Status::Verified),
            (1, Status::Sorry),
            (2, Status::Sorry)
        ]
    );
}

#[test]
fn a_formalize_reply_for_an_edited_block_is_rejected() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let stale = run_until(&mut mls, |job| {
        matches!(
            job,
            Job::Formalize {
                label: Label::Statement,
                ..
            }
        )
    });
    let stale_reply = answer(&stale);

    mls.sync(&paras(&["Def d.", "Lemma a2."]));
    assert!(!mls.apply(stale_reply));
    let next = mls
        .next_job()
        .expect("the edited block is classified again");
    assert!(matches!(
        &next,
        Job::Classify { text, .. } if text == "Lemma a2."
    ));
}

#[test]
fn a_check_reply_for_an_edited_block_is_rejected() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let a = id_of(&mls, 1);
    let stale = run_until(
        &mut mls,
        |job| matches!(job, Job::Check { block, .. } if *block == a),
    );
    let stale_reply = answer(&stale);

    mls.sync(&paras(&["Def d.", "Lemma a2."]));
    assert!(!mls.apply(stale_reply));
    assert!(matches!(
        mls.next_job(),
        Some(Job::Classify { text, .. }) if text == "Lemma a2."
    ));
}

#[test]
fn an_edit_in_an_open_paragraph_waits_until_the_paragraph_is_closed() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    run(&mut mls, answer);
    let before = statuses(&mls);

    mls.sync(&paras_open(&["Def d.", "Lemma a2."], 1));
    assert!(mls.next_job().is_none());
    assert_eq!(statuses(&mls), before);

    mls.sync(&paras(&["Def d.", "Lemma a2."]));
    assert!(matches!(
        mls.next_job(),
        Some(Job::Classify { text, .. }) if text == "Lemma a2."
    ));
}

#[test]
fn a_new_open_paragraph_gets_no_job_and_no_dot() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d."]));
    run(&mut mls, answer);

    mls.sync(&paras_open(&["Def d.", "Lemma a"], 1));
    assert!(mls.next_job().is_none());
    assert_eq!(statuses(&mls), [(0, Status::Verified)]);
}

/// The reply of a service that is down, as with a missing key: classification fails.
fn service_down(job: &Job) -> Reply {
    match job {
        Job::Classify {
            paragraph,
            previous,
            text,
            proof_open,
        } => Reply::Classified {
            paragraph: *paragraph,
            previous: previous.clone(),
            text: text.clone(),
            proof_open: *proof_open,
            result: Err(Failure {
                message: MISSING_KEY.to_string(),
                service: true,
            }),
        },
        _ => panic!("only classification runs while the service is down"),
    }
}

#[test]
fn retry_runs_the_failed_job_again_without_an_edit() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello.", "World."]));
    let jobs = run(&mut mls, service_down);
    let failed = match &jobs[..] {
        [Job::Classify { paragraph, .. }, ..] => *paragraph,
        other => panic!("expected a classification first, got {other:?}"),
    };
    assert!(mls.next_job().is_none());

    mls.retry();
    let job = mls
        .next_job()
        .expect("retry makes the failed job run again");
    assert!(matches!(
        &job,
        Job::Classify { paragraph, text, .. } if *paragraph == failed && text == "Hello."
    ));
}

#[test]
fn a_classification_service_failure_stops_at_its_paragraph_until_a_change() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello.", "World."]));
    let jobs = run(&mut mls, service_down);
    assert_eq!(kinds(&jobs), ["classify"]);
    assert!(mls.next_job().is_none());

    let dots = mls.dots();
    assert_eq!(dots.len(), 1);
    assert_eq!(dots[0].paragraph, 0);
    assert_eq!(dots[0].key, id_of(&mls, 0));
    assert_eq!(dots[0].status, Status::Failed);
    assert_eq!(
        mls.details(id_of(&mls, 0)),
        [Details {
            label: None,
            status: Status::Failed,
            assumptions: Vec::new(),
            lean: None,
            error: Some(MISSING_KEY.to_string()),
            service: true,
            uses: Vec::new(),
            note: None,
        }]
    );

    mls.sync(&paras(&["Hello there.", "World."]));
    let retry = mls
        .next_job()
        .expect("a changed paragraph retries the service");
    assert!(matches!(
        &retry,
        Job::Classify { text, .. } if text == "Hello there."
    ));
}

#[test]
fn a_lost_repl_rechecks_every_block_from_its_base_without_formalizing_again() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let a = id_of(&mls, 1);
    let repl_down = |job: &Job| match job {
        Job::Check {
            block,
            version,
            base,
            ..
        } if *block == a => Reply::Checked {
            block: *block,
            version: *version,
            base: *base,
            result: Err(Failure {
                message: REPL_DOWN.to_string(),
                service: true,
            }),
        },
        _ => answer(job),
    };
    run(&mut mls, repl_down);
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Failed)]);

    mls.sync(&paras(&["Def d.", "Lemma a.", "Skip me."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify", "check", "check"]);
    let checked: Vec<u64> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Check { block, .. } => Some(*block),
            _ => None,
        })
        .collect();
    assert_eq!(checked, [id_of(&mls, 0), a]);
    let Job::Check { base, .. } = &jobs[1] else {
        panic!("the definition is checked")
    };
    assert_eq!(*base, 0);
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Sorry)]);
}

#[test]
fn editing_a_statement_reformalizes_its_proof_and_checks_it_after_the_statement() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a.", "Proof. ok ∎"]));
    run(&mut mls, answer);
    let (a, proof) = (id_of(&mls, 1), id_of(&mls, 2));

    mls.sync(&paras(&["Def d.", "Lemma a2.", "Proof. ok ∎"]));
    let jobs = run(&mut mls, answer);
    // The proof was tagged against the old statement text, so it is tagged again.
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "classify",
            "formalize",
            "check",
            "formalize",
            "check"
        ]
    );

    let Job::Check {
        block: checked_a,
        version: a_version,
        ..
    } = &jobs[3]
    else {
        panic!("the edited statement is checked")
    };
    assert_eq!(*checked_a, a);
    let Job::Formalize {
        block, statement, ..
    } = &jobs[4]
    else {
        panic!("the proof is formalized again")
    };
    assert_eq!(*block, proof);
    assert_eq!(
        statement.as_ref().map(|(version, _)| *version),
        Some(*a_version),
        "the proof is formalized against the edited statement"
    );
    let Job::Check { block, base, .. } = &jobs[5] else {
        panic!("the proof is checked")
    };
    assert_eq!(*block, proof);
    assert_eq!(
        *base,
        100 + *a_version,
        "the proof is checked after the edited statement"
    );
}

#[test]
fn next_job_is_none_while_a_job_is_in_flight() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d."]));
    let job = mls.next_job().expect("the paragraph is classified");
    assert!(mls.next_job().is_none());
    assert!(mls.apply(answer(&job)));
    assert!(mls.next_job().is_some());
}

#[test]
fn splitting_a_paragraph_keeps_the_ids_of_the_paragraphs_around_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Def e."]));
    run(&mut mls, answer);
    let (d, e) = (id_of(&mls, 0), id_of(&mls, 1));

    mls.sync(&paras(&["Def d.", "Def mid.", "Def e."]));
    assert_eq!((id_of(&mls, 0), id_of(&mls, 2)), (d, e));
    run(&mut mls, answer);

    let keys: Vec<u64> = mls.dots().iter().map(|dot| dot.key).collect();
    assert!(keys.contains(&d), "{keys:?}");
    assert!(keys.contains(&e), "{keys:?}");
}

/// The message of a check that ran past its deadline.
const TIMED_OUT: &str = "Lean took longer than 60 s on this block.";

/// The mock worker, except that the check of block `slow` times out.
fn times_out(slow: u64) -> impl Fn(&Job) -> Reply {
    move |job| match job {
        Job::Check {
            block,
            version,
            base,
            ..
        } if *block == slow => Reply::Checked {
            block: *block,
            version: *version,
            base: *base,
            result: Err(Failure {
                message: TIMED_OUT.to_string(),
                service: false,
            }),
        },
        _ => answer(job),
    }
}

#[test]
fn a_skip_edit_above_a_checked_statement_keeps_its_dots_while_it_is_classified() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello.", "Def d.", "Lemma a."]));
    run(&mut mls, answer);
    assert_eq!(statuses(&mls), [(1, Status::Verified), (2, Status::Sorry)]);

    mls.sync(&paras(&["Hello there.", "Def d.", "Lemma a."]));
    let classify = mls.next_job().expect("the edited paragraph is classified");
    assert!(matches!(&classify, Job::Classify { text, .. } if text == "Hello there."));
    assert_eq!(statuses(&mls), [(1, Status::Verified), (2, Status::Sorry)]);

    assert!(mls.apply(answer(&classify)));
    let retag = mls
        .next_job()
        .expect("the definition below is classified again");
    assert!(matches!(&retag, Job::Classify { text, .. } if text == "Def d."));
    assert!(mls.apply(answer(&retag)));
    assert!(
        mls.next_job().is_none(),
        "no Lean above the statement changed"
    );
    assert_eq!(statuses(&mls), [(1, Status::Verified), (2, Status::Sorry)]);
}

#[test]
fn an_edit_above_a_checked_block_keeps_its_dot_while_its_first_paragraph_is_retagged() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello.", "Lemma a."]));
    run(&mut mls, answer);
    let a = id_of(&mls, 1);
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);

    mls.sync(&paras(&["Hello there.", "Lemma a."]));
    let classify = mls.next_job().expect("the edited paragraph is classified");
    assert!(matches!(&classify, Job::Classify { text, .. } if text == "Hello there."));
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);
    assert!(mls.apply(answer(&classify)));

    let retag = mls
        .next_job()
        .expect("the statement is classified again against the new text above it");
    assert!(matches!(
        &retag,
        Job::Classify { paragraph, text, previous, .. }
            if *paragraph == a && text == "Lemma a." && previous.as_deref() == Some("Hello there.")
    ));
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);
    assert!(mls.apply(answer(&retag)));
    assert!(
        mls.next_job().is_none(),
        "the same tag means no formalize and no check"
    );
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);
}

#[test]
fn reset_while_busy_keeps_the_next_job_held_until_the_old_reply_is_applied() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d."]));
    let job = mls.next_job().expect("the paragraph is classified");
    mls.reset();
    assert!(
        mls.next_job().is_none(),
        "a job for the old document is still in flight"
    );
    assert!(
        !mls.apply(answer(&job)),
        "the old document's reply is stale"
    );
    mls.sync(&paras(&["Def e."]));
    assert!(matches!(
        mls.next_job(),
        Some(Job::Classify { text, .. }) if text == "Def e."
    ));
}

#[test]
fn blocks_above_a_failed_label_still_get_their_jobs() {
    fn fails_on_hello(job: &Job) -> Reply {
        match job {
            Job::Classify {
                paragraph,
                previous,
                text,
                proof_open,
            } if text == "Hello." => Reply::Classified {
                paragraph: *paragraph,
                previous: previous.clone(),
                text: text.clone(),
                proof_open: *proof_open,
                result: Err(Failure {
                    message: MISSING_KEY.to_string(),
                    service: true,
                }),
            },
            _ => answer(job),
        }
    }
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Hello."]));
    let jobs = run(&mut mls, fails_on_hello);
    assert_eq!(kinds(&jobs), ["classify", "classify", "formalize", "check"]);
    // The frontier's dot comes first in the plan, so the order of the dots is not checked.
    let dots = statuses(&mls);
    assert_eq!(dots.len(), 2, "{dots:?}");
    assert!(dots.contains(&(0, Status::Verified)), "{dots:?}");
    assert!(dots.contains(&(1, Status::Failed)), "{dots:?}");
}

#[test]
fn a_timed_out_check_clears_the_other_checks_so_they_run_again() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let (d, a) = (id_of(&mls, 0), id_of(&mls, 1));
    let jobs = run(&mut mls, times_out(a));

    assert!(
        matches!(jobs.last(), Some(Job::Check { block, .. }) if *block == d),
        "the definition is checked on the new REPL: {jobs:?}"
    );
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Failed)]);
    assert_eq!(item(&mls, a).error, Some(TIMED_OUT.to_string()));
}

#[test]
fn a_timed_out_check_is_not_checked_again_when_the_block_above_changes() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let a = id_of(&mls, 1);
    run(&mut mls, times_out(a));

    mls.sync(&paras(&["Def e.", "Lemma a."]));
    let jobs = run(&mut mls, answer);
    assert!(
        jobs.iter()
            .all(|job| !matches!(job, Job::Check { block, .. } if *block == a)),
        "{jobs:?}"
    );
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Failed)]);
}

#[test]
fn a_stale_lost_repl_still_clears_the_checks_on_the_next_change() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let (d, a) = (id_of(&mls, 0), id_of(&mls, 1));
    let Job::Check { version, base, .. } = run_until(
        &mut mls,
        |job| matches!(job, Job::Check { block, .. } if *block == a),
    ) else {
        panic!("the statement is checked")
    };
    let lost = Reply::Checked {
        block: a,
        version,
        base,
        result: Err(Failure {
            message: REPL_DOWN.to_string(),
            service: true,
        }),
    };

    mls.sync(&paras(&["Def d.", "Lemma a2."]));
    assert!(
        !mls.apply(lost),
        "the reply is for the statement's old text"
    );

    mls.sync(&paras(&["Def d.", "Lemma a2.", "Skip me."]));
    let jobs = run(&mut mls, answer);
    assert!(
        jobs.iter()
            .any(|job| matches!(job, Job::Check { block, .. } if *block == d)),
        "the lost REPL's checks run again: {jobs:?}"
    );
}

#[test]
fn paragraph_ids_are_unique_across_documents() {
    let lines = ["Def d.", "Lemma a."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    let mut second = Mls::new();
    second.sync(&paras(&lines));
    for index in 0..lines.len() {
        assert_ne!(id_of(&first, index), id_of(&second, index));
    }
}

/// A fresh `Mls` that restores `twin` and then syncs `lines`, not yet run.
fn restored(twin: &str, lines: &[&str]) -> Mls {
    let mut mls = Mls::new();
    mls.restore(twin);
    mls.sync(&paras(lines));
    mls
}

/// The formalize jobs' blocks, in order.
fn formalized(jobs: &[Job]) -> Vec<u64> {
    jobs.iter()
        .filter_map(|job| match job {
            Job::Formalize { block, .. } => Some(*block),
            _ => None,
        })
        .collect()
}

#[test]
fn a_twin_restores_labels_and_formals_so_a_reopened_document_only_checks() {
    let lines = ["Hello there.", "Def d.", "Lemma a.", "Proof. ok ∎"];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let mut second = restored(&first.twin(), &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check", "check", "check"]);
    assert_eq!(statuses(&second), statuses(&first));
}

#[test]
fn after_restore_only_an_edited_paragraph_and_the_one_below_are_classified() {
    let lines = ["Hello there.", "Def d.", "Lemma a.", "Proof. ok ∎"];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let mut second = restored(
        &first.twin(),
        &["Hello there.", "Def e.", "Lemma a.", "Proof. ok ∎"],
    );
    let jobs = run(&mut second, answer);
    let classified: Vec<&str> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Classify { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(classified, ["Def e.", "Lemma a."]);
    assert_eq!(formalized(&jobs), [id_of(&second, 1)]);
}

#[test]
fn a_proof_is_formalized_again_when_its_statement_changes_after_restore() {
    let lines = ["Def d.", "Lemma a.", "Proof. ok ∎"];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let mut second = restored(&first.twin(), &["Def d.", "Lemma b.", "Proof. ok ∎"]);
    let jobs = run(&mut second, answer);
    assert_eq!(
        formalized(&jobs),
        [id_of(&second, 1), id_of(&second, 2)],
        "the changed statement and its proof are formalized; the definition is restored"
    );
}

#[test]
fn a_proof_edited_with_its_statement_is_not_remembered_under_the_new_statement() {
    let mut first = Mls::new();
    first.sync(&paras(&["Lemma a.", "Proof. ok ∎"]));
    run(&mut first, answer);

    first.sync(&paras(&["Lemma b.", "Proof. ok ∎"]));
    // The new statement is formalized and its proof is now stale, waiting to be formalized again.
    let job = run_until(&mut first, |job| matches!(job, Job::Formalize { .. }));
    assert!(first.apply(answer(&job)));

    let twin = first.twin();
    let mut second = restored(&twin, &["Lemma b.", "Proof. ok ∎"]);
    let jobs = run(&mut second, answer);
    assert_eq!(
        formalized(&jobs),
        [id_of(&second, 1)],
        "the stale proof is formalized again, not restored: {twin}"
    );
}

#[test]
fn lean_with_comment_delimiters_and_quotes_round_trips_through_the_twin() {
    const LEAN: &str = "def s := \"a \\\"q\\\" -/ /- b\"";
    fn with_lean(job: &Job) -> Reply {
        match job {
            Job::Formalize { block, version, .. } => Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: None,
                result: Ok(Formal {
                    lean: LEAN.to_string(),
                    lean_name: Some("s".to_string()),
                    assumptions: vec!["x : Nat".to_string()],
                    uses: Vec::new(),
                }),
            },
            _ => answer(job),
        }
    }
    let lines = ["Def d."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, with_lean);
    let twin = first.twin();

    let memory = twin
        .split(MEMORY_OPEN)
        .nth(1)
        .expect("the twin has a memory comment");
    assert_eq!(
        (memory.matches("/-").count(), memory.matches("-/").count()),
        (0, 1),
        "only the memory's own end closes its comment: {twin}"
    );

    let mut second = restored(&twin, &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check"]);
    assert_eq!(
        item(&second, id_of(&second, 0)).lean,
        Some(LEAN.to_string())
    );
}

#[test]
fn a_rejected_formal_is_restored_as_a_failed_dot_without_a_request() {
    const REJECTED: &str = "The model reply was not Lean.";
    fn rejects(job: &Job) -> Reply {
        match job {
            Job::Formalize { block, version, .. } => Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: None,
                result: Err(Failure {
                    message: REJECTED.to_string(),
                    service: false,
                }),
            },
            _ => answer(job),
        }
    }
    let lines = ["Def d."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, rejects);
    assert_eq!(statuses(&first), [(0, Status::Failed)]);

    let mut second = restored(&first.twin(), &lines);
    let jobs = run(&mut second, answer);
    assert!(jobs.is_empty(), "{:?}", kinds(&jobs));
    assert_eq!(statuses(&second), [(0, Status::Failed)]);
    assert_eq!(
        item(&second, id_of(&second, 0)).error,
        Some(REJECTED.to_string())
    );
}

#[test]
fn a_restored_formal_that_the_rules_reject_is_a_failed_dot_and_is_never_checked() {
    const STATEMENT: &str = "#eval 1\ntheorem t : True := by sorry";
    const DEFINITION: &str = "axiom ax : False";
    // Stands in for a twin whose memory holds Lean that no model reply could pass the rules with.
    fn crafted(job: &Job) -> Reply {
        match job {
            Job::Formalize {
                block,
                version,
                label,
                ..
            } => Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: None,
                result: Ok(Formal {
                    lean: match label {
                        Label::Statement => STATEMENT,
                        Label::Definition => DEFINITION,
                        Label::Proof | Label::Skip => panic!("the document has no proof"),
                    }
                    .to_string(),
                    lean_name: None,
                    assumptions: Vec::new(),
                    uses: Vec::new(),
                }),
            },
            _ => answer(job),
        }
    }
    let lines = ["Def d.", "Lemma a."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, crafted);

    let mut second = restored(&first.twin(), &lines);
    let jobs = run(&mut second, answer);
    assert!(
        jobs.is_empty(),
        "restored Lean is not sent to Lean: {:?}",
        kinds(&jobs)
    );
    assert_eq!(
        statuses(&second),
        [(0, Status::Failed), (1, Status::Failed)]
    );
    assert_eq!(
        item(&second, id_of(&second, 0)).error,
        Some("The Lean declares an axiom.".to_string())
    );
    assert_eq!(
        item(&second, id_of(&second, 1)).error,
        Some("The Lean uses `#eval`, which is not allowed.".to_string())
    );
}

#[test]
fn a_missing_or_malformed_memory_restores_nothing() {
    let lines = ["Hello.", "Def d.", "Lemma a."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);
    let twin = first.twin();
    assert!(twin.contains("\"version\":3"), "{twin}");

    let mut baseline = Mls::new();
    baseline.sync(&paras(&lines));
    let expected = kinds(&run(&mut baseline, answer));

    let lean_only = &twin[..twin.rfind(MEMORY_OPEN).expect("the twin has a memory")];
    let cases = [
        ("no memory comment", lean_only.to_string()),
        (
            "garbage memory",
            format!("{lean_only}{MEMORY_OPEN}not json\n-/\n"),
        ),
        (
            "version 1 memory",
            twin.replace("\"version\":3", "\"version\":1"),
        ),
        (
            "version 2 memory",
            twin.replace("\"version\":3", "\"version\":2"),
        ),
        (
            "unmarked version 3 memory",
            twin.replace("\"segmentation_version\":2,", ""),
        ),
        (
            "old segmentation revision",
            twin.replace("\"segmentation_version\":2", "\"segmentation_version\":1"),
        ),
        (
            "no closing delimiter",
            twin.trim_end_matches("-/\n").to_string(),
        ),
    ];
    for (name, text) in cases {
        let mut mls = Mls::new();
        mls.restore(&text);
        mls.sync(&paras(&lines));
        assert_eq!(kinds(&run(&mut mls, answer)), expected, "{name}");
    }
}

#[test]
fn a_twin_starts_with_the_mathlib_import_and_ends_with_the_memory_comment() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello."]));
    run(&mut mls, answer);
    let twin = mls.twin();
    assert!(twin.starts_with("import Mathlib\n"), "{twin}");
    assert!(twin.ends_with("\n-/\n"), "{twin}");
}

#[test]
fn an_undone_edit_reuses_the_remembered_label_and_formal() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Lemma a."]));
    run(&mut mls, answer);
    mls.sync(&paras(&["Def d.", "Lemma a2."]));
    run(&mut mls, answer);

    mls.sync(&paras(&["Def d.", "Lemma a."]));
    let jobs = run(&mut mls, answer);
    assert!(
        jobs.iter()
            .all(|job| !matches!(job, Job::Classify { .. } | Job::Formalize { .. })),
        "{:?}",
        kinds(&jobs)
    );
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Sorry)]);
}

#[test]
fn a_setup_line_and_the_then_line_after_it_are_one_statement_block() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Let x be real.", "Then x = x."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(formalize_texts(&jobs), ["Let x be real.\n\nThen x = x."]);
    let dots = mls.dots();
    assert_eq!(dots.len(), 1, "{dots:?}");
    assert_eq!((dots[0].paragraph, dots[0].last), (0, 1));
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);
}

#[test]
fn a_plus_line_after_a_statement_joins_it_without_a_dot_of_its_own() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "+ And more."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(formalize_texts(&jobs), ["Lemma t.\n\n+ And more."]);
    let dots = mls.dots();
    assert_eq!(dots.len(), 1, "{dots:?}");
    assert_eq!((dots[0].paragraph, dots[0].last), (0, 1));
}

#[test]
fn a_statement_ending_in_a_comma_waits_for_the_paragraph_that_continues_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t,"]));
    let jobs = run(&mut mls, answer);
    assert!(formalize_texts(&jobs).is_empty(), "{jobs:?}");
    assert!(mls.dots().is_empty());

    mls.sync(&paras(&["Lemma t,", "+ and more."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(formalize_texts(&jobs), ["Lemma t,\n\n+ and more."]);
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);
}

#[test]
fn a_then_line_after_a_definition_starts_its_own_statement() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d.", "Then x = x."]));
    run(&mut mls, answer);
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Sorry)]);
}

#[test]
fn a_heading_is_never_classified_and_gets_no_dot() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["## Section", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    assert!(
        !jobs
            .iter()
            .any(|job| matches!(job, Job::Classify { text, .. } if text == "## Section")),
        "{jobs:?}"
    );
    assert_eq!(statuses(&mls), [(1, Status::Sorry)]);
}

#[test]
fn a_statement_under_a_heading_never_joins_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["# Title", "Then x = x."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(formalize_texts(&jobs), ["Then x = x."]);
    let dots = mls.dots();
    assert_eq!(dots.len(), 1, "{dots:?}");
    assert_eq!((dots[0].paragraph, dots[0].last), (1, 1));
}

#[test]
fn a_hash_without_a_space_or_a_seventh_hash_is_not_a_heading() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["#hashtag", "####### seven"]));
    let jobs = run(&mut mls, answer);
    for text in ["#hashtag", "####### seven"] {
        assert!(
            jobs.iter()
                .any(|job| matches!(job, Job::Classify { text: t, .. } if t == text)),
            "{text} is classified"
        );
    }
}

#[test]
fn a_plus_line_after_a_proof_that_has_ended_is_not_part_of_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Done. ∎", "+ Not part of it."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(formalize_texts(&jobs), ["Lemma t.", "Proof. Done. ∎"]);
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn a_block_takes_the_whole_prose_unit_above_it() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello there.", "+ And more.", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    let above: Vec<Option<String>> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Formalize { above, .. } => Some(above.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(above, [Some("Hello there.\n\n+ And more.".to_string())]);
}

#[test]
fn the_twin_remembers_what_jev_said_about_joining() {
    let lines = ["Let x be real.", "Then x = x."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let twin = first.twin();
    assert!(twin.contains("\"version\":3"), "{twin}");
    assert!(twin.contains("\"joins\":true"), "{twin}");
    assert!(twin.contains("\"previous\":\"Let x be real.\""), "{twin}");

    let mut second = restored(&twin, &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check"]);
    assert_eq!(statuses(&second), [(0, Status::Sorry)]);
}

/// The reply of a splitter that is down: the cut fails for a service.
fn split_down(job: &Job) -> Reply {
    match job {
        Job::Split {
            paragraph,
            text,
            context,
        } => Reply::Split {
            paragraph: *paragraph,
            text: text.clone(),
            context: context.clone(),
            result: Err(Failure {
                message: MISSING_KEY.to_string(),
                service: true,
            }),
        },
        _ => answer(job),
    }
}

#[test]
fn a_paragraph_with_a_statement_and_its_proof_is_read_as_two_blocks() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t. / Proof. ok ∎"]));
    let jobs = run(&mut mls, answer);
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "split",
            "formalize",
            "check",
            "formalize",
            "check"
        ]
    );
    let (statement, proof) = (mls.blocks[0].id, mls.blocks[1].id);
    assert_ne!(statement, proof);
    assert!(matches!(
        &jobs[1],
        Job::Split { paragraph, text, .. } if *paragraph == id_of(&mls, 0) && text == "Lemma t. / Proof. ok ∎"
    ));
    assert!(matches!(
        &jobs[2],
        Job::Formalize { block, label: Label::Statement, text, .. } if *block == statement && text == "Lemma t. /"
    ));
    assert!(matches!(
        &jobs[4],
        Job::Formalize { block, label: Label::Proof, text, .. }
            if *block == proof && text == "Proof. ok ∎"
    ));
    assert_eq!(formalized(&jobs), [statement, proof]);
    let Job::Formalize {
        statement: above, ..
    } = &jobs[4]
    else {
        panic!("the proof is formalized")
    };
    assert!(above.is_some(), "the proof pairs with the statement piece");
    let Job::Check { block, base, .. } = &jobs[5] else {
        panic!("the proof is checked")
    };
    assert_eq!(
        (*block, *base),
        (proof, 100 + version_of(&mls, statement)),
        "a proof is checked after its statement, on the environment that statement left"
    );

    assert_eq!(statuses(&mls), [(0, Status::Verified)]);
    let dots = mls.dots();
    assert_eq!((dots[0].paragraph, dots[0].last), (0, 0));
    assert_eq!(dots[0].key, statement, "the dot has its first item's key");
    let items = mls.details(statement);
    let labels: Vec<Option<Label>> = items.iter().map(|item| item.label).collect();
    assert_eq!(labels, [Some(Label::Statement), Some(Label::Proof)]);
    assert!(items.iter().all(|item| item.status == Status::Verified));
    assert!(items.iter().all(|item| item.lean.is_some()));
    assert!(
        mls.details(proof).is_empty(),
        "only a group's first key names it"
    );
}

#[test]
fn a_definition_a_statement_and_a_proof_in_one_paragraph_are_three_blocks_and_one_dot() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d. / Lemma t. / Proof. ok ∎"]));
    run(&mut mls, answer);
    let labels: Vec<Label> = mls.blocks.iter().map(|block| block.label).collect();
    assert_eq!(labels, [Label::Definition, Label::Statement, Label::Proof]);
    assert_eq!(statuses(&mls), [(0, Status::Verified)]);
    assert_eq!(mls.details(mls.blocks[0].id).len(), 3);
}

fn labeled_reply(job: &Job, items: Vec<Segment>) -> Reply {
    let Job::Split {
        paragraph,
        text,
        context,
    } = job
    else {
        panic!("expected segmentation")
    };
    Reply::Split {
        paragraph: *paragraph,
        text: text.clone(),
        context: context.clone(),
        result: Ok(items),
    }
}

fn labeled_items(pieces: &[(Label, &str, Option<u64>)]) -> Vec<Segment> {
    pieces
        .iter()
        .enumerate()
        .map(|(index, (label, text, proves))| Segment {
            id: index as u64 + 1,
            label: *label,
            text: text.to_string(),
            proves: *proves,
            proves_previous: false,
        })
        .collect()
}

#[test]
fn mixed_mathematics_uses_authoritative_labels_and_reaches_statement_and_proof_checks() {
    let cases = [
        vec![
            (
                Label::Statement,
                "**Lemma.** For every natural number $n$, $n^2+n$ is even.",
                None,
            ),
            (
                Label::Proof,
                "Indeed, $n^2+n=n(n+1)$ and one of $n$ and $n+1$ is even. ∎",
                Some(1),
            ),
        ],
        vec![
            (
                Label::Definition,
                "Define $q(x)=(x-1)^2$ for real $x$.",
                None,
            ),
            (
                Label::Statement,
                r"Then $q(x)\ge 0$ for every real $x$,",
                None,
            ),
            (Label::Proof, "since squares are nonnegative.", Some(2)),
        ],
        vec![
            (Label::Statement, "Every real square is nonnegative.", None),
            (
                Label::Statement,
                "Every natural number doubled is even.",
                None,
            ),
        ],
    ];
    for pieces in cases {
        let text = pieces
            .iter()
            .map(|(_, text, _)| *text)
            .collect::<Vec<_>>()
            .join(" ");
        let items = labeled_items(&pieces);
        let mut mls = Mls::new();
        mls.sync(&paras(&[&text]));
        let jobs = run(&mut mls, |job| match job {
            Job::Classify { .. } => {
                let mut reply = answer(job);
                if let Reply::Classified { result, .. } = &mut reply {
                    *result = Ok(Tag {
                        label: Label::Skip,
                        joins: false,
                        split: true,
                    });
                }
                reply
            }
            Job::Split { .. } => labeled_reply(job, items.clone()),
            _ => answer(job),
        });
        assert_eq!(
            jobs.iter()
                .filter(|job| matches!(job, Job::Classify { .. }))
                .count(),
            1
        );
        assert_eq!(
            mls.blocks
                .iter()
                .map(|block| block.label)
                .collect::<Vec<_>>(),
            items.iter().map(|item| item.label).collect::<Vec<_>>()
        );
        assert_eq!(
            jobs.iter()
                .filter(|job| matches!(job, Job::Check { .. }))
                .count(),
            items.len()
        );
        for block in mls
            .blocks
            .iter()
            .filter(|block| block.label == Label::Proof)
        {
            assert_eq!(block.statement, Some(mls.blocks[mls.blocks.len() - 2].id));
        }
        mls.sync(&paras(&[&text]));
        assert!(
            mls.next_job().is_none(),
            "unchanged labeled proof must not loop"
        );
    }
}

#[test]
fn an_ordinary_unpaired_proof_gets_exactly_one_segmentation_recovery() {
    let text = "Proof. A claim and its argument.";
    let mut mls = Mls::new();
    mls.sync(&paras(&[text]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify", "split"]);
    assert_eq!(statuses(&mls), [(0, Status::Failed)]);
    mls.retry();
    mls.sync(&paras(&[text]));
    assert!(mls.next_job().is_none());
    let mut reopened = restored(&mls.twin(), &[text]);
    assert!(
        reopened.next_job().is_none(),
        "unpaired labeled proof remains a structural error after reopen"
    );

    let mut recovered = Mls::new();
    recovered.sync(&paras(&[text]));
    let jobs = run(&mut recovered, |job| match job {
        Job::Split { .. } => labeled_reply(
            job,
            labeled_items(&[
                (Label::Statement, "Proof. A claim", None),
                (Label::Proof, "and its argument.", Some(1)),
            ]),
        ),
        _ => answer(job),
    });
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "split",
            "formalize",
            "check",
            "formalize",
            "check"
        ]
    );
    assert_eq!(statuses(&recovered), [(0, Status::Verified)]);
}

#[test]
fn recovery_after_a_definition_checks_the_extracted_statement_before_its_proof() {
    let definition = "Define $q(x)=(x-1)^2$ for real $x$.";
    let residual = r"Then $q(x)\ge 0$ for every real $x$, since squares are nonnegative.";
    let pieces = labeled_items(&[
        (
            Label::Statement,
            r"Then $q(x)\ge 0$ for every real $x$,",
            None,
        ),
        (Label::Proof, "since squares are nonnegative.", Some(1)),
    ]);
    let mut mls = Mls::new();
    mls.sync(&paras(&[definition, residual]));
    let jobs = run(&mut mls, |job| match job {
        Job::Classify { text, .. } if text == residual => {
            let mut reply = answer(job);
            if let Reply::Classified { result, .. } = &mut reply {
                // A false-negative gate followed by a proof label must still recover generically.
                *result = Ok(Tag {
                    label: Label::Proof,
                    joins: false,
                    split: false,
                });
            }
            reply
        }
        Job::Split { context, .. } => {
            assert_eq!(
                context.previous,
                Some((Label::Definition, definition.to_string()))
            );
            assert_eq!(context.statement, None);
            labeled_reply(job, pieces.clone())
        }
        _ => answer(job),
    });
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "classify",
            "split",
            "formalize",
            "check",
            "formalize",
            "check",
            "formalize",
            "check"
        ]
    );
    let definition_env = 100 + mls.blocks[0].version;
    let statement_env = 100 + mls.blocks[1].version;
    let bases: Vec<u64> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Check { base, .. } => Some(*base),
            _ => None,
        })
        .collect();
    assert_eq!(bases, [0, definition_env, statement_env]);
    let lean_above = jobs
        .iter()
        .find_map(|job| match job {
            Job::Formalize {
                label: Label::Statement,
                lean_above,
                ..
            } => Some(lean_above),
            _ => None,
        })
        .unwrap();
    assert_eq!(lean_above.len(), 1, "the definition remains in context");
    assert_eq!(lean_above[0].0, mls.blocks[0].id);
    assert_eq!(mls.blocks[2].statement, Some(mls.blocks[1].id));
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
    mls.retry();
    assert!(mls.next_job().is_none(), "recovery is not repeated");

    let mut reopened = restored(&mls.twin(), &[definition, residual]);
    assert_eq!(
        kinds(&run(&mut reopened, answer)),
        ["check", "check", "check"]
    );
}

#[test]
fn a_labeled_proof_targeting_the_previous_statement_continues_its_open_proof() {
    let mut mls = Mls::new();
    let before = ["Lemma t.", "Proof. first step"];
    mls.sync(&paras(&before));
    run(&mut mls, answer);
    let proof_id = mls.blocks[1].id;
    let continuation = "continuation / with its last step ∎";
    mls.sync(&paras(&[before[0], before[1], continuation]));
    let jobs = run(&mut mls, |job| match job {
        Job::Split { context, .. } => {
            assert_eq!(context.statement.as_deref(), Some(before[0]));
            assert_eq!(
                context.previous,
                Some((Label::Proof, before[1].to_string()))
            );
            labeled_reply(
                job,
                vec![Segment {
                    id: 1,
                    label: Label::Proof,
                    text: continuation.to_string(),
                    proves: None,
                    proves_previous: true,
                }],
            )
        }
        _ => answer(job),
    });
    assert_eq!(kinds(&jobs), ["classify", "split", "formalize", "check"]);
    assert_eq!(mls.blocks.len(), 2, "the proof remains a single block");
    assert_eq!(mls.blocks[1].id, proof_id);
    assert_eq!(mls.blocks[1].statement, Some(mls.blocks[0].id));
    assert_eq!(
        mls.blocks[1].text,
        format!("{}\n\n{continuation}", before[1])
    );
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn labeled_items_preserve_preceding_and_internal_prose_for_formalization() {
    let convention = "Throughout, $x$ is a real number.";
    let statement = r"Lemma. $x^2 \ge 0$.";
    let proof = "Proof. Squares are nonnegative. ∎";
    let mixed = format!("{statement} {proof}");
    let internal = format!("{convention} {mixed}");
    for (lines, text, pieces) in [
        (
            vec![convention, mixed.as_str()],
            mixed.as_str(),
            vec![
                (Label::Statement, statement, None),
                (Label::Proof, proof, Some(1)),
            ],
        ),
        (
            vec![internal.as_str()],
            internal.as_str(),
            vec![
                (Label::Skip, convention, None),
                (Label::Statement, statement, None),
                (Label::Proof, proof, Some(2)),
            ],
        ),
    ] {
        let mut mls = Mls::new();
        mls.sync(&paras(&lines));
        let jobs = run(&mut mls, |job| match job {
            Job::Classify { text: current, .. } if current == text => {
                let mut reply = answer(job);
                if let Reply::Classified { result, .. } = &mut reply {
                    *result = Ok(Tag {
                        label: Label::Skip,
                        joins: false,
                        split: true,
                    });
                }
                reply
            }
            Job::Split { .. } => labeled_reply(job, labeled_items(&pieces)),
            _ => answer(job),
        });
        let above = jobs.iter().find_map(|job| match job {
            Job::Formalize {
                label: Label::Statement,
                above,
                ..
            } => Some(above.as_deref()),
            _ => None,
        });
        assert_eq!(above, Some(Some(convention)), "{lines:?}");
        assert_eq!(mls.blocks.len(), 2, "the items stay separate");
        assert_eq!(mls.blocks[1].statement, Some(mls.blocks[0].id));
    }
}

#[test]
fn labeled_proofs_can_target_the_preceding_statement_across_prose() {
    let statement = "Lemma. $x=x$.";
    let prose = "We give the elementary argument.";
    let proof = "Proof. Reflexivity. ∎";
    let next = "Lemma. $0=0$.";
    let mixed = format!("{proof} {next}");
    let mut mls = Mls::new();
    mls.sync(&paras(&[statement, prose, &mixed]));
    let jobs = run(&mut mls, |job| match job {
        Job::Classify { text, .. } if text == &mixed => {
            let mut reply = answer(job);
            if let Reply::Classified { result, .. } = &mut reply {
                *result = Ok(Tag {
                    label: Label::Skip,
                    joins: false,
                    split: true,
                });
            }
            reply
        }
        Job::Split { context, .. } => {
            assert_eq!(context.previous, Some((Label::Skip, prose.to_string())));
            assert_eq!(context.statement.as_deref(), Some(statement));
            let mut pieces =
                labeled_items(&[(Label::Proof, proof, None), (Label::Statement, next, None)]);
            pieces[0].proves_previous = true;
            labeled_reply(job, pieces)
        }
        _ => answer(job),
    });
    assert_eq!(mls.blocks.len(), 3);
    assert_eq!(mls.blocks[1].statement, Some(mls.blocks[0].id));
    let proof_job = jobs
        .iter()
        .find(|job| {
            matches!(
                job,
                Job::Formalize {
                    label: Label::Proof,
                    ..
                }
            )
        })
        .unwrap();
    let Job::Formalize {
        statement: target,
        above: context,
        ..
    } = proof_job
    else {
        unreachable!()
    };
    let above = format!("{statement}\n\n{prose}");
    assert_eq!(context.as_deref(), Some(above.as_str()));
    assert_eq!(
        target.as_ref().unwrap().1,
        format!("theorem t{} : True := by sorry", mls.blocks[0].id)
    );
}

#[test]
fn labeled_context_does_not_reuse_statements_across_math_or_ended_proofs() {
    let statement = "Lemma. $x=x$.";
    for prefix in [
        vec![statement, "Def d.", "Some prose."],
        vec![statement, "Proof. done ∎", "Some prose."],
        vec![statement, "Proof. first step", "Some prose."],
        vec![statement, "Proof. done ∎"],
    ] {
        let mut mls = Mls::new();
        mls.sync(&paras(&prefix));
        run(&mut mls, answer);
        let mut lines = prefix.clone();
        lines.push("Proof. another argument. / Lemma. $0=0$.");
        mls.sync(&paras(&lines));
        let split = run_until(&mut mls, |job| matches!(job, Job::Split { .. }));
        let Job::Split { context, .. } = &split else {
            unreachable!()
        };
        assert_eq!(context.statement, None, "{prefix:?}");
        let mut pieces = labeled_items(&[
            (Label::Proof, "Proof. another argument. /", None),
            (Label::Statement, "Lemma. $0=0$.", None),
        ]);
        pieces[0].proves_previous = true;
        assert!(mls.apply(labeled_reply(&split, pieces)));
        assert!(mls.next_job().is_none(), "invalid link must not formalize");
        let dot = mls.dots().pop().unwrap();
        let details = mls.details(dot.key);
        assert_eq!(details[0].status, Status::Failed);
        assert!(
            details[0]
                .error
                .as_ref()
                .unwrap()
                .contains("a proof must target an earlier statement"),
            "{prefix:?}"
        );
    }
}

#[test]
fn split_replies_with_stale_context_are_ignored_and_editing_above_resegments() {
    let mut mls = Mls::new();
    let mixed = "Lemma t. / Proof. ok ∎";
    mls.sync(&paras(&["Def a.", mixed]));
    let stale = run_until(&mut mls, |job| matches!(job, Job::Split { .. }));
    mls.sync(&paras(&["Def b.", mixed]));
    assert!(!mls.apply(answer(&stale)));
    let jobs = run(&mut mls, answer);
    assert_eq!(
        jobs.iter()
            .filter(|job| matches!(job, Job::Split { .. }))
            .count(),
        1
    );
    mls.sync(&paras(&["Def c.", mixed]));
    let jobs = run(&mut mls, answer);
    assert_eq!(
        jobs.iter()
            .filter(|job| matches!(job, Job::Split { .. }))
            .count(),
        1
    );
}

#[test]
fn malformed_coverage_or_proof_links_cannot_enter_the_pipeline_cache() {
    for items in [
        labeled_items(&[(Label::Statement, "Lemma t.", None)]),
        labeled_items(&[
            (Label::Statement, "Lemma t. /", None),
            (Label::Proof, "Proof. ok ∎", Some(2)),
        ]),
    ] {
        let mut mls = Mls::new();
        mls.sync(&paras(&["Lemma t. / Proof. ok ∎"]));
        let jobs = run(&mut mls, |job| match job {
            Job::Split { .. } => labeled_reply(job, items.clone()),
            _ => answer(job),
        });
        assert_eq!(kinds(&jobs), ["classify", "split"]);
        let details = mls.details(mls.dots()[0].key);
        assert_eq!(details[0].label, None);
        assert!(!details[0].service);
        assert!(
            details[0]
                .error
                .as_ref()
                .unwrap()
                .contains("Invalid labeled segmentation")
        );
        assert!(mls.splits.is_empty());
    }
}

#[test]
fn a_false_proof_in_a_split_paragraph_makes_the_dot_red_and_lists_each_items_status() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d. / Lemma t. / Proof. FALSE ∎"]));
    run(&mut mls, answer);
    assert_eq!(statuses(&mls), [(0, Status::Failed)]);
    let items = mls.details(mls.blocks[0].id);
    let seen: Vec<(Option<Label>, Status)> =
        items.iter().map(|item| (item.label, item.status)).collect();
    assert_eq!(
        seen,
        [
            (Some(Label::Definition), Status::Verified),
            (Some(Label::Statement), Status::Sorry),
            (Some(Label::Proof), Status::Failed)
        ]
    );
    assert_eq!(items[2].error.as_deref(), Some("1:0: error"));
    assert_eq!(items[0].error, None);
}

#[test]
fn a_dot_takes_the_worst_status_of_its_items() {
    // The statement piece has no proof, so it is amber beside the verified definition.
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d. / Lemma t."]));
    run(&mut mls, answer);
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);

    let items = |status: &[Status]| -> Vec<Dot> {
        status
            .iter()
            .enumerate()
            .map(|(i, &status)| dot(0, 0, i as u64, status))
            .collect()
    };
    let worst = |status: &[Status]| merge_dots(&group_dots(&items(status))[0]).status;
    assert_eq!(
        worst(&[Status::Verified, Status::Sorry, Status::Checking]),
        Status::Checking
    );
    assert_eq!(
        worst(&[Status::Sorry, Status::Failed, Status::Checking]),
        Status::Failed
    );
    assert_eq!(
        worst(&[Status::Verified, Status::Verified]),
        Status::Verified
    );
}

#[test]
fn dots_group_while_their_ranges_overlap_and_not_when_they_only_touch() {
    let dots = [
        dot(2, 2, 5, Status::Sorry),
        dot(0, 0, 1, Status::Verified),
        dot(0, 1, 2, Status::Verified),
        dot(1, 1, 3, Status::Verified),
        dot(3, 3, 6, Status::Verified),
    ];
    let keys: Vec<Vec<u64>> = group_dots(&dots)
        .iter()
        .map(|group| group.iter().map(|d| d.key).collect())
        .collect();
    assert_eq!(keys, [vec![1, 2, 3], vec![5], vec![6]]);
}

#[test]
fn a_proof_that_starts_in_a_split_paragraph_continues_in_the_next() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t. / Proof. begin", "then more ∎"]));
    run(&mut mls, answer);
    let ranges: Vec<(usize, usize)> = mls.dots().iter().map(|d| (d.paragraph, d.last)).collect();
    assert_eq!(ranges, [(0, 1)]);
    assert_eq!(statuses(&mls), [(0, Status::Verified)]);
    assert_eq!(mls.details(mls.blocks[0].id).len(), 2);
}

#[test]
fn an_invalid_split_reports_a_permanent_structural_failure() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma UNSPLIT / t."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify", "split"]);
    assert_eq!(statuses(&mls), [(0, Status::Failed)]);
    assert!(formalize_texts(&jobs).is_empty());
    assert!(!mls.details(id_of(&mls, 0))[0].service);
    mls.retry();
    assert!(mls.next_job().is_none());

    mls.sync(&paras(&["Lemma UNSPLIT / t.", "Hello."]));
    let jobs = run(&mut mls, answer);
    assert!(jobs.is_empty(), "the structural failure remains actionable");
    assert_eq!(statuses(&mls), [(0, Status::Failed)]);
}

#[test]
fn a_split_service_failure_is_a_red_dot_until_a_retry() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t. / Proof. ok ∎", "Hello."]));
    let jobs = run(&mut mls, split_down);
    assert_eq!(kinds(&jobs), ["classify", "split"]);
    assert!(mls.next_job().is_none());

    let dots = mls.dots();
    assert_eq!(dots.len(), 1);
    assert_eq!((dots[0].paragraph, dots[0].last), (0, 0));
    assert_eq!(dots[0].key, id_of(&mls, 0));
    assert_eq!(dots[0].status, Status::Failed);
    assert_eq!(
        mls.details(id_of(&mls, 0)),
        [Details {
            label: None,
            status: Status::Failed,
            assumptions: Vec::new(),
            lean: None,
            error: Some(MISSING_KEY.to_string()),
            service: true,
            uses: Vec::new(),
            note: None,
        }]
    );

    mls.retry();
    let job = mls.next_job().expect("retry asks for the split again");
    assert!(matches!(
        &job,
        Job::Split { paragraph, .. } if *paragraph == id_of(&mls, 0)
    ));
}

#[test]
fn editing_only_the_proof_piece_keeps_the_statement_pieces_block() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t. / Proof. ok ∎"]));
    run(&mut mls, answer);
    let (statement, proof) = (mls.blocks[0].id, mls.blocks[1].id);
    let version = version_of(&mls, statement);

    mls.sync(&paras(&["Lemma t. / Proof. fine ∎"]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify", "split", "formalize", "check"]);
    assert_eq!(mls.blocks[0].id, statement);
    assert_eq!(version_of(&mls, statement), version);
    assert_ne!(mls.blocks[1].id, proof);
    assert_eq!(formalized(&jobs), [mls.blocks[1].id]);
    assert_eq!(statuses(&mls), [(0, Status::Verified)]);
}

#[test]
fn a_twin_restores_the_split_so_a_reopened_document_asks_no_model() {
    let lines = ["Hello there.", "Lemma t. / Proof. ok ∎", "Lemma u."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let twin = first.twin();
    assert!(twin.contains("\"splits\":[{"), "{twin}");
    let mut second = restored(&twin, &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check", "check", "check"]);
    assert_eq!(statuses(&second), statuses(&first));
}

#[test]
fn a_restored_split_that_drops_text_is_ignored_and_asked_again() {
    let lines = ["Hello there.", "Lemma t. / Proof. ok ∎"];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let twin = first.twin();
    let (lean, body) = twin.split_once(MEMORY_OPEN).unwrap();
    let mut memory: Value = serde_json::from_str(body.split_once("\n-/").unwrap().0).unwrap();
    memory["splits"][0]["segments"]["items"][1]["text"] = json!("Proof.");
    let edited = format!("{lean}{MEMORY_OPEN}{memory}\n-/\n");
    assert_ne!(edited, twin, "the split entry is edited");
    let mut second = restored(&edited, &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(
        kinds(&jobs),
        ["split", "check", "check"],
        "only the split is asked again; the labels and formals are restored"
    );
    assert_eq!(statuses(&second), statuses(&first));
}

/// The chart's node for the block with `key`.
fn node(mls: &Mls, key: u64) -> Node {
    mls.chart()
        .into_iter()
        .find(|node| node.key == key)
        .expect("the block has a chart node")
}

#[test]
fn chart_links_a_statement_to_the_definition_it_uses() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def a.", "Lemma USES."]));
    run(&mut mls, answer);
    let (def, lemma) = (id_of(&mls, 0), id_of(&mls, 1));

    let chart = mls.chart();
    assert_eq!(chart.len(), 2, "{chart:?}");
    assert_eq!(chart[0].key, def);
    assert_eq!(chart[0].level, 0);
    assert!(chart[0].uses.is_empty());
    assert_eq!(chart[1].key, lemma);
    assert_eq!(chart[1].level, 1);
    assert_eq!(chart[1].uses, [def]);
    assert_eq!(chart[1].status, Status::Sorry);
    assert_eq!(item(&mls, lemma).uses, [format!("d{def}")]);
}

#[test]
fn a_proofs_uses_go_to_its_statements_node() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def a.", "Lemma b.", "Proof USES ∎"]));
    run(&mut mls, answer);
    let (def, lemma) = (id_of(&mls, 0), id_of(&mls, 1));

    let chart = mls.chart();
    assert_eq!(chart.len(), 2, "{chart:?}");
    assert_eq!(node(&mls, lemma).uses, [def]);
    assert_eq!(node(&mls, lemma).status, Status::Verified);
    assert_eq!(item(&mls, id_of(&mls, 2)).uses, [format!("d{def}")]);
}

#[test]
fn a_statement_whose_used_definition_is_removed_is_formalized_again() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def a.", "Lemma USES."]));
    run(&mut mls, answer);
    let lemma = id_of(&mls, 1);

    mls.sync(&paras(&["Lemma USES."]));
    let jobs = run(&mut mls, answer);
    assert!(formalized(&jobs).contains(&lemma), "{:?}", kinds(&jobs));
}

#[test]
fn a_statement_that_uses_a_failed_definition_fails_without_a_check() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def a.", "Lemma USES."]));
    run(&mut mls, answer);
    let (def, lemma) = (id_of(&mls, 0), id_of(&mls, 1));

    // The definition is edited so that it fails. The statement's formal already names it.
    mls.sync(&paras(&["Def FALSE.", "Lemma USES."]));
    let jobs = run(&mut mls, answer);
    assert!(
        jobs.iter()
            .all(|job| !matches!(job, Job::Check { block, .. } if *block == lemma)),
        "{:?}",
        kinds(&jobs)
    );
    assert_eq!(statuses(&mls), [(0, Status::Failed), (1, Status::Failed)]);
    assert_eq!(
        item(&mls, lemma).note,
        Some(format!("Uses `d{def}`, which does not check."))
    );
}

#[test]
fn a_proof_that_relies_on_an_unproved_statement_is_amber() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma a.", "Lemma b.", "Proof USES ∎"]));
    run(&mut mls, answer);
    let (a, proof) = (id_of(&mls, 0), id_of(&mls, 2));

    assert_eq!(
        statuses(&mls),
        [(0, Status::Sorry), (1, Status::Sorry), (2, Status::Sorry)]
    );
    let details = item(&mls, proof);
    assert_eq!(
        details.note,
        Some(format!("Relies on `t{a}`, which is not proved yet."))
    );
    assert_eq!(details.error, None);
}

#[test]
fn the_twin_round_trips_the_uses_so_the_restored_chart_has_the_same_shape() {
    fn shape(mls: &Mls) -> Vec<(Status, usize, usize)> {
        mls.chart()
            .iter()
            .map(|node| (node.status, node.level, node.uses.len()))
            .collect()
    }
    let lines = ["Def a.", "Lemma USES."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let mut second = restored(&first.twin(), &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check", "check"]);
    assert_eq!(shape(&second), shape(&first));
    assert_eq!(
        shape(&second),
        [(Status::Verified, 0, 0), (Status::Sorry, 1, 1)]
    );
}

#[test]
fn jev_sees_every_paragraph_of_the_block_above_not_only_the_last() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma for all n,", "+ n = n.", "Proof. Clear. ∎"]));
    let jobs = run(&mut mls, answer);
    let proof = id_of(&mls, 2);

    assert!(jobs.iter().any(|job| matches!(
        job,
        Job::Classify { paragraph, previous: Some(above), .. }
            if *paragraph == proof && above == "Lemma for all n,\n\n+ n = n."
    )));
}

fn version_of(mls: &Mls, id: u64) -> u64 {
    mls.blocks
        .iter()
        .find(|b| b.id == id)
        .expect("the block is in the document")
        .version
}

fn repairs(jobs: &[Job]) -> usize {
    kinds(jobs)
        .into_iter()
        .filter(|kind| *kind == "repair")
        .count()
}

#[test]
fn a_failed_proof_gets_one_repair_and_a_repair_that_checks_turns_green() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"]));
    let jobs = run(&mut mls, answer);
    assert_eq!(
        kinds(&jobs),
        [
            "classify",
            "classify",
            "formalize",
            "check",
            "formalize",
            "check",
            "repair",
            "check",
        ]
    );
    let proof = id_of(&mls, 1);
    let Job::Repair {
        block,
        failed,
        error,
        ..
    } = &jobs[6]
    else {
        panic!("the failed proof is repaired")
    };
    assert_eq!(*block, proof);
    assert!(failed.contains("-- FALSE"), "{failed}");
    assert_eq!(error, "1:0: error");
    let Job::Check { lean, base, .. } = &jobs[7] else {
        panic!("the repaired proof is checked")
    };
    assert!(!lean.contains("FALSE"), "{lean}");
    assert_eq!(
        *base,
        100 + version_of(&mls, id_of(&mls, 0)),
        "the repair is checked after the statement, as the proof was"
    );
    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn a_proof_that_still_fails_after_its_repair_is_red_and_is_not_repaired_again() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎"]));
    let jobs = run(&mut mls, answer);
    assert_eq!(repairs(&jobs), 1);
    assert_eq!(kinds(&jobs).last(), Some(&"check"));
    let proof = id_of(&mls, 1);
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
    let details = item(&mls, proof);
    assert_eq!(details.error.as_deref(), Some("1:0: error"));
    assert!(mls.next_job().is_none());
}

#[test]
fn a_proof_is_grey_while_its_repair_is_pending() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"]));
    let job = run_until(&mut mls, |job| matches!(job, Job::Repair { .. }));
    assert!(matches!(job, Job::Repair { .. }));
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Checking)]);
    assert!(mls.next_job().is_none(), "one job is in flight at a time");
}

#[test]
fn a_failed_statement_or_definition_is_not_repaired() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Def d FALSE.", "Lemma t FALSE."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(repairs(&jobs), 0, "{:?}", kinds(&jobs));
    assert_eq!(statuses(&mls), [(0, Status::Failed), (1, Status::Failed)]);
}

#[test]
fn a_proof_whose_check_times_out_is_not_repaired() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Trivially. ∎"]));
    let proof = id_of(&mls, 1);
    let jobs = run(&mut mls, times_out(proof));
    assert_eq!(repairs(&jobs), 0);
    let details = item(&mls, proof);
    assert_eq!(details.error.as_deref(), Some(TIMED_OUT));
}

#[test]
fn a_repair_reply_for_an_edited_proof_is_rejected() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"]));
    let job = run_until(&mut mls, |job| matches!(job, Job::Repair { .. }));
    mls.sync(&paras(&[
        "Lemma t.",
        "Proof. FALSE claim FIXABLE, edited. ∎",
    ]));
    assert!(
        !mls.apply(answer(&job)),
        "a repair of the old proof is stale"
    );
}

#[test]
fn a_repair_reply_is_rejected_once_its_statement_is_edited() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"]));
    let job = run_until(&mut mls, |job| matches!(job, Job::Repair { .. }));
    mls.sync(&paras(&["Lemma u.", "Proof. FALSE claim FIXABLE. ∎"]));
    assert!(
        !mls.apply(answer(&job)),
        "a repair against the old statement is stale"
    );
}

#[test]
fn a_repair_reply_is_rejected_when_the_context_above_its_proof_changes() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"]));
    let job = run_until(&mut mls, |job| matches!(job, Job::Repair { .. }));
    let proof = id_of(&mls, 1);
    let version = version_of(&mls, proof);
    mls.sync(&paras(&[
        "Def d.",
        "Lemma t.",
        "Proof. FALSE claim FIXABLE. ∎",
    ]));
    assert_eq!(
        version_of(&mls, proof),
        version,
        "the proof itself is unchanged"
    );
    assert!(
        !mls.apply(answer(&job)),
        "a repair checked above a different context is stale"
    );
}

#[test]
fn a_repaired_proof_that_relies_on_an_unproved_statement_is_amber() {
    let mut mls = Mls::new();
    mls.sync(&paras(&[
        "Lemma t.",
        "Lemma u USES.",
        "Proof. FALSE claim FIXABLE USES. ∎",
    ]));
    let jobs = run(&mut mls, answer);
    assert_eq!(repairs(&jobs), 1);
    let proof = id_of(&mls, 2);
    assert_eq!(
        statuses(&mls),
        [(0, Status::Sorry), (1, Status::Sorry), (2, Status::Sorry)]
    );
    let note = format!("Relies on `t{}`, which is not proved yet.", id_of(&mls, 0));
    assert_eq!(item(&mls, proof).note, Some(note));
}

#[test]
fn a_repaired_proof_is_restored_from_the_twin_and_checked_without_a_repair() {
    let lines = ["Lemma t.", "Proof. FALSE claim FIXABLE. ∎"];
    let mut mls = Mls::new();
    mls.sync(&paras(&lines));
    run(&mut mls, answer);
    let twin = mls.twin();
    assert!(!twin.contains("-- FALSE"), "{twin}");

    let mut restored = Mls::new();
    restored.restore(&twin);
    restored.sync(&paras(&lines));
    let jobs = run(&mut restored, answer);
    assert_eq!(kinds(&jobs), ["check", "check"]);
    assert_eq!(
        statuses(&restored),
        [(0, Status::Verified), (1, Status::Verified)]
    );
}

#[test]
fn a_failed_repair_is_not_asked_again_after_an_edit_below() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎"]));
    run(&mut mls, answer);
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎", "Hello."]));
    let jobs = run(&mut mls, answer);
    assert_eq!(kinds(&jobs), ["classify"]);
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
}

#[test]
fn a_timeout_that_clears_a_failed_proof_does_not_repeat_its_repair() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎", "Lemma v."]));
    let slow = id_of(&mls, 2);
    let jobs = run(&mut mls, times_out(slow));
    assert_eq!(repairs(&jobs), 1);
    let proof = id_of(&mls, 1);
    assert_eq!(item(&mls, proof).error, Some("1:0: error".to_string()));
}

/// The mock worker, except that every repair is rejected with `failure`.
fn repair_fails(failure: Failure) -> impl Fn(&Job) -> Reply {
    move |job| match job {
        Job::Repair { .. } => Reply::Repaired {
            job: job.clone(),
            result: Err(failure.clone()),
        },
        _ => answer(job),
    }
}

#[test]
fn a_rejected_repair_is_the_last_diagnostic_and_is_not_asked_again() {
    let rejection = "The Lean uses `native_decide`, which is not allowed.";
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎"]));
    let jobs = run(
        &mut mls,
        repair_fails(Failure {
            message: rejection.to_string(),
            service: false,
        }),
    );
    assert_eq!(repairs(&jobs), 1);
    assert_eq!(kinds(&jobs).last(), Some(&"repair"));
    assert!(mls.next_job().is_none());
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
    let details = item(&mls, id_of(&mls, 1));
    assert_eq!(details.error.as_deref(), Some(rejection));
    assert!(!details.service);
}

#[test]
fn a_service_failure_during_repair_halts_with_its_own_message_until_a_retry() {
    let outage = "OpenRouter is unreachable: no route";
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. FALSE claim. ∎"]));
    let jobs = run(
        &mut mls,
        repair_fails(Failure {
            message: outage.to_string(),
            service: true,
        }),
    );
    assert_eq!(repairs(&jobs), 1);
    assert!(mls.next_job().is_none(), "a service failure halts the walk");
    let details = item(&mls, id_of(&mls, 1));
    assert_eq!(details.error.as_deref(), Some(outage));
    assert!(details.service);

    mls.retry();
    let jobs = run(&mut mls, answer);
    assert_eq!(repairs(&jobs), 1, "a retry asks for the repair again");
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
    let details = item(&mls, id_of(&mls, 1));
    assert_eq!(details.error.as_deref(), Some("1:0: error"));
}
