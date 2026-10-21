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
/// The model that formalizes blocks into Lean (OpenRouter id, pinned).
pub const FORMALIZER_MODEL: &str = "anthropic/claude-sonnet-5.5";

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

/// What Jev said about one paragraph after the independent-item gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tag {
    pub label: Label,
    pub joins: bool,
    /// Several items need labeled segmentation. When true, `label` and `joins` are ignored
    /// placeholders, never shown or used to build blocks.
    pub split: bool,
}

/// Context for labeled segmentation, echoed by the worker to reject stale answers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SplitContext {
    /// The preceding unit's role and complete text.
    pub previous: Option<(Label, String)>,
    /// The preceding statement, including when the preceding unit is its continuing proof.
    pub statement: Option<String>,
}

/// One authoritative item of a paragraph. IDs and proof targets are local to that paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub id: u64,
    pub label: Label,
    pub text: String,
    pub proves: Option<u64>,
    pub proves_previous: bool,
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
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Classify {
        /// The id of the paragraph being classified.
        paragraph: u64,
        /// The block or prose run directly above: every paragraph grouped into it so far, joined by
        /// a blank line. None for the first paragraph. Jev is asked whether `text` continues it.
        previous: Option<String>,
        text: String,
        proof_open: bool,
    },
    /// One labeled segmentation, also used once to recover an unpaired whole-paragraph proof.
    Split {
        paragraph: u64,
        text: String,
        context: SplitContext,
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
    /// Once per proof whose Lean failed to check: a repaired translation of the same proof, asked
    /// with the failed candidate and Lean's error. Every field is the context it is checked against,
    /// so a reply that no longer matches the plan is rejected.
    Repair {
        block: u64,
        version: u64,
        text: String,
        prose: Option<String>,
        lean_above: Vec<(u64, String)>,
        names_above: Vec<String>,
        /// The paired statement's version and Lean.
        statement: (u64, String),
        /// The REPL environment the statement's proof is checked on: the one the repair is checked on.
        base: u64,
        /// The Lean that failed, and the error Lean gave for it.
        failed: String,
        error: String,
    },
}

/// The worker's answer to one job, echoing what it was computed from.
#[derive(Clone, Debug)]
pub enum Reply {
    Classified {
        /// The id of the paragraph that was classified.
        paragraph: u64,
        previous: Option<String>,
        text: String,
        proof_open: bool,
        result: Result<Tag, Failure>,
    },
    /// The answer to a `Job::Split`, which it echoes: the pieces, or why the paragraph could not be
    /// cut.
    Split {
        paragraph: u64,
        text: String,
        context: SplitContext,
        result: Result<Vec<Segment>, Failure>,
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
    /// The answer to a `Job::Repair`, which it echoes.
    Repaired {
        job: Job,
        result: Result<Formal, Failure>,
    },
}
