//! The sample documents for the MLS tests. Included by `tests/documents.rs` (the live test) and
//! `tests/unit/worker.rs` (the scripted test), so it names the crate's types through `super`.

use super::{Job, Label, Mls, Paragraph, Reply, Status, Worker};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Document {
    pub name: &'static str,
    pub lines: &'static [Line],
}

/// One paragraph: its text, the status its dot must end with (None = no dot), and the scripted
/// model answers: Jev's label and whether the paragraph continues the one above, and, for the first
/// paragraph of a block, the Lean the formalizer returns.
#[allow(dead_code)] // The live test reads only `text` and `expect`; the scripted test reads the rest.
pub struct Line {
    pub text: &'static str,
    pub expect: Option<Status>,
    pub label: Label,
    pub joins: bool,
    pub lean: Option<&'static str>,
    /// The assumptions the formalizer reports for `lean`: one or two plain sentences.
    pub assumptions: &'static [&'static str],
}

const REPLY_TIMEOUT: Duration = Duration::from_secs(600);

pub const DOCUMENTS: [Document; 4] = [
    Document {
        name: "group theory",
        lines: &[
            Line {
                text: r"Let $G$ be a group with identity $1$.",
                expect: None,
                label: Label::Skip,
                joins: false,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Definition.** An element $g \in G$ is an involution if $g^2 = 1$.",
                expect: Some(Status::Verified),
                label: Label::Definition,
                joins: false,
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
                joins: false,
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
                joins: false,
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
                joins: false,
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
                joins: false,
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
                joins: false,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Lemma.** If $a_n \to L$ and $b_n \to M$, then $a_n + b_n \to L + M$.",
                expect: Some(Status::Verified),
                label: Label::Statement,
                joins: false,
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
                joins: false,
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
                joins: false,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"**Lemma.** For every natural number $n$, $n^2 \ge 2^n$.",
                expect: Some(Status::Sorry),
                label: Label::Statement,
                joins: false,
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
                joins: false,
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
    Document {
        name: "grouped lemma",
        lines: &[
            Line {
                text: r"**Lemma.** For every natural number $n$,",
                expect: Some(Status::Verified),
                label: Label::Statement,
                joins: false,
                lean: Some("theorem add_zero_eq (n : ℕ) : n + 0 = n := by sorry"),
                assumptions: &["n is a natural number."],
            },
            Line {
                text: "$$n + 0 = n.$$",
                expect: None,
                label: Label::Skip,
                joins: true,
                lean: None,
                assumptions: &[],
            },
            Line {
                text: r"*Proof.* Adding zero changes nothing. ∎",
                expect: Some(Status::Verified),
                label: Label::Proof,
                joins: false,
                lean: Some("theorem add_zero_eq (n : ℕ) : n + 0 = n := by\n  simp"),
                assumptions: &["n is a natural number."],
            },
        ],
    },
];

/// A document that only the live test runs. It has no scripted model answers, so its paragraphs
/// carry only their text and the dot they must end with.
#[allow(dead_code)] // Read only by the live test, which the scripted test does not compile.
pub struct LiveDocument {
    pub name: &'static str,
    pub lines: &'static [LiveLine],
}

#[allow(dead_code)] // Read only by the live test, which the scripted test does not compile.
pub struct LiveLine {
    pub text: &'static str,
    pub expect: Option<Status>,
}

/// The documents for the live test. Expected dots are the intended behaviour, written from the
/// text; they are never derived from a run.
#[allow(dead_code)] // Read only by the live test, which the scripted test does not compile.
pub const LIVE_DOCUMENTS: [LiveDocument; 3] = [
    LiveDocument {
        name: "supplied note",
        lines: &[
            LiveLine {
                text: r"In this note we study divisibility of integers and a classical identity for sums of odd numbers, building each result on the ones before it, so that later proofs can simply cite earlier lemmas instead of repeating their arguments.",
                expect: None,
            },
            LiveLine {
                text: r"We say that an integer $a$ divides an integer $b$ when there is an integer $c$ such that $b = a c$; for instance $3$ divides $12$ because $12 = 3 \cdot 4$, while $5$ does not divide $12$ since no integer $c$ satisfies $12 = 5 c$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Let $a$, $b$ and $c$ be integers, and suppose that $a$ divides $b$ and that $b$ divides $c$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Then $a$ divides $c$, so divisibility is transitive.",
                expect: None,
            },
            LiveLine {
                text: r"Proof. By assumption there are integers $d$ and $e$ with $b = a d$ and $c = b e$. Substituting the first equation into the second gives $c = (a d) e$, which still mentions the product $a d$ rather than a single multiple of $a$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"By associativity of multiplication this equals $a (d e)$, and since $d e$ is an integer, the equation $c = a (d e)$ says exactly that $a$ divides $c$. ∎",
                expect: None,
            },
            LiveLine {
                text: r"Lemma. If an integer $a$ divides both $b$ and $c$, then for all integers $x$ and $y$ it also divides the linear combination $b x + c y$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Proof. Write $b = a d$ and $c = a e$ for suitable integers $d$ and $e$. Then $b x + c y = a d x + a e y = a (d x + e y)$, and because $d x + e y$ is an integer, this shows that $a$ divides $b x + c y$, as claimed.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"This lemma is used constantly in elementary number theory, for example in the Euclidean algorithm, where it shows that every common divisor of two numbers also divides the remainder of one by the other.",
                expect: None,
            },
            LiveLine {
                text: r"Theorem. For every natural number $n$, the sum of the first $n$ odd numbers is a perfect square:",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"$$\sum_{i=0}^{n-1} (2i + 1) = n^2.$$",
                expect: None,
            },
            LiveLine {
                text: r"Proof. We argue by induction on $n$. For $n = 0$ the sum is empty, so both sides are equal to $0$ and there is nothing to prove.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Suppose the identity holds for some $n$. Adding the next odd number $2n + 1$ to both sides gives $\sum_{i=0}^{n} (2i + 1) = n^2 + 2n + 1 = (n + 1)^2$, which is exactly the identity for $n + 1$, and the induction is complete.",
                expect: None,
            },
            LiveLine {
                text: r"Corollary. For every natural number $n \ge 1$, the sum of the first $n$ odd numbers is at least $n$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Proof. By the theorem the sum equals $n^2$, and since $n \ge 1$ we have $n^2 = n \cdot n \ge n \cdot 1 = n$. ∎",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"One might hope that $n^2 + n + 41$ is prime for every natural number $n$, since a direct computation shows that it is prime for each of $n = 0, 1, \dots, 39$.",
                expect: Some(Status::Sorry),
            },
            LiveLine {
                text: r"Finally, if $a$ divides $b$ and $b$ divides $c$, then $a$ also divides the sum $b + c$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"This follows from the two lemmas above: $a$ divides $c$ by transitivity, and therefore it divides $b \cdot 1 + c \cdot 1 = b + c$ by the lemma on linear combinations.",
                expect: Some(Status::Verified),
            },
        ],
    },
    LiveDocument {
        name: "prime notation",
        lines: &[
            LiveLine {
                text: r"Write $\mathbb{N}$ for the natural numbers $0, 1, 2, \dots$ and $p \mid m$ for divisibility.",
                expect: None,
            },
            LiveLine {
                text: r"**Definition.** A natural number $p$ is prime when $p \ge 2$ and its only divisors are $1$ and $p$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"**Lemma.** Every prime $p$ is at least $2$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"*Proof.* Immediate from the definition.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"**Lemma.** Every natural number $n \ge 2$ is prime.",
                expect: Some(Status::Sorry),
            },
        ],
    },
    LiveDocument {
        name: "long paragraph",
        lines: &[
            LiveLine {
                text: r"Lemma. For all real numbers $a$ and $b$, $(a + b)^2 = a^2 + 2 a b + b^2$.",
                expect: Some(Status::Verified),
            },
            LiveLine {
                text: r"Proof. Let $a$ and $b$ be real numbers. The square of a number $x$ is the product $x^2 = x x$, so $(a + b)^2 = (a + b)(a + b)$, and the task is to expand this product into a sum of terms of the form $a a$, $a b$, $b a$ and $b b$. We use three properties of the real numbers, all for real numbers $x$, $y$ and $z$: the left distributive law $x (y + z) = x y + x z$, the right distributive law $(y + z) x = y x + z x$, and commutativity of multiplication $x y = y x$. The left distributive law is an axiom of the real numbers, and the right one follows from it and commutativity, since $(y + z) x = x (y + z) = x y + x z = y x + z x$. Apply the left distributive law with $x = a + b$, $y = a$ and $z = b$ to get $(a + b)(a + b) = (a + b) a + (a + b) b$, a sum of two products that each have a sum as one factor. Expand the first summand with the right distributive law, $(a + b) a = a a + b a$, and the second in the same way, $(a + b) b = a b + b b$. Substituting both back gives $(a + b)^2 = a a + b a + a b + b b$, a sum of four products. Addition is associative and commutative, so these four terms may be grouped in any order without changing the sum, and grouping the two middle terms together is what the next step needs. By the definition of the square, $a a = a^2$ and $b b = b^2$. By commutativity of multiplication, $b a = a b$, so the two middle terms are both equal to $a b$, and their sum $a b + a b$ is $2 a b$: for any real number $t$, the right distributive law gives $t + t = (1 + 1) t = 2 t$, because $1 + 1 = 2$ in the real numbers. Substituting, $(a + b)^2 = a^2 + 2 a b + b^2$. As a check on the arithmetic, take $a = 1$ and $b = 2$. The left side is $(1 + 2)^2 = 9$, and the right side is $1^2 + 2 \cdot 1 \cdot 2 + 2^2 = 1 + 4 + 4 = 9$, which agrees. The identity holds for every pair of real numbers, because the argument used only the distributive laws, commutativity of multiplication, associativity of addition and the numeral fact $1 + 1 = 2$. Commutativity is the one step that a noncommutative ring would not supply: for square matrices $b a$ and $a b$ need not be equal, so the middle term becomes $a b + b a$ rather than $2 a b$, and the identity fails in general. ∎",
                expect: Some(Status::Verified),
            },
        ],
    },
];

/// The long proof must stay one paragraph of at least 2000 bytes, so the live test exercises a long
/// block.
const _: () = assert!(
    LIVE_DOCUMENTS[2].lines[1].text.len() >= 2000,
    "the long proof is shorter than 2000 bytes"
);

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

/// What `drive_observed` reports: each job before it is submitted, and each reply before it is applied.
#[allow(dead_code)] // The scripted test drives without an observer, so the payloads go unread there.
pub enum Observed<'a> {
    Job(&'a Job),
    Reply(&'a Reply),
}

/// Feeds the lines to `mls` as closed paragraphs and runs every job it asks for, in order, until
/// it needs nothing more.
#[allow(dead_code)] // Used by the scripted test; the live test passes an observer instead.
pub fn drive(mls: &mut Mls, worker: &Worker, lines: &[&str]) -> Result<(), String> {
    drive_observed(mls, worker, lines, &mut |_| {})
}

/// `drive`, calling `observe` with every job and reply along the way.
pub fn drive_observed(
    mls: &mut Mls,
    worker: &Worker,
    lines: &[&str],
    observe: &mut dyn FnMut(Observed<'_>),
) -> Result<(), String> {
    let paragraphs: Vec<Paragraph> = lines
        .iter()
        .map(|text| Paragraph {
            text: text.to_string(),
            open: false,
        })
        .collect();
    mls.sync(&paragraphs);
    while let Some(job) = mls.next_job() {
        observe(Observed::Job(&job));
        worker.submit(job);
        let reply = worker
            .rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| "no reply within 10 minutes".to_string())?;
        observe(Observed::Reply(&reply));
        if !mls.apply(reply) {
            return Err("a reply was stale".to_string());
        }
    }
    Ok(())
}

/// The status of the dot at each line of `document` (None where there is no dot).
#[allow(dead_code)] // Used by the scripted test; the live test compares its own paragraphs.
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
#[allow(dead_code)] // Used by the scripted test; the live test compares its own paragraphs.
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
#[allow(dead_code)] // Used by the scripted test; the live test compares its own paragraphs.
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
