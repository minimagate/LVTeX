//! The sample documents for the MLS tests. Included by `tests/documents.rs` (the live test) and
//! `tests/unit/worker.rs` (the scripted test), so it names the crate's types through `super`.

use super::{Label, Mls, Paragraph, Status, Worker};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Document {
    pub name: &'static str,
    pub lines: &'static [Line],
}

/// One paragraph: its text, the status its dot must end with (None = no dot), and the scripted
/// model answers: Jev's label and, for the first paragraph of a block, the Lean Haiku returns.
#[allow(dead_code)] // The live test reads only `text` and `expect`; the scripted test reads the rest.
pub struct Line {
    pub text: &'static str,
    pub expect: Option<Status>,
    pub label: Label,
    pub lean: Option<&'static str>,
    /// The assumptions Haiku reports for `lean`: one or two plain sentences.
    pub assumptions: &'static [&'static str],
}

const REPLY_TIMEOUT: Duration = Duration::from_secs(600);

pub const DOCUMENTS: [Document; 3] = [
    Document {
        name: "group theory",
        lines: &[
            Line {
                text: r"Let $G$ be a group with identity $1$.",
                expect: None,
                label: Label::Skip,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Definition.** An element $g \in G$ is an involution if $g^2 = 1$.",
                expect: Some(Status::Verified),
                label: Label::Definition,
                lean: Some("def IsInvolution {G : Type*} [Group G] (g : G) : Prop := g ^ 2 = 1"),
                assumptions: &[
                    "g is an element of the group G.",
                    "An involution is an element whose square is the identity.",
                ],
            },
            Line {
                text: r"**Lemma.** If $e \in G$ satisfies $e g = g$ for every $g \in G$, then $e = 1$.",
                expect: Some(Status::Verified),
                label: Label::Statement,
                lean: Some(
                    "theorem left_identity_unique {G : Type*} [Group G] (e : G) (h : ∀ g : G, e * g = g) : e = 1 := by sorry",
                ),
                assumptions: &[
                    "e is an element of the group G.",
                    "The hypothesis says that e times every g is g.",
                ],
            },
            Line {
                text: r"*Proof.* Taking $g = 1$ gives $e = e \cdot 1 = 1$. ∎",
                expect: Some(Status::Verified),
                label: Label::Proof,
                lean: Some(
                    "theorem left_identity_unique {G : Type*} [Group G] (e : G) (h : ∀ g : G, e * g = g) : e = 1 := by\n  simpa using h 1",
                ),
                assumptions: &[
                    "e is an element of the group G.",
                    "The hypothesis says that e times every g is g.",
                ],
            },
            Line {
                text: r"**Lemma.** The identity of $G$ is an involution.",
                expect: Some(Status::Verified),
                label: Label::Statement,
                lean: Some(
                    "theorem one_is_involution {G : Type*} [Group G] : IsInvolution (1 : G) := by sorry",
                ),
                assumptions: &[
                    "G is a group.",
                    "An involution is an element whose square is the identity.",
                ],
            },
            Line {
                text: r"*Proof.* $1^2 = 1$. ∎",
                expect: Some(Status::Verified),
                label: Label::Proof,
                lean: Some(
                    "theorem one_is_involution {G : Type*} [Group G] : IsInvolution (1 : G) := by\n  simp [IsInvolution]",
                ),
                assumptions: &[
                    "G is a group.",
                    "An involution is an element whose square is the identity.",
                ],
            },
        ],
    },
    Document {
        name: "limit lemma",
        lines: &[
            Line {
                text: r"We work with real sequences indexed by $n \in \mathbb{N}$.",
                expect: None,
                label: Label::Skip,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Lemma.** If $a_n \to L$ and $b_n \to M$, then $a_n + b_n \to L + M$.",
                expect: Some(Status::Verified),
                label: Label::Statement,
                lean: Some(
                    "theorem tendsto_add_of_tendsto (a b : ℕ → ℝ) (L M : ℝ) (ha : Filter.Tendsto a Filter.atTop (nhds L)) (hb : Filter.Tendsto b Filter.atTop (nhds M)) : Filter.Tendsto (fun n => a n + b n) Filter.atTop (nhds (L + M)) := by sorry",
                ),
                assumptions: &[
                    "a and b are real sequences indexed by the natural numbers.",
                    "L and M are real numbers, and the sequences converge to them.",
                ],
            },
            Line {
                text: r"*Proof.* Given $\varepsilon > 0$, choose $N$ with $|a_n - L| < \varepsilon/2$ and $|b_n - M| < \varepsilon/2$ for $n \ge N$; the triangle inequality gives $|a_n + b_n - (L + M)| < \varepsilon$. ∎",
                expect: Some(Status::Verified),
                label: Label::Proof,
                lean: Some(
                    "theorem tendsto_add_of_tendsto (a b : ℕ → ℝ) (L M : ℝ) (ha : Filter.Tendsto a Filter.atTop (nhds L)) (hb : Filter.Tendsto b Filter.atTop (nhds M)) : Filter.Tendsto (fun n => a n + b n) Filter.atTop (nhds (L + M)) := by\n  exact ha.add hb",
                ),
                assumptions: &[
                    "a and b are real sequences indexed by the natural numbers.",
                    "L and M are real numbers, and the sequences converge to them.",
                ],
            },
        ],
    },
    Document {
        name: "false lemma",
        lines: &[
            Line {
                text: r"A tempting claim that fails for small $n$.",
                expect: None,
                label: Label::Skip,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Lemma.** For every natural number $n$, $n^2 \ge 2^n$.",
                expect: Some(Status::Sorry),
                label: Label::Statement,
                lean: Some("theorem sq_ge_two_pow (n : ℕ) : n ^ 2 ≥ 2 ^ n := by sorry"),
                assumptions: &[
                    "n is a natural number, so n is at least 0.",
                    "The square and the power are natural-number operations.",
                ],
            },
            Line {
                text: r"*Proof.* By induction on $n$: the base case holds, and $(n+1)^2 \ge 2^{n+1}$ follows from the inductive hypothesis. ∎",
                expect: Some(Status::Failed),
                label: Label::Proof,
                lean: Some(
                    "theorem sq_ge_two_pow (n : ℕ) : n ^ 2 ≥ 2 ^ n := by\n  induction n with\n  | zero => simp\n  | succ k ih => nlinarith [ih]",
                ),
                assumptions: &[
                    "n is a natural number, so n is at least 0.",
                    "The square and the power are natural-number operations.",
                ],
            },
        ],
    },
];

/// The project named by `MATHSTRAL_LEAN_PROJECT`, else the `lean/` project at the repository root.
pub fn lean_project() -> PathBuf {
    match std::env::var_os("MATHSTRAL_LEAN_PROJECT") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lean")),
    }
}

/// Whether the REPL that the checks need is built in `project`.
pub fn lean_ready(project: &Path) -> bool {
    project
        .join(".lake/packages/REPL/.lake/build/bin/repl")
        .is_file()
}

/// Feeds the lines to `mls` as closed paragraphs and runs every job it asks for, in order, until
/// it needs nothing more.
pub fn drive(mls: &mut Mls, worker: &Worker, lines: &[&str]) -> Result<(), String> {
    let paragraphs: Vec<Paragraph> = lines
        .iter()
        .map(|text| Paragraph {
            text: text.to_string(),
            open: false,
        })
        .collect();
    mls.sync(&paragraphs);
    while let Some(job) = mls.next_job() {
        worker.submit(job);
        let reply = worker
            .rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| "no reply within 10 minutes".to_string())?;
        if !mls.apply(reply) {
            return Err("a reply was stale".to_string());
        }
    }
    Ok(())
}

/// The status of the dot at each line of `document` (None where there is no dot).
pub fn outcome(mls: &Mls, document: &Document) -> Vec<(usize, Option<Status>)> {
    let dots = mls.dots();
    (0..document.lines.len())
        .map(|index| {
            let status = dots
                .iter()
                .find(|dot| dot.paragraph == index)
                .map(|dot| dot.status);
            (index, status)
        })
        .collect()
}

/// The statuses that `document` must end with, by line.
pub fn expected(document: &Document) -> Vec<(usize, Option<Status>)> {
    document
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| (index, line.expect))
        .collect()
}

/// Asserts that `document` ends with its expected dots. On a mismatch, prints each dot's text,
/// status and details to stderr first, to show what Lean and the model said.
pub fn assert_outcome(mls: &Mls, document: &Document) {
    let actual = outcome(mls, document);
    let wanted = expected(document);
    if actual != wanted {
        for dot in mls.dots() {
            eprintln!(
                "{} paragraph {} status {:?}\n  text: {}\n  details: {:?}",
                document.name,
                dot.paragraph,
                dot.status,
                document.lines[dot.paragraph].text,
                mls.details(dot.key),
            );
        }
    }
    assert_eq!(actual, wanted, "{}", document.name);
}
