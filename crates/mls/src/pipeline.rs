//! The per-document state: paragraphs, blocks, versions, the next job and the dots.

use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::openrouter::{segments, segments_json};
use crate::rules::{checked_lean, counterexample, declared_names, is_blank};
use crate::{
    Checked, Failure, Formal, Job, Label, Outcome, Reply, Segment, SplitContext, Status, Tag,
};

/// One non-blank line of the document. `open` while the caret is in it or a formula in it is unfinished.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub text: String,
    pub open: bool,
}

/// A status dot beside a text block (index into the last `sync` input). A block holds one item, or
/// the several that one paragraph was cut into (see `Mls::dots`).
#[derive(Clone, Debug, PartialEq)]
pub struct Dot {
    /// The block's first paragraph.
    pub paragraph: usize,
    /// The block's last paragraph; equal to `paragraph` for a one-paragraph block.
    pub last: usize,
    pub key: u64,
    pub status: Status,
    error: Option<String>,
    /// Why the dot is in its state when it has no error text, such as a name it relies on that
    /// does not check.
    note: Option<String>,
    service: bool,
}

/// What the panel shows for one item of a dot.
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    /// The item's label; None when no block has the item's key, such as a failed classification.
    pub label: Option<Label>,
    /// The item's own status, whatever the dot's.
    pub status: Status,
    pub assumptions: Vec<String>,
    pub lean: Option<String>,
    pub error: Option<String>,
    pub service: bool,
    /// The names from the blocks above that the block's Lean uses.
    pub uses: Vec<String>,
    /// The dot's note (see `Dot`).
    pub note: Option<String>,
}

/// A definition or statement in the document's dependency chart. A proof adds what it uses to its
/// statement's node.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// The key of the block's dot.
    pub key: u64,
    /// The block's first paragraph.
    pub paragraph: usize,
    /// Definition or Statement.
    pub label: Label,
    /// The Lean name, or else the start of the block's text.
    pub name: String,
    pub status: Status,
    /// The keys of the nodes above that it uses, in document order.
    pub uses: Vec<u64>,
    /// 0 for a node that uses no other; otherwise one more than the deepest node it uses.
    pub level: usize,
}

#[derive(Default, Debug)]
pub struct Mls {
    paras: Vec<Para>,
    blocks: Vec<Block>,
    /// The settled text the last walk passed, joined by blank lines. Each block's `above` is a byte
    /// length into it, so the text is stored once rather than copied for every block.
    document: String,
    /// Records of blocks that are not in `blocks`, by id. An edit above them then reuses their
    /// formalization and check instead of redoing them. `plan` and `apply` never read them.
    parked: HashMap<u64, Block>,
    /// What Jev said, by paragraph and the text above it, so an unchanged one never asks
    /// again, in this session or after reopening (see `twin`). Only accepted answers are remembered.
    labels: HashMap<TagKey, Tag>,
    /// Validated authoritative segments, keyed by both paragraph and its interpretation context.
    splits: HashMap<SplitKey, Vec<Segment>>,
    /// The formalization, or a non-service rejection, of each block by its key.
    formals: HashMap<FormalKey, Result<Formal, String>>,
    frontier: Frontier,
    busy: bool,
    repl_lost: bool,
}

/// What decides a block's formal: its label, its text and, for a proof, the Lean of its statement.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FormalKey {
    label: Label,
    text: String,
    statement: Option<String>,
}

/// What decides the tag of a paragraph: the text itself, the unit above it (Jev is asked
/// whether it continues that; see `segment`) and whether a proof is open above it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TagKey {
    previous: Option<String>,
    text: String,
    proof_open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SplitKey {
    text: String,
    context: SplitContext,
}

/// Invalidate analysis made with older gates, labels or proof-target rules without changing the
/// twin's outer format. Bump this when those semantics change.
const SEGMENTATION_VERSION: u64 = 2;

/// Opens the memory comment at the end of a twin. Its JSON is written on the lines after it.
const MEMORY_OPEN: &str = "/- lvtex:memory\n";

/// A fresh id for a paragraph, a piece or a version. Global, so a new `Mls` never reuses an id
/// still in flight.
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
    /// Authoritative segments, from the several-item gate or unpaired-proof recovery.
    split: Option<Split>,
}

/// The result of cutting a paragraph into its pieces, like `Labelled` for a tag.
#[derive(Debug)]
struct Split {
    /// The settled text of the paragraph that was cut.
    text: String,
    context: SplitContext,
    /// The segments, a retryable service failure or a permanent structural failure.
    pieces: Result<Vec<Piece>, Failure>,
}

impl Split {
    /// The pieces, none after a failure.
    fn pieces(&self) -> &[Piece] {
        match &self.pieces {
            Ok(pieces) => pieces,
            Err(_) => &[],
        }
    }
}

/// One authoritative segment, with a stable block identity independent of the local model ID.
#[derive(Debug)]
struct Piece {
    /// A fresh id, which the piece keeps while its text is unchanged.
    id: u64,
    segment: Segment,
}

/// The paragraph whose Jev decision the walk is reading.
#[derive(Clone, Copy, Debug)]
struct Spot {
    /// Index of the paragraph in `paras`.
    paragraph: usize,
}

#[derive(Debug)]
struct Labelled {
    previous: Option<String>,
    text: String,
    proof_open: bool,
    result: Result<Tag, Failure>,
}

#[derive(Debug)]
struct Block {
    /// Id of its first unit: a paragraph, or a piece of one.
    id: u64,
    /// A new value from `next()` whenever its text or label changes.
    version: u64,
    /// Index of its first paragraph in `paras`.
    first: usize,
    /// Index of its last paragraph in `paras`; `first` for a one-paragraph block.
    last: usize,
    /// The settled texts of its units, joined by a blank line.
    text: String,
    /// Definition, Statement or Proof; never Skip.
    label: Label,
    /// False while the last block ends in ':' or ',' and may still continue. See `segment`.
    closed: bool,
    /// The byte length of `Mls::document` above `first`: the text above is `document[..above]`, and
    /// zero when nothing is above.
    above: usize,
    /// Proofs: the id of the statement block paired with it.
    statement: Option<u64>,
    formal: Option<Result<Formal, Failure>>,
    /// Proofs: the statement version `formal` was made against.
    statement_version: Option<u64>,
    /// The base environment it was checked on, and the result.
    check: Option<(u64, Result<Checked, Failure>)>,
}

/// Where segmentation stops. A `Classify` frontier is the first unit that needs a tag, and a
/// `Split` frontier the first paragraph needing labeled segmentation or proof recovery; its job
/// runs before checking. `Failed` is a paragraph whose decision or segmentation failed.
#[derive(Default, Debug)]
enum Frontier {
    #[default]
    Done,
    Classify {
        /// The id of the paragraph.
        unit: u64,
        text: String,
        previous: Option<String>,
        proof_open: bool,
    },
    Split {
        paragraph: u64,
        text: String,
        context: SplitContext,
    },
    Failed {
        /// Index of the paragraph, where the dot sits.
        paragraph: usize,
        /// The id of the unit, or of the paragraph for a failed split.
        key: u64,
        failure: Failure,
    },
}

/// The run of units that segmentation is building: a block, or prose that may still become one.
struct Unit {
    /// The id of its first unit.
    id: u64,
    /// Index of its first paragraph in `paras`.
    first: usize,
    /// The settled texts of its units, joined by a blank line.
    text: String,
    /// Skip for prose; otherwise the label of the item it is.
    kind: Label,
    /// Its index in the new blocks, once it is one.
    block: Option<usize>,
    /// True once a paragraph ending in ∎ has closed it.
    ended: bool,
    /// The byte length of the walk's document above its first unit, as in `Block::above`.
    above: usize,
}

/// How the walk reads the tag of one unit.
enum Read {
    /// The unit's tag, current or provisional.
    Tag(Tag),
    /// The unit has no tag and is the `Classify` frontier. It is a separator: nothing joins it.
    Next,
    /// The walk ends here.
    Stop,
}

/// The state of one walk over the units, and the blocks it builds (see `segment`).
struct Walk<'a> {
    labels: &'a HashMap<TagKey, Tag>,
    new: Vec<Block>,
    unit: Option<Unit>,
    /// The text of the last unit, for the one after a separator.
    previous: Option<&'a str>,
    /// The texts of every unit the walk has passed, joined by a blank line.
    document: String,
    frontier: Frontier,
    /// Tags remembered for units with no current one. They are stored once the walk ends.
    remembered: Vec<(Spot, Labelled)>,
}

impl<'a> Walk<'a> {
    fn context(&self) -> SplitContext {
        let previous = self
            .unit
            .as_ref()
            .map(|unit| (unit.kind, unit.text.clone()))
            .or_else(|| self.previous.map(|text| (Label::Skip, text.to_owned())));
        let statement = self
            .unit
            .as_ref()
            .and_then(|unit| {
                // Prose does not replace the last mathematical block. A proof can reuse its
                // statement only while it is the current, unfinished run.
                let block = self.new.last()?;
                match block.label {
                    Label::Statement => Some(block),
                    Label::Proof if unit.kind == Label::Proof && !unit.ended => block
                        .statement
                        .and_then(|id| self.new.iter().find(|block| block.id == id)),
                    Label::Definition | Label::Proof | Label::Skip => None,
                }
            })
            .map(|block| block.text.clone());
        SplitContext {
            previous,
            statement,
        }
    }
    /// Sets the frontier unless one above already is: its job runs first.
    fn mark(&mut self, frontier: Frontier) {
        if matches!(self.frontier, Frontier::Done) {
            self.frontier = frontier;
        }
    }

    /// Makes the unit with `text` a separator: nothing joins it, and the unit below sees it as the
    /// one above.
    fn separate(&mut self, text: &'a str) {
        self.unit = None;
        self.previous = Some(text);
        self.push_document(text);
    }

    /// Appends the settled `text` to the document the walk has passed.
    fn push_document(&mut self, text: &str) {
        if !self.document.is_empty() {
            self.document.push_str("\n\n");
        }
        self.document.push_str(text);
    }

    /// The tag of the unit `id` with `text` at `spot`, whose own record is `label`. Jev sees the
    /// whole run the unit may join, every unit grouped so far, so it judges against the item and
    /// not only its last line. After a separator there is no run, and it sees the unit just above.
    fn tag(&mut self, spot: Spot, id: u64, text: &'a str, label: &Option<Labelled>) -> Read {
        let proof_open = self
            .unit
            .as_ref()
            .is_some_and(|u| u.kind == Label::Proof && !u.ended);
        let above: Option<String> = self
            .unit
            .as_ref()
            .map(|u| u.text.clone())
            .or_else(|| self.previous.map(str::to_owned));
        if let Some(l) = label
            .as_ref()
            .filter(|l| l.previous == above && l.text == text && l.proof_open == proof_open)
        {
            return match &l.result {
                Ok(tag) => Read::Tag(*tag),
                Err(failure) => {
                    self.mark(Frontier::Failed {
                        paragraph: spot.paragraph,
                        key: id,
                        failure: failure.clone(),
                    });
                    Read::Stop
                }
            };
        }
        let key = TagKey {
            previous: above,
            text: text.to_owned(),
            proof_open,
        };
        if let Some(tag) = self.labels.get(&key).copied() {
            self.remembered.push((
                spot,
                Labelled {
                    previous: key.previous,
                    text: key.text,
                    proof_open,
                    result: Ok(tag),
                },
            ));
            return Read::Tag(tag);
        }
        // A tag given to this text before, under other text above or with another proof open,
        // stands in until Jev answers. The walk keeps going with it.
        if let Some(Labelled {
            text: old,
            result: Ok(provisional),
            ..
        }) = label
            && old == text
        {
            self.mark(Frontier::Classify {
                unit: id,
                text: key.text,
                previous: key.previous,
                proof_open,
            });
            return Read::Tag(*provisional);
        }
        // The first unit with no tag is the frontier. A second one stops.
        if !matches!(self.frontier, Frontier::Done) {
            return Read::Stop;
        }
        self.frontier = Frontier::Classify {
            unit: id,
            text: key.text,
            previous: key.previous,
            proof_open,
        };
        Read::Next
    }

    /// Adds the unit `id` with `text` and `tag`, in paragraph `index`, to the run above it or
    /// starts a run with it.
    fn add(&mut self, index: usize, id: u64, text: &'a str, tag: Tag) {
        if let Some(u) = self.unit.as_mut().filter(|u| joins_unit(u, tag)) {
            u.text.push_str("\n\n");
            u.text.push_str(text);
            match u.block {
                Some(b) => {
                    let block = &mut self.new[b];
                    block.text.push_str("\n\n");
                    block.text.push_str(text);
                    block.last = index;
                }
                // Prose that Jev says the unit continues becomes a block of its label.
                None if tag.label != Label::Skip => {
                    let block = new_block(
                        &self.new,
                        u.first,
                        index,
                        u.id,
                        u.text.clone(),
                        tag.label,
                        u.above,
                    );
                    self.new.push(block);
                    u.kind = tag.label;
                    u.block = Some(self.new.len() - 1);
                }
                None => {}
            }
        } else {
            let mut next = Unit {
                id,
                first: index,
                text: text.to_string(),
                kind: tag.label,
                block: None,
                ended: false,
                above: self.document.len(),
            };
            if tag.label != Label::Skip {
                let block = new_block(
                    &self.new,
                    index,
                    index,
                    id,
                    text.to_string(),
                    tag.label,
                    next.above,
                );
                self.new.push(block);
                next.block = Some(self.new.len() - 1);
            }
            self.unit = Some(next);
        }
        if let Some(u) = self
            .unit
            .as_mut()
            .filter(|_| text.trim_end().ends_with('∎'))
        {
            u.ended = true;
        }
        self.previous = Some(text);
        self.push_document(text);
    }
}

struct Plan {
    job: Option<Job>,
    dots: Vec<Dot>,
    lean: Vec<(u64, Label, String)>,
    /// The keys of the chart nodes that each closed block with a formal uses, by block id.
    edges: HashMap<u64, Vec<u64>>,
}

/// A statement whose Lean compiled, for the proof that follows it.
#[derive(Clone)]
struct Stated {
    version: u64,
    lean: String,
    /// Index of its dot in the plan being built.
    dot: usize,
}

fn dot(paragraph: usize, last: usize, key: u64, status: Status) -> Dot {
    Dot {
        paragraph,
        last,
        key,
        status,
        error: None,
        note: None,
        service: false,
    }
}

fn failed(paragraph: usize, last: usize, key: u64, error: &str, service: bool) -> Dot {
    Dot {
        paragraph,
        last,
        key,
        status: Status::Failed,
        error: Some(error.to_string()),
        note: None,
        service,
    }
}

/// A dot with no error text and a note saying why its status is what it is.
fn noted(paragraph: usize, last: usize, key: u64, status: Status, note: String) -> Dot {
    Dot {
        paragraph,
        last,
        key,
        status,
        error: None,
        note: Some(note),
        service: false,
    }
}

/// How bad a status is for the dot of a text block: red, then grey, then amber, then green.
fn severity(status: Status) -> u8 {
    match status {
        Status::Failed => 3,
        Status::Checking => 2,
        Status::Sorry => 1,
        Status::Verified => 0,
    }
}

/// The item dots grouped by text block: sorted by first paragraph, in plan order within a
/// paragraph, and a dot joins the group above while its paragraphs overlap the group's range.
/// The one owner of the rule for `Mls::dots` and `Mls::details`.
fn group_dots(dots: &[Dot]) -> Vec<Vec<&Dot>> {
    let mut sorted: Vec<&Dot> = dots.iter().collect();
    sorted.sort_by_key(|d| d.paragraph);
    let mut groups: Vec<Vec<&Dot>> = Vec::new();
    let mut end = 0;
    for dot in sorted {
        match groups.last_mut() {
            Some(group) if dot.paragraph <= end => {
                end = end.max(dot.last);
                group.push(dot);
            }
            _ => {
                end = dot.last;
                groups.push(vec![dot]);
            }
        }
    }
    groups
}

/// The dot of a group: the first item's key, the union range and the worst status. Its error, note
/// and service flag stay empty, as `Mls::details` has them per item.
fn merge_dots(group: &[&Dot]) -> Dot {
    let last = group.iter().map(|d| d.last).max().unwrap_or_default();
    let status = group
        .iter()
        .map(|d| d.status)
        .max_by_key(|&status| severity(status))
        .unwrap_or(Status::Verified);
    dot(group[0].paragraph, last, group[0].key, status)
}

/// The status of the most recent dot with `key`.
fn latest_status(dots: &[Dot], key: u64) -> Option<Status> {
    dots.iter().rev().find(|d| d.key == key).map(|d| d.status)
}

/// The first name in `uses` whose block is still Sorry. Lean accepts a block that calls such a
/// name, but the block is not proved until that one is.
fn unproved_use<'a>(
    uses: &'a [String],
    declared: &HashMap<String, u64>,
    dots: &[Dot],
) -> Option<&'a str> {
    uses.iter()
        .find(|name| {
            declared
                .get(*name)
                .and_then(|key| latest_status(dots, *key))
                == Some(Status::Sorry)
        })
        .map(String::as_str)
}

/// The name of `statement` and the counterexample sentence, when the Lean of `formal` refutes it.
fn refutation(statement: &Stated, formal: &Formal) -> Option<(String, String)> {
    let name = declared_names(&statement.lean).into_iter().next()?;
    let sentence = counterexample(&formal.lean, &name)?;
    Some((name, sentence))
}

/// Records a verified refutation `formal` of the statement `statement`, named `name`, as the
/// proof's dot. The statement turns red with the counterexample, unless the refutation relies on a
/// name that is not proved yet: then the proof is red with a note saying so, and the statement
/// stays as it was. The name it uses for its own statement is not a dependency.
fn refute(
    dots: &mut Vec<Dot>,
    b: &Block,
    statement: &Stated,
    formal: &Formal,
    declared: &HashMap<String, u64>,
    name: &str,
    sentence: &str,
) {
    let others: Vec<String> = formal
        .uses
        .iter()
        .filter(|used| used.as_str() != name)
        .cloned()
        .collect();
    if let Some(other) = unproved_use(&others, declared, dots) {
        let note = format!("The refutation relies on `{other}`, which is not proved yet.");
        dots.push(noted(b.first, b.last, b.id, Status::Failed, note));
        return;
    }
    dots.push(failed(
        b.first,
        b.last,
        b.id,
        &format!("The statement is false: {sentence}"),
        false,
    ));
    let old = &dots[statement.dot];
    let error = format!("False as formalized: {sentence} Lean proves its negation.");
    let statement_dot = failed(old.paragraph, old.last, old.key, &error, false);
    dots[statement.dot] = statement_dot;
}

/// The key of the chart node a block belongs to. A proof belongs to the statement it proves.
fn node_key(block: &Block) -> u64 {
    match block.label {
        Label::Proof => block.statement.unwrap_or(block.id),
        Label::Definition | Label::Statement | Label::Skip => block.id,
    }
}

/// The Lean name of a definition or statement, else the name it declares, else the start of its text.
fn node_name(block: &Block) -> String {
    if let Some(Ok(formal)) = &block.formal {
        let declared = declared_names(&formal.lean).into_iter().next();
        if let Some(name) = formal.lean_name.clone().or(declared) {
            return name;
        }
    }
    let line = block.text.lines().next().unwrap_or("");
    let start: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        format!("{start}…")
    } else {
        start
    }
}

/// The document text above a block whose `above` is a byte length into `document`, None when
/// nothing is above.
fn document_above(document: &str, above: usize) -> Option<String> {
    (above > 0).then(|| document[..above].to_string())
}

/// The job that formalizes block `b`, against the statement it pairs with, the document above it,
/// the Lean above it and the names declared above it. Its Lean is checked on `base`, the environment
/// the block would be checked on.
fn formalize_job(
    b: &Block,
    document: &str,
    statement: Option<&Stated>,
    lean: &[(u64, Label, String)],
    names: &[String],
    base: u64,
) -> Job {
    Job::Formalize {
        block: b.id,
        version: b.version,
        label: b.label,
        text: b.text.clone(),
        above: document_above(document, b.above),
        lean_above: lean
            .iter()
            .map(|(id, _, lean)| (*id, lean.clone()))
            .collect(),
        names_above: names.to_vec(),
        statement: statement.map(|s| (s.version, s.lean.clone())),
        base,
    }
}

/// Whether `text` is an ATX Markdown heading: one to six `#` followed by a space or the end.
fn is_heading(text: &str) -> bool {
    let trimmed = text.trim_start();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes) && matches!(trimmed[hashes..].chars().next(), None | Some(' '))
}

/// Whether a paragraph with `tag` joins `unit`, the unit above it. An unfinished proof takes a
/// proof. Otherwise Jev must say the paragraph continues the unit, and the two must be of one label,
/// or the paragraph must be Skip, or the unit prose.
fn joins_unit(unit: &Unit, tag: Tag) -> bool {
    !unit.ended
        && ((unit.kind == Label::Proof && tag.label == Label::Proof)
            || (tag.joins
                && (tag.label == Label::Skip
                    || unit.kind == Label::Skip
                    || tag.label == unit.kind)))
}

/// The kinds a statement or a proof can name, as in "Theorem 2.1" or "Proof of Lemma A".
const STATEMENT_KINDS: [&str; 6] = [
    "theorem",
    "lemma",
    "proposition",
    "corollary",
    "claim",
    "conjecture",
];

/// The label a paragraph starts with, after optional emphasis, as in "**Theorem 2.1.**" or
/// "*Proof of the Lemma A.*": whether "proof of" came first, then the kind and the label, both
/// lowercased. None when there is no kind, or no label after it.
fn reference(text: &str) -> Option<(bool, String, String)> {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '_'));
    let (proof, rest) = match strip_word(text, "proof").and_then(|rest| strip_word(rest, "of")) {
        Some(rest) => (true, strip_word(rest, "the").unwrap_or(rest)),
        None => (false, text),
    };
    let (kind, rest) = STATEMENT_KINDS
        .iter()
        .find_map(|kind| Some((*kind, strip_word(rest, kind)?)))?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    let label = rest[..end].trim_end_matches(['.', '-']);
    (!label.is_empty()).then(|| (proof, kind.to_string(), label.to_ascii_lowercase()))
}

/// `text` without its leading `word` and the whitespace after it, when `word` is followed by
/// whitespace. The comparison ignores ASCII case.
fn strip_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    let rest = &text[word.len()..];
    (head.eq_ignore_ascii_case(word) && rest.starts_with(char::is_whitespace))
        .then(|| rest.trim_start())
}

/// The statement a proof of `text` pairs with. A proof that names its statement, as in "Proof of
/// Lemma 1", pairs with the nearest statement above with that label, wherever it is. Any other
/// proof pairs with the block directly above, when that is a statement. None when there is none.
fn target(new: &[Block], text: &str) -> Option<u64> {
    match reference(text) {
        Some((true, kind, label)) => new.iter().rev().find(|block| {
            block.label == Label::Statement
                && reference(&block.text)
                    .is_some_and(|(proof, k, l)| !proof && k == kind && l == label)
        }),
        _ => new.last().filter(|block| block.label == Label::Statement),
    }
    .map(|block| block.id)
}

/// A block of `label` from paragraph `first` to `last`. A proof pairs with its `target`.
fn new_block(
    new: &[Block],
    first: usize,
    last: usize,
    id: u64,
    text: String,
    label: Label,
    above: usize,
) -> Block {
    let statement = match label {
        Label::Proof => target(new, &text),
        Label::Definition | Label::Statement | Label::Skip => None,
    };
    Block {
        id,
        version: 0,
        first,
        last,
        text,
        label,
        closed: true,
        above,
        statement,
        formal: None,
        statement_version: None,
        check: None,
    }
}

/// The key a block's formal is remembered under. A proof's key carries the Lean of its statement,
/// so a proof is not restored against a statement that has changed. None when that statement has
/// no formal Lean yet, or when the proof's formal was made for an earlier version of the statement:
/// its Lean would then be filed under the statement's new Lean and restored as if it proved it.
fn formal_key(blocks: &[Block], block: &Block) -> Option<FormalKey> {
    let statement = match block.label {
        Label::Definition | Label::Statement | Label::Skip => None,
        Label::Proof => {
            let statement = blocks.iter().find(|b| Some(b.id) == block.statement)?;
            if block.formal.is_some() && block.statement_version != Some(statement.version) {
                return None;
            }
            match &statement.formal {
                Some(Ok(formal)) => Some(formal.lean.clone()),
                Some(Err(_)) | None => return None,
            }
        }
    };
    Some(FormalKey {
        label: block.label,
        text: block.text.clone(),
        statement,
    })
}

/// Clears a tag that failed for a service, so the unit is classified again.
fn forget_service_failure(label: &mut Option<Labelled>) {
    if matches!(label, Some(Labelled { result: Err(f), .. }) if f.service) {
        *label = None;
    }
}

/// The tag slot of the paragraph with `id` and settled `text`.
fn label_slot<'a>(paras: &'a mut [Para], id: u64, text: &str) -> Option<&'a mut Option<Labelled>> {
    for para in paras {
        if para.id == id {
            return (para.settled.as_deref() == Some(text)).then_some(&mut para.label);
        }
    }
    None
}

/// A JSON string, or null for None. Anything else is malformed, and so is a missing value.
fn nullable(value: Option<&Value>) -> Option<Option<String>> {
    match value? {
        Value::Null => Some(None),
        Value::String(text) => Some(Some(text.clone())),
        _ => None,
    }
}

fn label_json(labelled: &Labelled) -> Option<Value> {
    let Ok(tag) = &labelled.result else {
        return None;
    };
    Some(json!({
        "previous": labelled.previous,
        "text": labelled.text,
        "proof_open": labelled.proof_open,
        "label": tag.label.name(),
        "joins": tag.joins,
        "split": tag.split,
    }))
}

fn label_entry(entry: &Value) -> Option<(TagKey, Tag)> {
    let key = TagKey {
        previous: nullable(entry.get("previous"))?,
        text: entry.get("text")?.as_str()?.to_owned(),
        proof_open: entry.get("proof_open")?.as_bool()?,
    };
    let tag = Tag {
        label: Label::from_name(entry.get("label")?.as_str()?)?,
        joins: entry.get("joins")?.as_bool()?,
        split: entry.get("split")?.as_bool()?,
    };
    Some((key, tag))
}

fn context_json(context: &SplitContext) -> Value {
    json!({"previous": context.previous.as_ref().map(|(label, text)| json!({"role": label.name(), "text": text})), "statement": context.statement})
}

fn context_entry(entry: &Value) -> Option<SplitContext> {
    let previous = match entry.get("previous")? {
        Value::Null => None,
        value => Some((
            Label::from_name(value.get("role")?.as_str()?)?,
            value.get("text")?.as_str()?.to_owned(),
        )),
    };
    Some(SplitContext {
        previous,
        statement: nullable(entry.get("statement"))?,
    })
}

fn split_entry(entry: &Value) -> Option<(SplitKey, Vec<Segment>)> {
    let text = entry.get("text")?.as_str()?.to_owned();
    let context = context_entry(entry.get("context")?)?;
    let items = segments(entry.get("segments")?, &text, &context).ok()?;
    Some((SplitKey { text, context }, items))
}

fn refresh_pieces(
    slot: &mut Option<Split>,
    text: &str,
    context: &SplitContext,
    cache: &HashMap<SplitKey, Vec<Segment>>,
) {
    if slot
        .as_ref()
        .is_some_and(|split| split.text == text && split.context == *context)
    {
        return;
    }
    let key = SplitKey {
        text: text.to_owned(),
        context: context.clone(),
    };
    let Some(items) = cache.get(&key) else {
        return;
    };
    let mut earlier = slot
        .take()
        .and_then(|split| split.pieces.ok())
        .unwrap_or_default();
    let pieces = items
        .iter()
        .map(|segment| {
            let id = earlier
                .iter()
                .position(|old| {
                    old.segment.text == segment.text && old.segment.label == segment.label
                })
                .map(|index| earlier.remove(index).id)
                .unwrap_or_else(next);
            Piece {
                id,
                segment: segment.clone(),
            }
        })
        .collect();
    *slot = Some(Split {
        text: text.to_owned(),
        context: context.clone(),
        pieces: Ok(pieces),
    });
}

fn formal_entry(entry: &Value) -> Option<(FormalKey, Result<Formal, String>)> {
    let key = FormalKey {
        label: Label::from_name(entry.get("label")?.as_str()?)?,
        text: entry.get("text")?.as_str()?.to_owned(),
        statement: nullable(entry.get("statement"))?,
    };
    let value = match entry.get("rejected") {
        Some(message) => Err(message.as_str()?.to_owned()),
        None => {
            let formal = Formal {
                lean: entry.get("lean")?.as_str()?.to_owned(),
                lean_name: nullable(entry.get("lean_name"))?,
                assumptions: entry
                    .get("assumptions")?
                    .as_array()?
                    .iter()
                    .map(|assumption| assumption.as_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
                uses: entry
                    .get("uses")?
                    .as_array()?
                    .iter()
                    .map(|name| name.as_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
            };
            // A restored Lean is not trusted: it passes the rules a model's reply passes. The names
            // above are not known here, so the redefinition rule is skipped; Lean rejects a duplicate
            // declaration itself. A refutation of its statement is a proof's other possible Lean.
            let accepted = match (key.label, key.statement.as_deref()) {
                (Label::Proof, Some(statement))
                    if crate::rules::reject_refutation(&formal.lean, statement).is_ok() =>
                {
                    Ok(())
                }
                _ => crate::rules::reject(key.label, &formal, &[], key.statement.as_deref()),
            };
            match accepted {
                Ok(()) => Ok(formal),
                Err(message) => Err(message),
            }
        }
    };
    Some((key, value))
}

impl Mls {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the document: its paragraphs, blocks, parked records, remembered answers, frontier
    /// and lost REPL. A job for the old document may still be in flight, so `busy` stays set until
    /// its reply is applied.
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
                    split: None,
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

    /// Tries the service again after a failure: clears what service failures left behind and
    /// segments, so the next `next_job` runs the failed job again without an edit. Call it after the
    /// key or the settings change. A job already in flight still finishes.
    pub fn retry(&mut self) {
        self.retry_services();
        self.segment();
    }

    /// Clears what a service failure left behind, so the next edit or `retry` tries again. A lost
    /// REPL clears every check, since the environments they were made on are gone.
    fn retry_services(&mut self) {
        for para in &mut self.paras {
            forget_service_failure(&mut para.label);
            match &mut para.split {
                Some(Split { pieces: Err(f), .. }) if f.service => para.split = None,
                Some(_) | None => {}
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

    /// Rebuilds the blocks and the frontier from the paragraphs.
    ///
    /// The walk goes over units: the settled paragraphs, except that a paragraph Jev says holds
    /// several items is walked as authoritative labeled pieces, never reclassified. An ordinary
    /// unpaired proof takes that same path once. A piece belongs to its paragraph's index. Until a paragraph
    /// has its pieces it is a separator, like the `Classify` frontier, and the frontier is its
    /// `Split`. Invalid segmentation stops with a structural error; service failures wait for retry.
    ///
    /// Consecutive units form a run. A unit joins the run above it when `joins_unit` says so: an
    /// unfinished proof takes a proof whatever Jev says, and otherwise Jev must say the unit
    /// continues the run. A run that is prose becomes a block when a joined unit has a label. A
    /// unit that starts a run is not joined to the one above. A block with no formal takes the one
    /// remembered for its key, and a block reuses the record with its id, from `blocks` or else
    /// `parked`, when its text and label are unchanged.
    ///
    /// A unit's tag is current when it was labelled with the settled text above it and the same
    /// `proof_open`, or when that exact key is remembered. Otherwise, when the same text was tagged
    /// before, that tag stands in provisionally: the walk uses it, and the unit is classified again
    /// once the frontier reaches it. So the blocks below an edit keep their dots while Jev answers.
    /// A unit with no tag at all is the frontier: the first one counts as a separator, so nothing
    /// joins it until Jev answers, and a second one stops the walk. A pending Classify or Split
    /// job always runs first, so a provisional tag never leads to a Formalize or Check. The last
    /// block is open, so it takes no formal yet, while its text ends in ':' or ','. A proof
    /// without ∎ is not open: Enter closes it like any other block, and a paragraph that joins it
    /// later formalizes it again.
    fn segment(&mut self) {
        let mut old: HashMap<u64, Block> = take(&mut self.blocks)
            .into_iter()
            .map(|block| (block.id, block))
            .collect();
        let mut parked = take(&mut self.parked);
        let mut walk = Walk {
            labels: &self.labels,
            new: Vec::new(),
            unit: None,
            previous: None,
            document: String::new(),
            frontier: Frontier::Done,
            remembered: Vec::new(),
        };
        for (i, para) in self.paras.iter_mut().enumerate() {
            let Some(text) = para.settled.as_deref() else {
                continue;
            };
            // A heading names a section and is never a statement: it gets no job and separates.
            if is_heading(text) {
                walk.separate(text);
                continue;
            }
            let spot = Spot { paragraph: i };
            let tag = match walk.tag(spot, para.id, text, &para.label) {
                Read::Tag(tag) => tag,
                Read::Next => {
                    walk.separate(text);
                    continue;
                }
                Read::Stop => break,
            };
            let context = walk.context();
            refresh_pieces(&mut para.split, text, &context, &self.splits);
            let current_split = para
                .split
                .as_ref()
                .filter(|split| split.text == text && split.context == context);
            let orphan = tag.label == Label::Proof
                && !tag.split
                && !walk
                    .unit
                    .as_ref()
                    .is_some_and(|unit| unit.kind == Label::Proof && !unit.ended)
                && target(&walk.new, text).is_none();
            if !tag.split && !orphan && current_split.is_none() {
                walk.add(i, para.id, text, tag);
                continue;
            }
            // Keep the old segments' dots during context reanalysis, just as provisional paragraph
            // tags do. The frontier wins over every formalization/check until fresh segments arrive.
            let split = current_split.or_else(|| {
                let provisional = para
                    .split
                    .as_ref()
                    .filter(|split| split.text == text && split.pieces.is_ok())?;
                walk.mark(Frontier::Split {
                    paragraph: para.id,
                    text: text.to_owned(),
                    context: context.clone(),
                });
                Some(provisional)
            });
            match split {
                Some(Split {
                    pieces: Ok(pieces), ..
                }) => {
                    for piece in pieces {
                        let item = &piece.segment;
                        // Labeled items always start distinct runs. Only a proof explicitly targeting
                        // the preceding statement may continue its open proof across paragraphs.
                        let continuation = item.proves_previous
                            && walk
                                .unit
                                .as_ref()
                                .is_some_and(|unit| unit.kind == Label::Proof && !unit.ended);
                        // Keep prose as context for the new item, without joining labeled runs.
                        if !continuation
                            && walk
                                .unit
                                .as_ref()
                                .is_some_and(|unit| unit.kind != Label::Skip)
                        {
                            walk.unit = None;
                        }
                        walk.add(
                            i,
                            piece.id,
                            &item.text,
                            Tag {
                                label: item.label,
                                joins: false,
                                split: false,
                            },
                        );
                        if item.label == Label::Proof
                            && item.proves.is_none()
                            && !item.proves_previous
                            && let Some(block) = walk.new.last_mut()
                        {
                            block.statement = None;
                        }
                    }
                }
                Some(Split {
                    pieces: Err(failure),
                    ..
                }) => {
                    walk.mark(Frontier::Failed {
                        paragraph: i,
                        key: para.id,
                        failure: failure.clone(),
                    });
                    break;
                }
                _ => {
                    // The first paragraph to cut is the frontier; a second one stops.
                    if !matches!(walk.frontier, Frontier::Done) {
                        break;
                    }
                    walk.frontier = Frontier::Split {
                        paragraph: para.id,
                        text: text.to_owned(),
                        context,
                    };
                    walk.separate(text);
                }
            }
        }
        let Walk {
            mut new,
            unit,
            frontier,
            remembered,
            document,
            ..
        } = walk;
        if let Some(Unit { block: Some(b), .. }) = &unit {
            let block = &mut new[*b];
            let end = block.text.trim_end();
            block.closed = !(end.ends_with(':') || end.ends_with(','));
        }

        let ids: HashSet<u64> = self
            .paras
            .iter()
            .flat_map(|para| {
                let pieces = para.split.iter().flat_map(Split::pieces);
                std::iter::once(para.id).chain(pieces.map(|piece| piece.id))
            })
            .collect();
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
        for (spot, labelled) in remembered {
            self.paras[spot.paragraph].label = Some(labelled);
        }

        // A block with no formal takes the one remembered for its key. A statement comes before its
        // proof, so a proof sees a statement restored above it.
        for i in 0..new.len() {
            if new[i].formal.is_some() {
                continue;
            }
            let Some(key) = formal_key(&new, &new[i]) else {
                continue;
            };
            let Some(known) = self.formals.get(&key) else {
                continue;
            };
            let statement_version = new[i]
                .statement
                .and_then(|id| new.iter().find(|b| b.id == id))
                .map(|b| b.version);
            let block = &mut new[i];
            block.formal = Some(known.clone().map_err(|message| Failure {
                message,
                service: false,
            }));
            block.statement_version = statement_version;
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
        self.document = document;
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

    /// Applies a worker's reply. Returns false when the reply is stale: the paragraph, piece or
    /// block it was computed for has changed since. A failed check still clears the checks that a
    /// lost REPL invalidates, even when its own reply is stale.
    pub fn apply(&mut self, reply: Reply) -> bool {
        self.busy = false;
        match reply {
            Reply::Classified {
                paragraph,
                previous,
                text,
                proof_open,
                result,
            } => {
                let Some(slot) = label_slot(&mut self.paras, paragraph, &text) else {
                    return false;
                };
                if let Ok(tag) = &result {
                    self.labels.insert(
                        TagKey {
                            previous: previous.clone(),
                            text: text.clone(),
                            proof_open,
                        },
                        *tag,
                    );
                }
                *slot = Some(Labelled {
                    previous,
                    text,
                    proof_open,
                    result,
                });
                self.segment();
                true
            }
            Reply::Split {
                paragraph,
                text,
                context,
                result,
            } => {
                if self.plan().job.as_ref()
                    != Some(&Job::Split {
                        paragraph,
                        text: text.clone(),
                        context: context.clone(),
                    })
                {
                    return false;
                }
                let Some(para) = self.paras.iter_mut().find(|para| {
                    para.id == paragraph && para.settled.as_deref() == Some(text.as_str())
                }) else {
                    return false;
                };
                let result =
                    result.and_then(|items| segments(&segments_json(&items), &text, &context));
                match result {
                    Ok(items) => {
                        self.splits.insert(SplitKey { text, context }, items);
                    }
                    Err(failure) => {
                        para.split = Some(Split {
                            text,
                            context,
                            pieces: Err(failure),
                        });
                    }
                }
                self.segment();
                true
            }
            Reply::Formalized {
                block,
                version,
                statement_version,
                result,
                check,
            } => {
                if let Some((_, check)) = &check {
                    self.check_failed(check);
                }
                let Some(index) = self
                    .blocks
                    .iter()
                    .position(|b| b.id == block && b.version == version)
                else {
                    return false;
                };
                let b = &mut self.blocks[index];
                b.formal = Some(result);
                b.statement_version = statement_version;
                b.check = check;
                self.remember_formal(index);
                true
            }
            Reply::Checked {
                block,
                version,
                base,
                result,
            } => {
                self.check_failed(&result);
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

    /// Clears what a failed check invalidates, whether its reply is current or not. A service
    /// failure means the REPL is gone, so the next edit or `retry` tries again (see
    /// `retry_services`). Any other failure is a timeout, which kills the REPL: every environment
    /// that a check was made on is gone, so every check is cleared. The reply's own check is stored
    /// by the caller, after this.
    fn check_failed(&mut self, result: &Result<Checked, Failure>) {
        match result {
            Err(failure) if failure.service => self.repl_lost = true,
            Err(_) => {
                for b in self.blocks.iter_mut().chain(self.parked.values_mut()) {
                    b.check = None;
                }
            }
            Ok(_) => {}
        }
    }

    /// The dots of the text blocks: one for a paragraph, or for the paragraphs that one item spans,
    /// however many items it holds. Its colour is the worst of its items' (red, then grey, then
    /// amber, then green). The plan's own dots stay per item, for the chart and the checks.
    pub fn dots(&self) -> Vec<Dot> {
        let plan = self.plan();
        group_dots(&plan.dots)
            .iter()
            .map(|group| merge_dots(group))
            .collect()
    }

    /// The panel's details for the items of the dot with `key`, in document order. Empty when no
    /// dot has that key.
    pub fn details(&self, key: u64) -> Vec<Details> {
        let plan = self.plan();
        let groups = group_dots(&plan.dots);
        let Some(group) = groups.iter().find(|group| group[0].key == key) else {
            return Vec::new();
        };
        group
            .iter()
            .map(|found| {
                let block = self.blocks.iter().find(|b| b.id == found.key);
                let mut details = Details {
                    label: block.map(|b| b.label),
                    status: found.status,
                    assumptions: Vec::new(),
                    lean: None,
                    error: found.error.clone(),
                    service: found.service,
                    uses: Vec::new(),
                    note: found.note.clone(),
                };
                if let Some(Block {
                    formal: Some(Ok(formal)),
                    ..
                }) = block
                {
                    details.assumptions = formal.assumptions.clone();
                    details.lean = Some(formal.lean.clone());
                    details.uses = formal.uses.clone();
                }
                details
            })
            .collect()
    }

    /// The dependency chart: each definition and statement, with the nodes above it that it uses
    /// and its level. A proof's uses are added to its statement's node.
    pub fn chart(&self) -> Vec<Node> {
        let plan = self.plan();
        let mut nodes: Vec<Node> = Vec::new();
        for block in &self.blocks {
            let Some(status) = latest_status(&plan.dots, block.id) else {
                continue;
            };
            match block.label {
                Label::Definition | Label::Statement => nodes.push(Node {
                    key: block.id,
                    paragraph: block.first,
                    label: block.label,
                    name: node_name(block),
                    status,
                    uses: plan.edges.get(&block.id).cloned().unwrap_or_default(),
                    level: 0,
                }),
                Label::Proof => {
                    let Some(node) = block
                        .statement
                        .and_then(|id| nodes.iter_mut().find(|n| n.key == id))
                    else {
                        continue;
                    };
                    for &key in plan.edges.get(&block.id).into_iter().flatten() {
                        if !node.uses.contains(&key) {
                            node.uses.push(key);
                        }
                    }
                }
                Label::Skip => {}
            }
        }
        let position: HashMap<u64, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.key, i))
            .collect();
        for i in 0..nodes.len() {
            // Only nodes above this one count; the walk declares names top to bottom, so all do.
            let mut above: Vec<usize> = nodes[i]
                .uses
                .iter()
                .filter_map(|key| position.get(key).copied())
                .filter(|&p| p < i)
                .collect();
            above.sort_unstable();
            let uses: Vec<u64> = above.iter().map(|&p| nodes[p].key).collect();
            let level = above.iter().map(|&p| nodes[p].level + 1).max().unwrap_or(0);
            nodes[i].uses = uses;
            nodes[i].level = level;
        }
        nodes
    }

    /// Remembers block `index`'s formal under its key, unless its failure is a service's.
    fn remember_formal(&mut self, index: usize) {
        let block = &self.blocks[index];
        let value = match &block.formal {
            Some(Ok(formal)) => Ok(formal.clone()),
            Some(Err(failure)) if !failure.service => Err(failure.message.clone()),
            Some(Err(_)) | None => return,
        };
        if let Some(key) = formal_key(&self.blocks, block) {
            self.formals.insert(key, value);
        }
    }

    /// The Lean that checked, then what MLS remembers, so the file is a twin of the document.
    /// `restore` reads the memory back, so reopening the document asks no model about unchanged
    /// text. Lean still checks every block again.
    pub fn twin(&self) -> String {
        let mut twin = String::from("import Mathlib\n");
        for (id, label, lean) in self.plan().lean {
            twin.push_str(&format!(
                "\n-- lvtex:block {id} {}\n{}\n",
                label.name(),
                lean.trim_end()
            ));
        }
        twin.push('\n');
        twin.push_str(MEMORY_OPEN);
        twin.push_str(&self.memory());
        twin.push_str("\n-/\n");
        twin
    }

    /// The memory comment's JSON, on one line. Each `-` becomes the JSON escape for hyphen-minus,
    /// so the JSON never contains `-/`, which would close the comment early, or `/-`, which Lean
    /// reads as a nested comment that the closing `-/` cannot end. Every `-` in the memory is
    /// inside a JSON string, as it has no negative numbers, so the escape keeps the JSON valid.
    fn memory(&self) -> String {
        let splits: Vec<&Split> = self
            .paras
            .iter()
            .filter_map(|para| {
                para.split.as_ref().filter(|split| {
                    para.settled.as_deref() == Some(split.text.as_str()) && split.pieces.is_ok()
                })
            })
            .collect();
        let labels: Vec<Value> = self
            .paras
            .iter()
            .filter(|para| para.settled.is_some())
            .map(|para| &para.label)
            .filter_map(|label| label.as_ref().and_then(label_json))
            .collect();
        let splits: Vec<Value> = splits
            .iter()
            .map(|split| {
                let items: Vec<Segment> = split
                    .pieces()
                    .iter()
                    .map(|piece| piece.segment.clone())
                    .collect();
                json!({"text": split.text, "context": context_json(&split.context), "segments": segments_json(&items)})
            })
            .collect();
        let formals: Vec<Value> = self
            .blocks
            .iter()
            .filter_map(|block| {
                let key = formal_key(&self.blocks, block)?;
                match &block.formal {
                    Some(Ok(formal)) => Some(json!({
                        "label": key.label.name(),
                        "text": key.text,
                        "statement": key.statement,
                        "lean": formal.lean,
                        "lean_name": formal.lean_name,
                        "assumptions": formal.assumptions,
                        "uses": formal.uses,
                    })),
                    Some(Err(failure)) if !failure.service => Some(json!({
                        "label": key.label.name(),
                        "text": key.text,
                        "statement": key.statement,
                        "rejected": failure.message,
                    })),
                    Some(Err(_)) | None => None,
                }
            })
            .collect();
        json!({"version": 3, "segmentation_version": SEGMENTATION_VERSION, "labels": labels, "splits": splits, "formals": formals})
            .to_string()
            .replace('-', "\\u002d")
    }

    /// Restores the labels, splits and formalizations that `twin` remembers. Call it on a fresh or
    /// reset `Mls`, before the first `sync`. A twin without the current internal segmentation
    /// revision changes nothing (including unmarked version 3 memory), and
    /// an entry that is malformed is skipped. Restored Lean passes the same rules as a model's
    /// reply: a formal the rules reject is restored as a rejection, and is never checked by Lean, and a
    /// split that does not reproduce its paragraph is skipped.
    pub fn restore(&mut self, twin: &str) {
        let Some(start) = twin.rfind(MEMORY_OPEN) else {
            return;
        };
        let body = &twin[start + MEMORY_OPEN.len()..];
        let Some(end) = body.find("\n-/") else {
            return;
        };
        let Ok(memory) = serde_json::from_str::<Value>(&body[..end]) else {
            return;
        };
        if memory.get("version").and_then(Value::as_u64) != Some(3)
            || memory.get("segmentation_version").and_then(Value::as_u64)
                != Some(SEGMENTATION_VERSION)
        {
            return;
        }
        let (Some(labels), Some(splits), Some(formals)) = (
            memory.get("labels").and_then(Value::as_array),
            memory.get("splits").and_then(Value::as_array),
            memory.get("formals").and_then(Value::as_array),
        ) else {
            return;
        };
        self.labels.extend(labels.iter().filter_map(label_entry));
        self.splits.extend(splits.iter().filter_map(split_entry));
        self.formals.extend(formals.iter().filter_map(formal_entry));
    }

    /// Walks the closed blocks in document order and works out the next job, the dots and the
    /// Lean chain. A Classify or Split job comes first: when the frontier is a unit that needs a
    /// label or a paragraph that needs cutting, it wins over a pending Formalize or Check of an
    /// earlier block, so the earlier block waits its turn behind the work that the frontier needs.
    fn plan(&self) -> Plan {
        let mut dots = Vec::new();
        let mut job = match &self.frontier {
            Frontier::Classify {
                unit,
                text,
                previous,
                proof_open,
            } => Some(Job::Classify {
                paragraph: *unit,
                previous: previous.clone(),
                text: text.clone(),
                proof_open: *proof_open,
            }),
            Frontier::Split {
                paragraph,
                text,
                context,
            } => Some(Job::Split {
                paragraph: *paragraph,
                text: text.clone(),
                context: context.clone(),
            }),
            Frontier::Done | Frontier::Failed { .. } => None,
        };
        if let Frontier::Failed {
            paragraph,
            key,
            failure,
        } = &self.frontier
        {
            dots.push(failed(
                *paragraph,
                *paragraph,
                *key,
                &failure.message,
                failure.service,
            ));
        }

        let mut pending = false;
        let mut halted = false;
        let mut chain = 0u64;
        let mut names: Vec<String> = Vec::new();
        let mut lean: Vec<(u64, Label, String)> = Vec::new();
        let mut statements: HashMap<u64, Stated> = HashMap::new();
        // The node that declares each name, from the formals of the closed blocks above, whatever
        // their status. A rejected formal above sets `gap`.
        let mut declared: HashMap<String, u64> = HashMap::new();
        let mut gap = false;
        let mut edges: HashMap<u64, Vec<u64>> = HashMap::new();
        for b in self.blocks.iter().take_while(|b| b.closed) {
            if halted {
                break;
            }
            // Recorded before the pending shortcut, so a block below a pending one still links up.
            let key = node_key(b);
            if let Some(Ok(formal)) = &b.formal {
                let mut used: Vec<u64> = Vec::new();
                for name in &formal.uses {
                    let Some(&node) = declared.get(name) else {
                        continue;
                    };
                    if node != key && !used.contains(&node) {
                        used.push(node);
                    }
                }
                edges.insert(b.id, used);
                for name in declared_names(&formal.lean) {
                    declared.entry(name).or_insert(key);
                }
            }
            gap |= matches!(b.formal, Some(Err(_)));
            if pending {
                dots.push(dot(b.first, b.last, b.id, Status::Checking));
                continue;
            }
            let stmt: Option<Stated> = match (b.label, b.statement) {
                (Label::Proof, None) => {
                    let error = "There is no statement directly above this proof.";
                    dots.push(failed(b.first, b.last, b.id, error, false));
                    continue;
                }
                (Label::Proof, Some(id)) => match statements.get(&id) {
                    Some(stated) => Some(stated.clone()),
                    None => {
                        let error = "The statement above has no valid Lean.";
                        dots.push(failed(b.first, b.last, b.id, error, false));
                        continue;
                    }
                },
                (Label::Definition | Label::Statement | Label::Skip, _) => None,
            };

            let stale_proof = stmt
                .as_ref()
                .is_some_and(|s| b.statement_version != Some(s.version));
            let formal = match &b.formal {
                Some(Ok(formal)) if !stale_proof => formal,
                Some(Err(failure)) if !stale_proof => {
                    dots.push(failed(
                        b.first,
                        b.last,
                        b.id,
                        &failure.message,
                        failure.service,
                    ));
                    halted |= failure.service;
                    continue;
                }
                _ => {
                    job.get_or_insert_with(|| {
                        formalize_job(b, &self.document, stmt.as_ref(), &lean, &names, chain)
                    });
                    pending = true;
                    dots.push(dot(b.first, b.last, b.id, Status::Checking));
                    continue;
                }
            };
            // A definition block that only fixes notation declares nothing: no dot, nothing to check.
            if b.label == Label::Definition && is_blank(&formal.lean) {
                continue;
            }
            // A name the Lean uses that no block above declares: the block it came from was renamed
            // or removed. A rejected block above may have declared it, so then it is not asked again.
            if let Some(name) = formal
                .uses
                .iter()
                .find(|name| !declared.contains_key(*name))
            {
                if gap {
                    let note = format!("Uses `{name}`, which is not defined above.");
                    dots.push(noted(b.first, b.last, b.id, Status::Failed, note));
                } else {
                    job.get_or_insert_with(|| {
                        formalize_job(b, &self.document, stmt.as_ref(), &lean, &names, chain)
                    });
                    pending = true;
                    dots.push(dot(b.first, b.last, b.id, Status::Checking));
                }
                continue;
            }
            // A name whose block does not check. Lean would fail on it, so Lean is not asked.
            let blocked = formal.uses.iter().find(|name| {
                declared
                    .get(*name)
                    .and_then(|key| latest_status(&dots, *key))
                    == Some(Status::Failed)
            });
            if let Some(name) = blocked {
                let note = format!("Uses `{name}`, which does not check.");
                dots.push(noted(b.first, b.last, b.id, Status::Failed, note));
                continue;
            }

            match &b.check {
                // A timed-out check counts as current whatever its base. Re-checking it would
                // hang again, so it waits for the block's text to change.
                Some((checked_base, result))
                    if *checked_base == chain || matches!(result, Err(f) if !f.service) =>
                {
                    match result {
                        Err(failure) => {
                            dots.push(failed(
                                b.first,
                                b.last,
                                b.id,
                                &failure.message,
                                failure.service,
                            ));
                            halted |= failure.service;
                        }
                        Ok(Checked { env, outcome }) => match (b.label, outcome) {
                            (
                                Label::Definition | Label::Statement | Label::Proof | Label::Skip,
                                Outcome::Failed(message),
                            ) => {
                                dots.push(failed(b.first, b.last, b.id, message, false));
                            }
                            (Label::Definition, Outcome::Sorry) => {
                                let error = "The definition uses sorry.";
                                dots.push(failed(b.first, b.last, b.id, error, false));
                            }
                            (Label::Proof, Outcome::Sorry) => {
                                let error = "The proof uses sorry.";
                                dots.push(failed(b.first, b.last, b.id, error, false));
                            }
                            (Label::Definition, Outcome::Verified) => {
                                match unproved_use(&formal.uses, &declared, &dots) {
                                    Some(name) => {
                                        let note =
                                            format!("Relies on `{name}`, which is not proved yet.");
                                        dots.push(noted(
                                            b.first,
                                            b.last,
                                            b.id,
                                            Status::Sorry,
                                            note,
                                        ));
                                    }
                                    None => dots.push(dot(b.first, b.last, b.id, Status::Verified)),
                                }
                                chain = *env;
                                names.extend(declared_names(&formal.lean));
                                lean.push((b.id, Label::Definition, formal.lean.clone()));
                            }
                            (Label::Statement, _) => {
                                dots.push(dot(b.first, b.last, b.id, Status::Sorry));
                                chain = *env;
                                names.extend(declared_names(&formal.lean));
                                lean.push((b.id, Label::Statement, formal.lean.clone()));
                                statements.insert(
                                    b.id,
                                    Stated {
                                        version: b.version,
                                        lean: formal.lean.clone(),
                                        dot: dots.len() - 1,
                                    },
                                );
                            }
                            (Label::Proof, Outcome::Verified) => {
                                chain = *env;
                                // A refutation proves the statement false, so it never declares a name.
                                let refuted = stmt.as_ref().and_then(|statement| {
                                    let (name, sentence) = refutation(statement, formal)?;
                                    Some((statement, name, sentence))
                                });
                                if let Some((statement, name, sentence)) = refuted {
                                    refute(
                                        &mut dots, b, statement, formal, &declared, &name,
                                        &sentence,
                                    );
                                    lean.push((b.id, Label::Proof, formal.lean.clone()));
                                    continue;
                                }
                                match unproved_use(&formal.uses, &declared, &dots) {
                                    Some(name) => {
                                        let note =
                                            format!("Relies on `{name}`, which is not proved yet.");
                                        dots.push(noted(
                                            b.first,
                                            b.last,
                                            b.id,
                                            Status::Sorry,
                                            note,
                                        ));
                                    }
                                    None => {
                                        dots.push(dot(b.first, b.last, b.id, Status::Verified));
                                        if let Some(statement) = &stmt {
                                            dots[statement.dot].status = Status::Verified;
                                        }
                                    }
                                }
                                // A proof that directly follows its statement replaces the statement's
                                // Lean, which is then the last entry. Otherwise the statement stays and
                                // the proof follows it as an example, so no name is declared twice.
                                if let Some(entry) = lean
                                    .last_mut()
                                    .filter(|(id, _, _)| Some(*id) == b.statement)
                                {
                                    *entry = (b.id, Label::Proof, formal.lean.clone());
                                } else {
                                    let lean_checked = checked_lean(
                                        b.label,
                                        &formal.lean,
                                        stmt.as_ref().map(|s| s.lean.as_str()),
                                    );
                                    lean.push((b.id, Label::Proof, lean_checked));
                                }
                                for name in declared_names(&formal.lean) {
                                    if !names.contains(&name) {
                                        names.push(name);
                                    }
                                }
                            }
                            (Label::Skip, _) => {
                                dots.push(failed(
                                    b.first,
                                    b.last,
                                    b.id,
                                    "Unexpected label.",
                                    false,
                                ));
                            }
                        },
                    }
                }
                _ => {
                    job.get_or_insert_with(|| Job::Check {
                        block: b.id,
                        version: b.version,
                        lean: checked_lean(
                            b.label,
                            &formal.lean,
                            stmt.as_ref().map(|s| s.lean.as_str()),
                        ),
                        base: chain,
                    });
                    pending = true;
                    dots.push(dot(b.first, b.last, b.id, Status::Checking));
                }
            }
        }
        Plan {
            job,
            dots,
            lean,
            edges,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/pipeline.rs"]
mod tests;
