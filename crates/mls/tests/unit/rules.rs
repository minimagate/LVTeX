use super::*;
use crate::{Formal, Label};

type Case<'a> = (
    Label,
    &'a str,
    &'a [&'a str],
    Option<&'a str>,
    Option<&'a str>,
);

const EMPTY: &str = "The formalization is empty.";
const AXIOM: &str = "The Lean declares an axiom.";
const ADMIT: &str = "The Lean uses admit.";
const STATEMENT_SORRY: &str = "A statement must have `sorry` as its whole proof.";
const DEFINITION_SORRY: &str = "The definition uses sorry.";
const PROOF_SORRY: &str = "The proof uses sorry.";
const CHANGED: &str = "The proof changes the statement above.";
const CITES: &str = "The proof cites the statement it proves.";

fn formal(lean: &str) -> Formal {
    Formal {
        lean: lean.to_string(),
        lean_name: None,
        assumptions: Vec::new(),
        uses: Vec::new(),
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn check_cases(cases: &[Case<'_>]) {
    for &(label, lean, above, statement, expected) in cases {
        let got = reject(label, &formal(lean), &names(above), statement).err();
        assert_eq!(got.as_deref(), expected, "{label:?}: {lean:?}");
    }
}

#[test]
fn strip_comments_removes_line_block_and_doc_comments() {
    let cases = [
        ("a /- one /- two -/ still -/ b", "a b"),
        ("/-- the successor -/\ndef f := 1", "def f := 1"),
        ("/- -- not a line -/ x", "x"),
        ("-- /- still a line\ny", "y"),
        ("a/-x-/b", "a b"),
        ("no comments here", "no comments here"),
    ];
    for (input, expected) in cases {
        assert_eq!(normalize(&strip_comments(input)), expected, "{input:?}");
    }
}

#[test]
fn strip_comments_keeps_the_newline_after_a_line_comment() {
    assert_eq!(strip_comments("x -- note\ny"), "x \ny");
}

#[test]
fn strip_comments_copies_string_and_char_literals_verbatim() {
    let cases = [
        ("\"a -- b\" c", "\"a -- b\" c"),
        ("\"/- open\" x", "\"/- open\" x"),
        ("\"a \\\" -- b\" c", "\"a \\\" -- b\" c"),
        ("'\"' -- note\nx", "'\"' x"),
        ("\"unclosed -- x", "\"unclosed -- x"),
    ];
    for (input, expected) in cases {
        assert_eq!(normalize(&strip_comments(input)), expected, "{input:?}");
    }
}

#[test]
fn declared_names_reads_declarations_past_modifiers_and_comments() {
    let cases: &[(&str, &[&str])] = &[
        ("def f (n : ℕ) : ℕ := n", &["f"]),
        ("noncomputable def g := 1", &["g"]),
        ("private def h := 1", &["h"]),
        ("@[simp]\ndef k := 1", &["k"]),
        ("theorem t : True := trivial", &["t"]),
        ("lemma l' : True := trivial", &["l'"]),
        ("instance foo : Inhabited Nat := ⟨0⟩", &["foo"]),
        ("instance : Inhabited Nat := ⟨0⟩", &[]),
        ("structure Point where\n  x : Nat", &["Point"]),
        ("class inductive Shape", &["Shape"]),
        ("inductive Color", &["Color"]),
        (
            "abbrev ab := 1\nopaque o : Nat\naxiom ax : True",
            &["ab", "o", "ax"],
        ),
        ("-- def fake\ntheorem real : True := trivial", &["real"]),
        ("/-- def doc -/ def d := 1", &["d"]),
        ("def x.y := 1", &["x.y"]),
        ("def a := 1\ndef a := 2", &["a"]),
        ("mydef x := 1 -- def z", &[]),
    ];
    for &(lean, expected) in cases {
        assert_eq!(declared_names(lean), names(expected), "{lean:?}");
    }
}

#[test]
fn declared_names_handles_a_large_document() {
    let lean: String = (0..2000).map(|n| format!("def d{n} := {n}\n")).collect();
    let declared = declared_names(&lean);
    assert_eq!(declared.len(), 2000);
    assert_eq!(declared.last().map(String::as_str), Some("d1999"));
}

#[test]
fn rejects_empty_statements_and_proofs_but_not_a_definition_that_declares_nothing() {
    check_cases(&[
        (Label::Proof, "", &[], None, Some(EMPTY)),
        (
            Label::Statement,
            "-- only a comment\n",
            &[],
            None,
            Some(EMPTY),
        ),
        (Label::Definition, " \n\t", &[], None, None),
        (Label::Definition, "-- only a comment\n", &[], None, None),
        (Label::Definition, "/- only -/", &[], None, None),
    ]);
}

#[test]
fn rejects_axiom_and_admit_only_as_whole_words() {
    check_cases(&[
        (Label::Definition, "axiom a : True", &[], None, Some(AXIOM)),
        (
            Label::Proof,
            "theorem t : True := by admit",
            &[],
            None,
            Some(ADMIT),
        ),
        (
            Label::Proof,
            "theorem t : True := admit_lemma",
            &[],
            None,
            None,
        ),
        (Label::Definition, "def axiomatic := 1", &[], None, None),
        (
            Label::Definition,
            "-- axiom here\ndef d := 1",
            &[],
            None,
            None,
        ),
    ]);
}

#[test]
fn statement_must_end_with_sorry_as_its_whole_proof() {
    check_cases(&[
        (
            Label::Statement,
            "theorem t : True := by sorry",
            &[],
            None,
            None,
        ),
        (
            Label::Statement,
            "theorem t : True := by sorry -- done",
            &[],
            None,
            None,
        ),
        (
            Label::Statement,
            "theorem t : True := by\n  -- sorry\n  sorry",
            &[],
            None,
            None,
        ),
        (
            Label::Statement,
            "theorem t : True := by trivial",
            &[],
            None,
            Some(STATEMENT_SORRY),
        ),
        (
            Label::Statement,
            "theorem t : True := by mysorry",
            &[],
            None,
            Some(STATEMENT_SORRY),
        ),
        (
            Label::Statement,
            "theorem t : True := by sorry\ndef q := sorry",
            &[],
            None,
            Some(STATEMENT_SORRY),
        ),
        (
            Label::Statement,
            "theorem t : True := by sorry\n#check t",
            &[],
            None,
            Some(STATEMENT_SORRY),
        ),
    ]);
}

#[test]
fn definitions_and_proofs_must_not_use_sorry() {
    check_cases(&[
        (
            Label::Definition,
            "def d : Nat := sorry",
            &[],
            None,
            Some(DEFINITION_SORRY),
        ),
        (Label::Definition, "def d : Nat := mysorry", &[], None, None),
        (Label::Definition, "def sorry_helper := 1", &[], None, None),
        (
            Label::Definition,
            "-- sorry\ndef d : Nat := 1",
            &[],
            None,
            None,
        ),
        (
            Label::Proof,
            "theorem t : True := by sorry",
            &[],
            None,
            Some(PROOF_SORRY),
        ),
        (Label::Skip, "def s := sorry", &[], None, Some(PROOF_SORRY)),
    ]);
}

#[test]
fn redefinitions_are_rejected_unless_a_proof_restates_its_statement() {
    let statement = Some("theorem t : True := by sorry");
    check_cases(&[
        (
            Label::Definition,
            "def f := 2",
            &["f"],
            None,
            Some("`f` is already defined above."),
        ),
        (Label::Definition, "def g := 2", &["f"], None, None),
        (
            Label::Proof,
            "theorem t : True := by\n  trivial",
            &["t"],
            statement,
            None,
        ),
        (
            Label::Proof,
            "theorem t : True := by trivial",
            &["t"],
            None,
            Some("`t` is already defined above."),
        ),
        (
            Label::Proof,
            "def f := 1\ntheorem t : True := by trivial",
            &["f", "t"],
            statement,
            Some("`f` is already defined above."),
        ),
    ]);
}

#[test]
fn proof_must_keep_the_header_of_its_statement() {
    let statement = Some("theorem t (n : ℕ) : n + 0 = n := by sorry");
    check_cases(&[
        (
            Label::Proof,
            "theorem t (n : ℕ) : n + 0 = n := by\n  simp",
            &[],
            statement,
            None,
        ),
        (
            Label::Proof,
            "theorem t (n : ℕ) : n = n := by rfl",
            &[],
            statement,
            Some(CHANGED),
        ),
        (
            Label::Proof,
            "theorem t : True := trivial",
            &[],
            Some("theorem t : True := by sorry"),
            None,
        ),
        (
            Label::Proof,
            "theorem t : True := by trivial",
            &[],
            Some("theorem t : True := sorry"),
            None,
        ),
        (
            Label::Proof,
            "theorem t :\n  True := by\n  trivial",
            &[],
            Some("theorem t /- note -/ : True := by sorry"),
            None,
        ),
        (
            Label::Proof,
            "theorem t : True := by trivial",
            &[],
            Some("by sorry"),
            Some(CHANGED),
        ),
        (
            Label::Proof,
            "theorem t2 : True := by trivial",
            &[],
            Some("theorem t : True := by sorry"),
            Some(CHANGED),
        ),
    ]);
}

#[test]
fn a_proof_may_not_cite_its_own_statement() {
    let statement = Some("theorem t (n : ℕ) : n + 0 = n := by sorry");
    check_cases(&[
        (
            Label::Proof,
            "theorem t (n : ℕ) : n + 0 = n := by\n  simp",
            &[],
            statement,
            None,
        ),
        (
            Label::Proof,
            "theorem t (n : ℕ) : n + 0 = n := by\n  exact t n",
            &[],
            statement,
            Some(CITES),
        ),
        (
            Label::Proof,
            "theorem t (n : ℕ) : n + 0 = n := by\n  exact t_aux n",
            &["t_aux"],
            statement,
            None,
        ),
        (
            Label::Proof,
            "theorem t (n : ℕ) : n + 0 = n := by\n  -- t n\n  simp",
            &[],
            statement,
            None,
        ),
    ]);
}

#[test]
fn an_axiom_after_a_string_that_looks_like_a_comment_is_rejected() {
    check_cases(&[(
        Label::Definition,
        "def s : String := \"/-\"\naxiom bogus : False\ndef e : String := \"-/\"",
        &[],
        None,
        Some(AXIOM),
    )]);
}

#[test]
fn a_string_literal_is_not_a_comment_in_a_statement_header() {
    check_cases(&[(
        Label::Proof,
        "theorem foo (n : ℕ) : n = 0 ∨ (\"--\" = \"--\" ∧ True) := by simp",
        &[],
        Some("theorem foo (n : ℕ) : n = 0 ∨ (\"--\" = \"--\" ∧ n = 1)\n  := by sorry"),
        Some(CHANGED),
    )]);
}

#[test]
fn a_string_literal_is_not_a_comment_in_a_proof_header() {
    check_cases(&[(
        Label::Proof,
        "theorem t : \"a\" = \" := by trivial\"",
        &[],
        Some("theorem t : \"a\" = \"--\"\n  := by sorry"),
        Some(CHANGED),
    )]);
}

#[test]
fn a_quote_char_literal_does_not_open_a_string() {
    check_cases(&[(
        Label::Definition,
        "def c : Char := '\"'\n-- axiom here\ndef d : Nat := 1",
        &[],
        None,
        None,
    )]);
}

#[test]
fn rejects_commands_that_run_code_or_change_limits() {
    check_cases(&[
        (
            Label::Definition,
            "def d := 1\n#eval d",
            &[],
            None,
            Some("The Lean uses `#eval`, which is not allowed."),
        ),
        (
            Label::Definition,
            "run_cmd logInfo \"hi\"",
            &[],
            None,
            Some("The Lean uses `run_cmd`, which is not allowed."),
        ),
        (
            Label::Definition,
            "set_option maxHeartbeats 0 in\ndef d := 1",
            &[],
            None,
            Some("The Lean uses `set_option`, which is not allowed."),
        ),
        (
            Label::Definition,
            "unsafe def d := 1",
            &[],
            None,
            Some("The Lean uses `unsafe`, which is not allowed."),
        ),
        (
            Label::Proof,
            "theorem t : True := by native_decide",
            &[],
            None,
            Some("The Lean uses `native_decide`, which is not allowed."),
        ),
        (Label::Definition, "def xs := #[1, 2]", &[], None, None),
        (
            Label::Definition,
            "-- macro here\ndef d := 1",
            &[],
            None,
            None,
        ),
    ]);
}

#[test]
fn uses_lists_the_names_above_that_the_lean_relies_on() {
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "theorem t4 : d1 = d1 := trivial",
            &["IsEven", "IsEvenNat", "d1"],
            &["d1"],
        ),
        (
            "theorem t4 : IsEvenNat 2 := trivial",
            &["IsEven", "IsEvenNat"],
            &["IsEvenNat"],
        ),
        (
            "-- d1 is a comment\ntheorem t4 : True := trivial",
            &["d1"],
            &[],
        ),
        ("/- t2 -/ theorem t4 : True := trivial", &["t2"], &[]),
        ("theorem t2 : t3 = t3 := trivial", &["t2", "t3"], &["t3"]),
        ("theorem t2 : True := by trivial", &["t2"], &[]),
        ("def x := d1 + t3", &["t3", "d1"], &["t3", "d1"]),
        ("def x := d1", &["d1", "d1"], &["d1"]),
    ];
    for &(lean, above, expected) in cases {
        assert_eq!(uses(lean, &names(above)), names(expected), "{lean:?}");
    }
}

#[test]
fn as_example_turns_the_declaration_of_the_name_into_an_example() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "theorem t (n : ℕ) : n = n := by rfl",
            "t",
            "example (n : ℕ) : n = n := by rfl",
        ),
        (
            "lemma t (n : ℕ) : n = n := by rfl",
            "t",
            "example (n : ℕ) : n = n := by rfl",
        ),
        (
            "theorem t_aux : True := trivial\n\ntheorem t : True := by\n  exact t_aux",
            "t",
            "theorem t_aux : True := trivial\n\nexample : True := by\n  exact t_aux",
        ),
        (
            "theorem tt : True := trivial",
            "t",
            "theorem tt : True := trivial",
        ),
        ("theorem u : t = t := rfl", "t", "theorem u : t = t := rfl"),
        (
            "my_theorem t : True := trivial",
            "t",
            "my_theorem t : True := trivial",
        ),
        (
            "theorem t : True := trivial",
            "",
            "theorem t : True := trivial",
        ),
    ];
    for &(lean, name, expected) in cases {
        assert_eq!(as_example(lean, name), expected, "{lean:?} for {name:?}");
    }
}

#[test]
fn counterexample_reads_a_refutation_of_the_named_statement() {
    let refutation = "-- Counterexample: p = 3, where p - 1 = 2 divides 2, so S_2 = -1.\n\
                      example : ¬ type_of% @X := by\n  intro h\n  exact h";
    let sentence = Some("p = 3, where p - 1 = 2 divides 2, so S_2 = -1.");
    let cases: &[(&str, &str, Option<&str>)] = &[
        (refutation, "X", sentence),
        (
            "\n\n  -- Counterexample: n = 0.\nexample : ¬ type_of% @X := by\n  intro h",
            "X",
            Some("n = 0."),
        ),
        (
            "-- Counterexample: n = 0.\nexample :\n  ¬  type_of%   @X :=\n  by intro h",
            "X",
            Some("n = 0."),
        ),
        ("example : ¬ type_of% @X := by\n  intro h", "X", None),
        (
            "-- Counterexample:  \nexample : ¬ type_of% @X := by",
            "X",
            None,
        ),
        (refutation, "Y", None),
        (refutation, "X2", None),
        (
            "-- Counterexample: n = 0.\ntheorem X : True := by\n  intro h",
            "X",
            None,
        ),
        (
            "example : ¬ type_of% @X := by\n-- Counterexample: n = 0.",
            "X",
            None,
        ),
    ];
    for &(lean, name, expected) in cases {
        assert_eq!(
            counterexample(lean, name).as_deref(),
            expected,
            "{lean:?} for {name:?}"
        );
    }
}

#[test]
fn reject_refutation_accepts_a_refutation_of_its_statement_and_nothing_else() {
    let statement = "theorem X (p : ℕ) : p = 0 := by sorry";
    let refutation = "-- Counterexample: p = 3, where p - 1 = 2 divides 2, so S_2 = -1.\n\
                      example : ¬ type_of% @X := by\n  intro h\n  have := @h 3 ⟨by norm_num⟩ ⟨1, rfl⟩\n  revert this\n  decide";
    assert_eq!(reject_refutation(refutation, statement), Ok(()));

    let cases = [
        (refutation.replace("decide", "sorry"), PROOF_SORRY),
        (refutation.replace("decide", "exact foo axiom"), AXIOM),
        (
            refutation.replacen(
                "-- Counterexample: p = 3, where p - 1 = 2 divides 2, so S_2 = -1.\n",
                "",
                1,
            ),
            "The refutation does not have the required form.",
        ),
        (
            format!("{refutation}\n\ntheorem extra : True := trivial"),
            "The refutation declares a name.",
        ),
    ];
    for (lean, message) in cases {
        assert_eq!(
            reject_refutation(&lean, statement),
            Err(message.to_string()),
            "{lean}"
        );
    }
}
