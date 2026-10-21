//! The background thread that runs MLS jobs: model requests and Lean checks, one at a time.

use crate::openrouter::{
    self, CHAT_URL, DECISIONS_URL, classify_request, formalize_request, items_request,
    parse_formal, parse_items, parse_split, parse_tag, split_request,
};
use crate::repl::Repl;
use crate::{Checked, Failure, Formal, Job, Label, Reply, Tag, rules};
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
            result: request(CHAT_URL, &split_request(text, context))
                .and_then(|response| parse_split(&response, text, context)),
        },
        Job::Formalize {
            block,
            version,
            label,
            text,
            prose,
            lean_above,
            names_above,
            statement,
        } => {
            let statement_lean = statement.as_ref().map(|(_, lean)| lean.as_str());
            let body = formalize_request(
                *label,
                text,
                prose.as_deref(),
                lean_above,
                statement_lean,
                None,
            );
            let result = checked_formal(request, &body, *label, names_above, statement_lean);
            Reply::Formalized {
                block: *block,
                version: *version,
                statement_version: statement.as_ref().map(|(version, _)| *version),
                result,
            }
        }
        Job::Repair {
            text,
            prose,
            lean_above,
            names_above,
            statement,
            failed,
            error,
            ..
        } => {
            let body = formalize_request(
                Label::Proof,
                text,
                prose.as_deref(),
                lean_above,
                Some(&statement.1),
                Some((failed, error)),
            );
            Reply::Repaired {
                job: job.clone(),
                result: checked_formal(
                    request,
                    &body,
                    Label::Proof,
                    names_above,
                    Some(&statement.1),
                ),
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

/// Sends one formalization request and checks the reply with the rules. The formal carries the
/// names from above that its Lean uses.
fn checked_formal(
    request: &mut impl FnMut(&str, &Value) -> Result<Value, Failure>,
    body: &Value,
    label: Label,
    names_above: &[String],
    statement: Option<&str>,
) -> Result<Formal, Failure> {
    request(CHAT_URL, body)
        .and_then(|response| parse_formal(&response))
        .and_then(|mut formal| {
            rules::reject(label, &formal, names_above, statement)
                .map(|()| {
                    formal.uses = rules::uses(&formal.lean, names_above);
                    formal
                })
                .map_err(|message| Failure {
                    message,
                    service: false,
                })
        })
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
        Job::Repair { .. } => Reply::Repaired {
            job: job.clone(),
            result: Err(failure),
        },
    }
}

#[cfg(test)]
#[path = "../tests/unit/worker.rs"]
mod tests;
