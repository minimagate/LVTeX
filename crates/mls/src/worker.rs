//! The background thread that runs MLS jobs: model requests and Lean checks, one at a time.

use crate::openrouter::{
    self, CHAT_URL, DECISIONS_URL, classify_request, formalize_request, items_request,
    parse_formal, parse_items, parse_split, parse_tag, split_request,
};
use crate::repl::Repl;
use crate::{
    Checked, Failure, Formal, Job, Label, Outcome, Reply, Segment, SplitContext, Tag, rules,
};
use serde_json::Value;
use std::{
    fmt,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, mpsc},
    thread,
};

/// Runs MLS jobs one at a time on its own thread, in the order submitted.
#[derive(Debug)]
pub struct Worker {
    tx: mpsc::Sender<Job>,
    pub rx: mpsc::Receiver<Reply>,
    key: Key,
}

/// The OpenRouter key that model requests use. The worker's request and its owner share it, and
/// `Debug` never shows it.
#[derive(Clone, Default)]
struct Key(Arc<Mutex<Option<String>>>);

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(..)")
    }
}

impl Worker {
    /// `project` is the Lake project with Mathlib and the REPL; `wake` is called after each reply.
    /// Model requests use the key from `set_key`, which is unset until one is given.
    pub fn new(project: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let key = Key::default();
        let slot = key.clone();
        let request = move |url: &str, body: &Value| {
            // Copied out, so the lock is not held while the request runs.
            let key = slot
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            openrouter::post(url, body, key.as_deref())
        };
        Self {
            key,
            ..Self::with_request(project, wake, request)
        }
    }

    /// Sets the OpenRouter key that the next model request uses. `None` or a blank key means no key,
    /// and a request then fails with a message saying so. A request already in flight keeps the key
    /// it started with.
    pub fn set_key(&self, key: Option<String>) {
        let key = key.filter(|key| !key.trim().is_empty());
        *self.key.0.lock().unwrap_or_else(PoisonError::into_inner) = key;
    }

    fn with_request(
        project: PathBuf,
        wake: impl Fn() + Send + 'static,
        mut request: impl FnMut(&str, &Value) -> Result<Value, Failure> + Send + 'static,
    ) -> Self {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (replies, rx) = mpsc::channel::<Reply>();
        thread::Builder::new()
            .name("lvtex-mls".into())
            .spawn(move || {
                let mut repl: Option<Repl> = None;
                for job in jobs {
                    let reply = match panic::catch_unwind(AssertUnwindSafe(|| {
                        run(&job, &mut repl, &project, &mut request)
                    })) {
                        Ok(reply) => reply,
                        Err(_) => {
                            // A panic may leave the REPL mid-command, so the next check starts a fresh one.
                            repl = None;
                            failure_reply(
                                &job,
                                Failure {
                                    message: "MLS stopped unexpectedly.".into(),
                                    service: true,
                                },
                            )
                        }
                    };
                    let _ = replies.send(reply);
                    wake();
                }
            })
            .expect("MLS worker");
        Self {
            tx,
            rx,
            key: Key::default(),
        }
    }

    /// Queues a job. A send error means the worker has stopped, so the job is dropped.
    pub fn submit(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}

/// The most formalization requests one block gets in one job. A formalization that the rules reject,
/// that Lean does not accept, or whose reply is malformed is asked again with the reason, up to this
/// many in all.
const ATTEMPTS: usize = 3;

/// The formal of a formalization job and the check of that formal (see `Reply::Formalized`).
type Formalized = (
    Result<Formal, Failure>,
    Option<(u64, Result<Checked, Failure>)>,
);

/// Computes the reply to one job. A failed request or Lean check becomes a failed reply.
fn run(
    job: &Job,
    repl: &mut Option<Repl>,
    project: &Path,
    request: &mut impl FnMut(&str, &Value) -> Result<Value, Failure>,
) -> Reply {
    match job {
        Job::Classify {
            paragraph,
            previous,
            text,
            proof_open,
        } => {
            let result = request(
                DECISIONS_URL,
                &items_request(previous.as_deref(), text, *proof_open),
            )
            .and_then(|response| parse_items(&response))
            .and_then(|several| {
                if several {
                    Ok(Tag {
                        label: Label::Skip,
                        joins: false,
                        split: true,
                    })
                } else {
                    request(
                        DECISIONS_URL,
                        &classify_request(previous.as_deref(), text, *proof_open),
                    )
                    .and_then(|response| parse_tag(&response))
                }
            });
            Reply::Classified {
                paragraph: *paragraph,
                previous: previous.clone(),
                text: text.clone(),
                proof_open: *proof_open,
                result,
            }
        }
        Job::Split {
            paragraph,
            text,
            context,
        } => Reply::Split {
            paragraph: *paragraph,
            text: text.clone(),
            context: context.clone(),
            result: split(request, text, context),
        },
        Job::Formalize {
            block,
            version,
            label,
            text,
            above,
            lean_above,
            names_above,
            statement,
            base,
        } => {
            let statement_lean = statement.as_ref().map(|(_, lean)| lean.as_str());
            let (result, check) = formalize(
                request,
                &mut |lean, on| check(repl, project, lean, on),
                |retry| {
                    formalize_request(
                        *label,
                        text,
                        above.as_deref(),
                        lean_above,
                        statement_lean,
                        retry,
                    )
                },
                *label,
                names_above,
                statement_lean,
                *base,
            );
            Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: statement.as_ref().map(|(version, _)| *version),
                result,
                check,
            }
        }
        Job::Check {
            block,
            version,
            lean,
            base,
        } => Reply::Checked {
            block: *block,
            version: *version,
            base: *base,
            result: check(repl, project, lean, *base),
        },
    }
}

/// Formalizes one block. Each attempt sends a request, applies the rules and checks the Lean on
/// `base`. A rejected or unaccepted formalization is asked again with its Lean and the reason, and
/// so is a reply that was cut off or is not valid JSON, with its reason and no Lean. Up to
/// `ATTEMPTS` requests in all; the last attempt is the result. A service failure, or a timed-out
/// check, stops at once. `body` builds the request from the retry, if any.
fn formalize(
    request: &mut impl FnMut(&str, &Value) -> Result<Value, Failure>,
    check: &mut impl FnMut(&str, u64) -> Result<Checked, Failure>,
    body: impl Fn(Option<(&str, &str)>) -> Value,
    label: Label,
    names_above: &[String],
    statement: Option<&str>,
    base: u64,
) -> Formalized {
    let mut retry: Option<(String, String)> = None;
    let mut last = None;
    for _ in 0..ATTEMPTS {
        let request_body = body(
            retry
                .as_ref()
                .map(|(lean, reason)| (lean.as_str(), reason.as_str())),
        );
        let mut formal = match request(CHAT_URL, &request_body).and_then(|r| parse_formal(&r)) {
            Ok(formal) => formal,
            Err(failure) if failure.service => return (Err(failure), None),
            Err(failure) => {
                retry = Some((String::new(), failure.message.clone()));
                last = Some((Err(failure), None));
                continue;
            }
        };
        if let Err(message) = rules::reject(label, &formal, names_above, statement) {
            retry = Some((formal.lean.clone(), message.clone()));
            last = Some((
                Err(Failure {
                    message,
                    service: false,
                }),
                None,
            ));
            continue;
        }
        formal.uses = rules::uses(&formal.lean, names_above);
        if label == Label::Definition && rules::is_blank(&formal.lean) {
            return (Ok(formal), None);
        }
        let lean = rules::checked_lean(label, &formal.lean, statement);
        let checked = match check(&lean, base) {
            Ok(checked) => checked,
            Err(failure) => return (Ok(formal), Some((base, Err(failure)))),
        };
        match unaccepted(label, &checked.outcome) {
            None => return (Ok(formal), Some((base, Ok(checked)))),
            Some(reason) => {
                retry = Some((formal.lean.clone(), reason));
                last = Some((Ok(formal), Some((base, Ok(checked)))));
            }
        }
    }
    // `ATTEMPTS` is at least one, so the loop ran and set `last`.
    last.expect("the formalization loop makes at least one attempt")
}

/// Cuts one paragraph into its items. A reply whose pieces are not the paragraph in order, or that
/// is otherwise invalid, is asked again once with its reason. A second invalid reply is the result.
/// A service failure is returned at once, without a second request.
fn split(
    request: &mut impl FnMut(&str, &Value) -> Result<Value, Failure>,
    text: &str,
    context: &SplitContext,
) -> Result<Vec<Segment>, Failure> {
    let mut attempt = |retry: Option<&str>| {
        request(CHAT_URL, &split_request(text, context, retry))
            .and_then(|response| parse_split(&response, text, context))
    };
    match attempt(None) {
        Err(failure) if !failure.service => attempt(Some(failure.message.as_str())),
        result => result,
    }
}

/// Why Lean's outcome does not accept a formalization of `label`, or None when it does. A
/// statement may keep its `sorry`; a definition or a proof may not.
fn unaccepted(label: Label, outcome: &Outcome) -> Option<String> {
    match (label, outcome) {
        (_, Outcome::Verified) | (Label::Statement, Outcome::Sorry) => None,
        (Label::Definition, Outcome::Sorry) => Some("The definition uses sorry.".to_string()),
        (_, Outcome::Sorry) => Some("The proof uses sorry.".to_string()),
        (_, Outcome::Failed(error)) => Some(error.clone()),
    }
}

/// Checks `lean` on the REPL, starting the REPL on first use. Any failed check drops the REPL: a
/// service failure means it is gone, and a timed-out one must be killed, since its answer may
/// still arrive. The next check then starts a fresh one.
fn check(
    repl: &mut Option<Repl>,
    project: &Path,
    lean: &str,
    base: u64,
) -> Result<Checked, Failure> {
    if repl.is_none() {
        *repl = Some(Repl::start(project)?);
    }
    let live = repl.as_mut().expect("the REPL was started above");
    let result = live.check(lean, base);
    if result.is_err() {
        *repl = None;
    }
    result
}

/// The reply for a job that did not finish, echoing the fields it was computed from.
fn failure_reply(job: &Job, failure: Failure) -> Reply {
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
            result: Err(failure),
        },
        Job::Split {
            paragraph,
            text,
            context,
        } => Reply::Split {
            paragraph: *paragraph,
            text: text.clone(),
            context: context.clone(),
            result: Err(failure),
        },
        Job::Formalize {
            block,
            version,
            statement,
            ..
        } => Reply::Formalized {
            block: *block,
            version: *version,
            statement_version: statement.as_ref().map(|(version, _)| *version),
            result: Err(failure),
            check: None,
        },
        Job::Check {
            block,
            version,
            base,
            ..
        } => Reply::Checked {
            block: *block,
            version: *version,
            base: *base,
            result: Err(failure),
        },
    }
}

#[cfg(test)]
#[path = "../tests/unit/worker.rs"]
mod tests;
