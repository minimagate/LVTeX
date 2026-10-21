//! MLS: classify each block of a document, formalize it into Lean and check it with the Lean REPL.

mod openrouter;
mod pipeline;
mod repl;
mod rules;
mod worker;

pub use openrouter::key_is_valid;
pub use pipeline::{Details, Dot, Mls, Node, Paragraph};
pub use worker::Worker;

/// The decision model that labels each paragraph (OpenRouter id, pinned).
pub const JEV_MODEL: &str = "typesafe/jev-1.13";
/// The model that formalizes blocks into Lean (OpenRouter id).
pub const HAIKU_MODEL: &str = "anthropic/claude-haiku-5.5";

/// What a paragraph is, which decides how its Lean is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Label {
    Definition,
    Statement,
    Proof,
    Skip,
}

impl Label {
    pub const ALL: [Label; 4] = [
        Label::Definition,
        Label::Statement,
        Label::Proof,
        Label::Skip,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Label::Definition => "definition",
            Label::Statement => "statement",
            Label::Proof => "proof",
            Label::Skip => "skip",
        }
    }

    pub fn from_name(name: &str) -> Option<Label> {
        Label::ALL.into_iter().find(|label| label.name() == name)
    }
}

/// What Jev said about one paragraph: its label, and whether it continues the paragraph above as
/// part of the same definition, statement or proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag {
    pub label: Label,
    pub joins: bool,
}

/// The state shown beside a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Grey while checking.
    Checking,
    /// Amber: compiles with sorry.
    Sorry,
    /// Green: verified.
    Verified,
    /// Red: Lean error or rejected output.
    Failed,
}

/// The Lean for one block and its assumptions in plain language, written from the Lean.
#[derive(Clone, Debug, PartialEq)]
pub struct Formal {
    pub lean: String,
    pub lean_name: Option<String>,
    pub assumptions: Vec<String>,
    /// The names from the blocks above that the Lean uses (see `rules::uses`). The worker fills it
    /// in; it is empty in the model's answer.
    pub uses: Vec<String>,
}

/// A failure. A service failure (missing key, network, Lean setup) halts checking until the next edit
/// or `Mls::retry`.
#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub message: String,
    pub service: bool,
}

/// What Lean said about one block.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Verified,
    Sorry,
    Failed(String),
}

/// A Lean check: the REPL environment after the block, and its outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub env: u64,
    pub outcome: Outcome,
}

/// One unit of background work, produced by the pipeline and run by the worker.
#[derive(Clone, Debug)]
pub enum Job {
    Classify {
        paragraph: u64,
        /// The block or prose run directly above: every paragraph grouped into it so far, joined by
        /// a blank line. None for the first paragraph. Jev is asked whether `text` continues it.
        previous: Option<String>,
        text: String,
        proof_open: bool,
    },
    Formalize {
        block: u64,
        version: u64,
        label: Label,
        text: String,
        prose: Option<String>,
        lean_above: Vec<(u64, String)>,
        names_above: Vec<String>,
        /// For proofs: the paired statement's version and Lean.
        statement: Option<(u64, String)>,
    },
    Check {
        block: u64,
        version: u64,
        lean: String,
        /// The REPL environment to check on; 0 is the environment right after `import Mathlib`.
        base: u64,
    },
}

/// The worker's answer to one job, echoing what it was computed from.
#[derive(Clone, Debug)]
pub enum Reply {
    Classified {
        paragraph: u64,
        previous: Option<String>,
        text: String,
        proof_open: bool,
        result: Result<Tag, Failure>,
    },
    Formalized {
        block: u64,
        version: u64,
        statement_version: Option<u64>,
        result: Result<Formal, Failure>,
    },
    Checked {
        block: u64,
        version: u64,
        base: u64,
        result: Result<Checked, Failure>,
    },
}
