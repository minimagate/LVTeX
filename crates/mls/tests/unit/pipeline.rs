use super::*;
use crate::{Checked, Failure, Formal, Job, Label, Outcome, Reply, Status, Tag};

const MISSING_KEY: &str = "No OpenRouter key";
const REPL_DOWN: &str = "the Lean REPL is not running";

/// The mock model's tags. "Def" is a definition, "Lemma" and "Then" a statement, "Proof" starts a
/// proof, a lowercase line continues an open proof, and anything else is prose. A "Then" line
/// and a "+" line continue the paragraph above.
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
    }
}

/// The mock worker. Lean is deterministic: a statement ends in `sorry`, a proof replaces that
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

    let Job::Check { block, base, .. } = &jobs[5] else {
        panic!("the proof is checked")
    };
    assert_eq!(*block, proof);
    assert_eq!(*base, 0, "a proof is checked on its statement's base");

    assert_eq!(
        statuses(&mls),
        [(0, Status::Verified), (1, Status::Verified)]
    );
    let file = mls.twin();
    assert!(
        file.contains(&format!(
            "\n-- mathstral:block {proof} proof\ntheorem t{statement} : True := by trivial\n"
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
    let details = mls.details(proof).expect("the proof has a dot");
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
    let proof = id_of(&mls, 1);

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
    let proof = id_of(&mls, 1);

    assert!(jobs.iter().all(|job| !matches!(
        job,
        Job::Formalize { block, .. } | Job::Check { block, .. } if *block == proof
    )));
    assert_eq!(statuses(&mls), [(0, Status::Verified), (1, Status::Failed)]);
    let details = mls.details(proof).expect("the proof has a dot");
    assert_eq!(
        details.error.as_deref(),
        Some("There is no statement directly above this proof.")
    );
}

#[test]
fn the_last_skip_paragraph_above_a_block_is_its_prose() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Just words.", "Def d."]));
    let jobs = run(&mut mls, answer);
    let prose = jobs.iter().find_map(|job| match job {
        Job::Formalize { prose, .. } => Some(prose.clone()),
        _ => None,
    });
    assert_eq!(prose, Some(Some("Just words.".to_string())));
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
        Some(Details {
            error: Some(MISSING_KEY.to_string()),
            service: true,
            ..Details::default()
        })
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
fn editing_a_statement_reformalizes_its_proof_against_the_statement_base() {
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
        base: a_base,
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
    assert_eq!(base, a_base, "the proof is checked on the statement's base");
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
    assert_eq!(
        mls.details(a).and_then(|details| details.error),
        Some(TIMED_OUT.to_string())
    );
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
        second.details(id_of(&second, 0)).and_then(|d| d.lean),
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
        second.details(id_of(&second, 0)).and_then(|d| d.error),
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
        second.details(id_of(&second, 0)).and_then(|d| d.error),
        Some("The Lean declares an axiom.".to_string())
    );
    assert_eq!(
        second.details(id_of(&second, 1)).and_then(|d| d.error),
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
    assert!(twin.contains("\"version\":2"), "{twin}");

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
            twin.replace("\"version\":2", "\"version\":1"),
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
fn a_block_takes_the_whole_prose_unit_above_it_as_its_prose() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Hello there.", "+ And more.", "Lemma t."]));
    let jobs = run(&mut mls, answer);
    let prose: Vec<Option<String>> = jobs
        .iter()
        .filter_map(|job| match job {
            Job::Formalize { prose, .. } => Some(prose.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(prose, [Some("Hello there.\n\n+ And more.".to_string())]);
}

#[test]
fn the_twin_remembers_what_jev_said_about_joining() {
    let lines = ["Let x be real.", "Then x = x."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);

    let twin = first.twin();
    assert!(twin.contains("\"version\":2"), "{twin}");
    assert!(twin.contains("\"joins\":true"), "{twin}");
    assert!(twin.contains("\"previous\":\"Let x be real.\""), "{twin}");

    let mut second = restored(&twin, &lines);
    let jobs = run(&mut second, answer);
    assert_eq!(kinds(&jobs), ["check"]);
    assert_eq!(statuses(&second), [(0, Status::Sorry)]);
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
    assert_eq!(
        mls.details(lemma).map(|details| details.uses),
        Some(vec![format!("d{def}")])
    );
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
    assert_eq!(
        mls.details(id_of(&mls, 2)).map(|details| details.uses),
        Some(vec![format!("d{def}")])
    );
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
        mls.details(lemma).and_then(|details| details.note),
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
    let details = mls.details(proof).expect("the proof has a dot");
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
    assert_eq!(*base, 0, "the repair is checked on the statement's base");
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
    let details = mls.details(proof).expect("the proof has a dot");
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
    let details = mls.details(proof).expect("the proof has a dot");
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
    assert_eq!(
        mls.details(proof).and_then(|details| details.note),
        Some(note)
    );
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
    assert_eq!(
        mls.details(proof).and_then(|details| details.error),
        Some("1:0: error".to_string())
    );
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
    let details = mls.details(id_of(&mls, 1)).expect("the proof has a dot");
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
    let details = mls.details(id_of(&mls, 1)).expect("the proof has a dot");
    assert_eq!(details.error.as_deref(), Some(outage));
    assert!(details.service);

    mls.retry();
    let jobs = run(&mut mls, answer);
    assert_eq!(repairs(&jobs), 1, "a retry asks for the repair again");
    assert_eq!(statuses(&mls), [(0, Status::Sorry), (1, Status::Failed)]);
    let details = mls.details(id_of(&mls, 1)).expect("the proof has a dot");
    assert_eq!(details.error.as_deref(), Some("1:0: error"));
}
