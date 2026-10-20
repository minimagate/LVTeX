//! The per-document state: paragraphs, blocks, versions, the next job and the dots.

use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::rules::declared_names;
use crate::{Checked, Failure, Formal, Job, Label, Outcome, Reply, Status};

/// One non-blank line of the document. `open` while the caret is in it or a formula in it is unfinished.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub text: String,
    pub open: bool,
}

/// A status dot beside a paragraph (index into the last `sync` input).
#[derive(Clone, Debug, PartialEq)]
pub struct Dot {
    pub paragraph: usize,
    pub key: u64,
    pub status: Status,
    error: Option<String>,
    service: bool,
}

/// What the panel shows for a dot.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Details {
    pub assumptions: Vec<String>,
    pub lean: Option<String>,
    pub error: Option<String>,
    pub service: bool,
}

#[derive(Default, Debug)]
pub struct Mls {
    paras: Vec<Para>,
    blocks: Vec<Block>,
    /// Records of blocks that are not in `blocks`, by id. An edit above them then reuses their
    /// formalization and check instead of redoing them. `plan` and `apply` never read them.
    parked: HashMap<u64, Block>,
    frontier: Frontier,
    busy: bool,
    repl_lost: bool,
}

/// A fresh id for a paragraph or a version. Global, so a new `Mls` never reuses an id still in flight.
static NEXT: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
struct Para {
    id: u64,
    text: String,
    settled: Option<String>,
    label: Option<Labelled>,
}

#[derive(Debug)]
struct Labelled {
    text: String,
    proof_open: bool,
    result: Result<Label, Failure>,
}

#[derive(Debug)]
struct Block {
    /// Id of its first paragraph.
    id: u64,
    /// A new value from `next()` whenever its text or label changes.
    version: u64,
    /// Index of its first paragraph in `paras`.
    first: usize,
    /// The settled texts of its paragraphs, joined by a blank line.
    text: String,
    /// Definition, Statement or Proof; never Skip.
    label: Label,
    /// False while a proof is still open at the end of the document or where segmentation stops.
    closed: bool,
    /// The settled text of the last settled paragraph before `first`, when that paragraph is
    /// labelled Skip. Unsettled paragraphs are ignored.
    prose: Option<String>,
    /// Proofs: the id of the statement block paired with it.
    statement: Option<u64>,
    formal: Option<Result<Formal, Failure>>,
    /// Proofs: the statement version `formal` was made against.
    statement_version: Option<u64>,
    /// The base environment it was checked on, and the result.
    check: Option<(u64, Result<Checked, Failure>)>,
}

/// Where segmentation stops. A `Classify` frontier is the first paragraph that needs a label; its
/// job runs before any other. `Failed` is a paragraph whose label failed, with no such paragraph
/// above it.
#[derive(Default, Debug)]
enum Frontier {
    #[default]
    Done,
    Classify {
        index: usize,
        proof_open: bool,
    },
    Failed {
        index: usize,
    },
}

struct Plan {
    job: Option<Job>,
    dots: Vec<Dot>,
    lean: Vec<(u64, Label, String)>,
}

/// A statement whose Lean compiled, for the proof that follows it.
#[derive(Clone)]
struct Stated {
    base: u64,
    version: u64,
    lean: String,
    /// Index of its dot in the plan being built.
    dot: usize,
}

fn dot(paragraph: usize, key: u64, status: Status) -> Dot {
    Dot {
        paragraph,
        key,
        status,
        error: None,
        service: false,
    }
}

fn failed(paragraph: usize, key: u64, error: &str, service: bool) -> Dot {
    Dot {
        paragraph,
        key,
        status: Status::Failed,
        error: Some(error.to_string()),
        service,
    }
}

fn settled_text(para: &Para) -> &str {
    para.settled
        .as_deref()
        .expect("the pipeline only stops at settled paragraphs")
}

impl Mls {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the document: its paragraphs, blocks, parked records, frontier and lost REPL. A job
    /// for the old document may still be in flight, so `busy` stays set until its reply is applied.
    pub fn reset(&mut self) {
        *self = Self {
            busy: self.busy,
            ..Self::default()
        };
    }

    /// Takes the paragraphs after an edit or a caret move. A paragraph keeps its identity while
    /// its text is unchanged in the common prefix and suffix. A closed paragraph takes its new
    /// text as settled; an open one keeps the text it had settled before.
    pub fn sync(&mut self, paragraphs: &[Paragraph]) {
        let old = take(&mut self.paras);
        let prefix = old
            .iter()
            .zip(paragraphs)
            .take_while(|(o, n)| o.text == n.text)
            .count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(paragraphs[prefix..].iter().rev())
            .take_while(|(o, n)| o.text == n.text)
            .count();
        let before = old.iter().filter(|p| p.settled.is_some()).count();
        let middle_old = old.len() - prefix - suffix;

        let mut old = old.into_iter();
        let mut next_paras: Vec<Para> = old.by_ref().take(prefix).collect();
        let middle: Vec<Para> = old.by_ref().take(middle_old).collect();
        let suffix_paras: Vec<Para> = old.collect();
        let mut middle = middle.into_iter();
        for new in &paragraphs[prefix..paragraphs.len() - suffix] {
            next_paras.push(match middle.next() {
                Some(para) => Para {
                    text: new.text.clone(),
                    ..para
                },
                None => Para {
                    id: next(),
                    text: new.text.clone(),
                    settled: None,
                    label: None,
                },
            });
        }
        let dropped_settled = middle.any(|para| para.settled.is_some());
        next_paras.extend(suffix_paras);

        let mut changed = dropped_settled;
        for (para, new) in next_paras.iter_mut().zip(paragraphs) {
            if !new.open && para.settled.as_deref() != Some(new.text.as_str()) {
                para.settled = Some(new.text.clone());
                changed = true;
            }
        }
        let after = next_paras.iter().filter(|p| p.settled.is_some()).count();
        changed |= after != before;
        self.paras = next_paras;

        if changed {
            self.retry_services();
        }
        self.segment();
    }

    /// Clears what a service failure left behind, so the next changed document tries again. A
    /// lost REPL clears every check, since the environments they were made on are gone.
    fn retry_services(&mut self) {
        for para in &mut self.paras {
            if matches!(&para.label, Some(Labelled { result: Err(f), .. }) if f.service) {
                para.label = None;
            }
        }
        let repl_lost = take(&mut self.repl_lost);
        for block in self.blocks.iter_mut().chain(self.parked.values_mut()) {
            if matches!(&block.formal, Some(Err(f)) if f.service) {
                block.formal = None;
            }
            if repl_lost || matches!(&block.check, Some((_, Err(f))) if f.service) {
                block.check = None;
            }
        }
    }

    /// Rebuilds the blocks and the frontier from the paragraphs. A block reuses the record with
    /// its id, from `blocks` or else `parked`, when its text and label are unchanged.
    fn segment(&mut self) {
        let paras = &self.paras;
        let mut old: HashMap<u64, Block> = take(&mut self.blocks)
            .into_iter()
            .map(|block| (block.id, block))
            .collect();
        let mut parked = take(&mut self.parked);
        let mut new: Vec<Block> = Vec::new();
        let mut open_proof: Option<usize> = None;
        let mut prev: Option<(Label, &str)> = None;
        let mut frontier = Frontier::Done;
        for (i, para) in paras.iter().enumerate() {
            let Some(text) = para.settled.as_deref() else {
                continue;
            };
            let proof_open = open_proof.is_some();
            let label = match &para.label {
                Some(l) if l.text.as_str() == text && l.proof_open == proof_open => match &l.result
                {
                    Ok(label) => *label,
                    Err(_) => {
                        // A Classify frontier above stays: its job runs first.
                        if matches!(frontier, Frontier::Done) {
                            frontier = Frontier::Failed { index: i };
                        }
                        break;
                    }
                },
                _ => {
                    // The first paragraph that needs a label is the frontier. Until it has a label
                    // it counts as Skip, so the blocks below keep their dots. A second one stops.
                    if !matches!(frontier, Frontier::Done) {
                        break;
                    }
                    frontier = Frontier::Classify {
                        index: i,
                        proof_open,
                    };
                    open_proof = None;
                    prev = None;
                    continue;
                }
            };
            match (label, open_proof) {
                (Label::Proof, Some(b)) => {
                    let block = &mut new[b];
                    block.text.push_str("\n\n");
                    block.text.push_str(text);
                }
                (Label::Skip, _) => open_proof = None,
                (Label::Definition | Label::Statement | Label::Proof, _) => {
                    open_proof = None;
                    let statement = match label {
                        Label::Proof => new
                            .last()
                            .filter(|b| b.label == Label::Statement)
                            .map(|b| b.id),
                        Label::Definition | Label::Statement | Label::Skip => None,
                    };
                    let prose = match prev {
                        Some((Label::Skip, skip)) => Some(skip.to_string()),
                        Some((Label::Definition | Label::Statement | Label::Proof, _)) | None => {
                            None
                        }
                    };
                    new.push(Block {
                        id: para.id,
                        version: 0,
                        first: i,
                        text: text.to_string(),
                        label,
                        closed: true,
                        prose,
                        statement,
                        formal: None,
                        statement_version: None,
                        check: None,
                    });
                    if label == Label::Proof {
                        open_proof = Some(new.len() - 1);
                    }
                }
            }
            if label == Label::Proof && text.trim_end().ends_with('∎') {
                open_proof = None;
            }
            prev = Some((label, text));
        }
        if let Some(b) = open_proof {
            new[b].closed = false;
        }

        let ids: HashSet<u64> = paras.iter().map(|para| para.id).collect();
        let mut unused: Vec<Block> = Vec::new();
        for block in &mut new {
            match old.remove(&block.id).or_else(|| parked.remove(&block.id)) {
                Some(prior) if prior.text == block.text && prior.label == block.label => {
                    block.version = prior.version;
                    block.formal = prior.formal;
                    block.statement_version = prior.statement_version;
                    block.check = prior.check;
                }
                prior => {
                    block.version = next();
                    unused.extend(prior);
                }
            }
        }

        let mut kept: HashMap<u64, Block> = parked
            .into_iter()
            .filter(|(id, _)| ids.contains(id))
            .collect();
        for block in old.into_values().chain(unused) {
            if ids.contains(&block.id) {
                kept.insert(block.id, block);
            }
        }
        self.parked = kept;
        self.blocks = new;
        self.frontier = frontier;
    }

    /// The next job for the worker, or None while one is in flight or nothing needs doing.
    pub fn next_job(&mut self) -> Option<Job> {
        if self.busy {
            return None;
        }
        let job = self.plan().job;
        if job.is_some() {
            self.busy = true;
        }
        job
    }

    /// Applies a worker's reply. Returns false when the reply is stale: the paragraph or block it
    /// was computed for has changed since. A failed check still clears the checks that a lost REPL
    /// invalidates, even when its own reply is stale.
    pub fn apply(&mut self, reply: Reply) -> bool {
        self.busy = false;
        match reply {
            Reply::Classified {
                paragraph,
                text,
                proof_open,
                result,
            } => {
                let Some(para) = self.paras.iter_mut().find(|para| {
                    para.id == paragraph && para.settled.as_deref() == Some(text.as_str())
                }) else {
                    return false;
                };
                para.label = Some(Labelled {
                    text,
                    proof_open,
                    result,
                });
                self.segment();
                true
            }
            Reply::Formalized {
                block,
                version,
                statement_version,
                result,
            } => {
                let Some(b) = self
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == block && b.version == version)
                else {
                    return false;
                };
                b.formal = Some(result);
                b.statement_version = statement_version;
                b.check = None;
                true
            }
            Reply::Checked {
                block,
                version,
                base,
                result,
            } => {
                match &result {
                    // The REPL is gone. The next changed document retries (see `retry_services`).
                    Err(failure) if failure.service => self.repl_lost = true,
                    // A timeout kills the REPL, so every environment that a check was made on is
                    // gone. This block's own check is set below, when its reply is current.
                    Err(_) => {
                        for b in self.blocks.iter_mut().chain(self.parked.values_mut()) {
                            b.check = None;
                        }
                    }
                    Ok(_) => {}
                }
                let Some(b) = self
                    .blocks
                    .iter_mut()
                    .find(|b| b.id == block && b.version == version)
                else {
                    return false;
                };
                b.check = Some((base, result));
                true
            }
        }
    }

    pub fn dots(&self) -> Vec<Dot> {
        self.plan().dots
    }

    /// The panel's details for the dot with `key`, or None when no dot has that key.
    pub fn details(&self, key: u64) -> Option<Details> {
        let found = self.plan().dots.into_iter().find(|d| d.key == key)?;
        let mut details = Details {
            error: found.error,
            service: found.service,
            ..Details::default()
        };
        if let Some(Block {
            formal: Some(Ok(formal)),
            ..
        }) = self.blocks.iter().find(|b| b.id == key)
        {
            details.assumptions = formal.assumptions.clone();
            details.lean = Some(formal.lean.clone());
        }
        Some(details)
    }

    /// The Lean of every block that checked, in document order, for saving.
    pub fn lean_file(&self) -> String {
        let mut file = String::from("import Mathlib\n");
        for (id, label, lean) in self.plan().lean {
            file.push_str(&format!(
                "\n-- mathstral:block {id} {}\n{}\n",
                label.name(),
                lean.trim_end()
            ));
        }
        file
    }

    /// Walks the closed blocks in document order and works out the next job, the dots and the
    /// Lean chain. A Classify job comes first: when the frontier is a paragraph that needs a
    /// label, it wins over a pending Formalize or Check of an earlier block, so the earlier
    /// block waits its turn behind the labelling that the frontier needs.
    fn plan(&self) -> Plan {
        let mut dots = Vec::new();
        let mut job = match self.frontier {
            Frontier::Classify { index, proof_open } => Some(Job::Classify {
                paragraph: self.paras[index].id,
                text: settled_text(&self.paras[index]).to_string(),
                proof_open,
            }),
            Frontier::Done | Frontier::Failed { .. } => None,
        };
        if let Frontier::Failed { index } = self.frontier {
            // The frontier is only Failed for a paragraph whose label is an error.
            let para = &self.paras[index];
            if let Some(Labelled {
                result: Err(failure),
                ..
            }) = &para.label
            {
                dots.push(failed(index, para.id, &failure.message, failure.service));
            }
        }

        let mut pending = false;
        let mut halted = false;
        let mut chain = 0u64;
        let mut names: Vec<String> = Vec::new();
        let mut lean: Vec<(u64, Label, String)> = Vec::new();
        let mut statements: HashMap<u64, Stated> = HashMap::new();
        for b in self.blocks.iter().take_while(|b| b.closed) {
            if halted {
                break;
            }
            if pending {
                dots.push(dot(b.first, b.id, Status::Checking));
                continue;
            }
            let stmt: Option<Stated> = match (b.label, b.statement) {
                (Label::Proof, None) => {
                    let error = "There is no statement directly above this proof.";
                    dots.push(failed(b.first, b.id, error, false));
                    continue;
                }
                (Label::Proof, Some(id)) => match statements.get(&id) {
                    Some(stated) => Some(stated.clone()),
                    None => {
                        let error = "The statement above has no valid Lean.";
                        dots.push(failed(b.first, b.id, error, false));
                        continue;
                    }
                },
                (Label::Definition | Label::Statement | Label::Skip, _) => None,
            };
            let base = stmt.as_ref().map_or(chain, |s| s.base);

            let stale_proof = stmt
                .as_ref()
                .is_some_and(|s| b.statement_version != Some(s.version));
            let formal = match &b.formal {
                Some(Ok(formal)) if !stale_proof => formal,
                Some(Err(failure)) if !stale_proof => {
                    dots.push(failed(b.first, b.id, &failure.message, failure.service));
                    halted |= failure.service;
                    continue;
                }
                _ => {
                    job.get_or_insert_with(|| Job::Formalize {
                        block: b.id,
                        version: b.version,
                        label: b.label,
                        text: b.text.clone(),
                        prose: b.prose.clone(),
                        lean_above: lean
                            .iter()
                            .map(|(id, _, lean)| (*id, lean.clone()))
                            .collect(),
                        names_above: names.clone(),
                        statement: stmt.as_ref().map(|s| (s.version, s.lean.clone())),
                    });
                    pending = true;
                    dots.push(dot(b.first, b.id, Status::Checking));
                    continue;
                }
            };

            match &b.check {
                // A timed-out check counts as current whatever its base. Re-checking it would
                // hang again, so it waits for the block's text to change.
                Some((checked_base, result))
                    if *checked_base == base || matches!(result, Err(f) if !f.service) =>
                {
                    match result {
                        Err(failure) => {
                            dots.push(failed(b.first, b.id, &failure.message, failure.service));
                            halted |= failure.service;
                        }
                        Ok(Checked { env, outcome }) => match (b.label, outcome) {
                            (
                                Label::Definition | Label::Statement | Label::Proof | Label::Skip,
                                Outcome::Failed(message),
                            ) => {
                                dots.push(failed(b.first, b.id, message, false));
                            }
                            (Label::Definition, Outcome::Sorry) => {
                                let error = "The definition uses sorry.";
                                dots.push(failed(b.first, b.id, error, false));
                            }
                            (Label::Proof, Outcome::Sorry) => {
                                let error = "The proof uses sorry.";
                                dots.push(failed(b.first, b.id, error, false));
                            }
                            (Label::Definition, Outcome::Verified) => {
                                dots.push(dot(b.first, b.id, Status::Verified));
                                chain = *env;
                                names.extend(declared_names(&formal.lean));
                                lean.push((b.id, Label::Definition, formal.lean.clone()));
                            }
                            (Label::Statement, _) => {
                                dots.push(dot(b.first, b.id, Status::Sorry));
                                chain = *env;
                                names.extend(declared_names(&formal.lean));
                                lean.push((b.id, Label::Statement, formal.lean.clone()));
                                statements.insert(
                                    b.id,
                                    Stated {
                                        base,
                                        version: b.version,
                                        lean: formal.lean.clone(),
                                        dot: dots.len() - 1,
                                    },
                                );
                            }
                            (Label::Proof, Outcome::Verified) => {
                                dots.push(dot(b.first, b.id, Status::Verified));
                                if let Some(statement) = &stmt {
                                    dots[statement.dot].status = Status::Verified;
                                }
                                chain = *env;
                                // The proof restates its statement, so it replaces the statement's Lean.
                                if let Some(entry) =
                                    lean.iter_mut().find(|(id, _, _)| Some(*id) == b.statement)
                                {
                                    *entry = (b.id, Label::Proof, formal.lean.clone());
                                }
                                for name in declared_names(&formal.lean) {
                                    if !names.contains(&name) {
                                        names.push(name);
                                    }
                                }
                            }
                            (Label::Skip, _) => {
                                dots.push(failed(b.first, b.id, "Unexpected label.", false));
                            }
                        },
                    }
                }
                _ => {
                    job.get_or_insert_with(|| Job::Check {
                        block: b.id,
                        version: b.version,
                        lean: formal.lean.clone(),
                        base,
                    });
                    pending = true;
                    dots.push(dot(b.first, b.id, Status::Checking));
                }
            }
        }
        Plan { job, dots, lean }
    }
}

#[cfg(test)]
#[path = "../tests/unit/pipeline.rs"]
mod tests;
