use super::*;
use crate::{Checked, Failure, Formal, Job, Label, Outcome, Reply, Status};

const MISSING_KEY: &str = "OPENROUTER_API_KEY is not set";
const REPL_DOWN: &str = "the Lean REPL is not running";

/// The mock model's labels: "Def" is a definition, "Lemma" a statement, "Proof" starts a proof,
/// a lowercase line continues an open proof, and anything else is prose.
fn label_for(text: &str, proof_open: bool) -> Label {
    if text.starts_with("Def") {
        Label::Definition
    } else if text.starts_with("Lemma") {
        Label::Statement
    } else if text.starts_with("Proof") || (proof_open && text.starts_with(char::is_lowercase)) {
        Label::Proof
    } else {
        Label::Skip
    }
}

/// The mock worker. Lean is deterministic: a statement ends in `sorry`, a proof replaces that
/// with `trivial`, and text containing FALSE produces Lean that the REPL rejects. A check's
/// environment is 100 plus the block version, so each check has a predictable environment.
fn answer(job: &Job) -> Reply {
    match job {
        Job::Classify {
            paragraph,
            text,
            proof_open,
        } => Reply::Classified {
            paragraph: *paragraph,
            text: text.clone(),
            proof_open: *proof_open,
            result: Ok(label_for(text, *proof_open)),
        },
        Job::Formalize {
            block,
            version,
            label,
            text,
            statement,
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
            Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: statement.as_ref().map(|(version, _)| *version),
                result: Ok(Formal {
                    lean,
                    lean_name: None,
                    assumptions: Vec::new(),
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

fn kinds(jobs: &[Job]) -> Vec<&'static str> {
    jobs.iter()
        .map(|job| match job {
            Job::Classify { .. } => "classify",
            Job::Formalize { .. } => "formalize",
            Job::Check { .. } => "check",
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
fn an_unfinished_proof_at_the_end_gets_no_dot_and_no_formalize() {
    let mut mls = Mls::new();
    mls.sync(&paras(&["Lemma t.", "Proof. Never ends."]));
    let jobs = run(&mut mls, answer);
    let proof = id_of(&mls, 1);

    assert_eq!(kinds(&jobs), ["classify", "classify", "formalize", "check"]);
    assert!(jobs.iter().all(|job| !matches!(
        job,
        Job::Formalize { block, .. } | Job::Check { block, .. } if *block == proof
    )));
    assert_eq!(statuses(&mls), [(0, Status::Sorry)]);
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

    assert_eq!(kinds(&jobs), ["classify", "formalize", "check", "check"]);
    assert!(matches!(
        &jobs[0],
        Job::Classify { paragraph, text, .. } if *paragraph == a && text == "Lemma a2."
    ));
    let Job::Check {
        block: checked_a,
        version: a_version,
        ..
    } = &jobs[2]
    else {
        panic!("the edited statement is checked")
    };
    assert_eq!(*checked_a, a);
    let Job::Check { block, base, .. } = &jobs[3] else {
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

#[test]
fn a_classification_service_failure_stops_at_its_paragraph_until_a_change() {
    fn service_down(job: &Job) -> Reply {
        match job {
            Job::Classify {
                paragraph,
                text,
                proof_open,
            } => Reply::Classified {
                paragraph: *paragraph,
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
    assert_eq!(
        kinds(&jobs),
        ["classify", "formalize", "check", "formalize", "check"]
    );

    let Job::Check {
        block: checked_a,
        version: a_version,
        base: a_base,
        ..
    } = &jobs[2]
    else {
        panic!("the edited statement is checked")
    };
    assert_eq!(*checked_a, a);
    let Job::Formalize {
        block, statement, ..
    } = &jobs[3]
    else {
        panic!("the proof is formalized again")
    };
    assert_eq!(*block, proof);
    assert_eq!(
        statement.as_ref().map(|(version, _)| *version),
        Some(*a_version),
        "the proof is formalized against the edited statement"
    );
    let Job::Check { block, base, .. } = &jobs[4] else {
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
    assert!(
        mls.next_job().is_none(),
        "no Lean above the statement changed"
    );
    assert_eq!(statuses(&mls), [(1, Status::Verified), (2, Status::Sorry)]);
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
                text,
                proof_open,
            } if text == "Hello." => Reply::Classified {
                paragraph: *paragraph,
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
fn an_edited_paragraph_is_the_only_one_classified_after_restore() {
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
    assert_eq!(classified, ["Def e."]);
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
fn a_missing_or_malformed_memory_restores_nothing() {
    let lines = ["Hello.", "Def d.", "Lemma a."];
    let mut first = Mls::new();
    first.sync(&paras(&lines));
    run(&mut first, answer);
    let twin = first.twin();
    assert!(twin.contains("\"version\":1"), "{twin}");

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
            "wrong version",
            twin.replace("\"version\":1", "\"version\":2"),
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
