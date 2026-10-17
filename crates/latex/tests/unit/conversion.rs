use super::*;

#[test]
fn nesting_limit_is_64_braces() {
    let nested = |n: usize| format!("{}x{}", "{".repeat(n), "}".repeat(n));
    assert!(convert(&nested(64)).is_ok());
    assert_eq!(convert(&nested(65)).unwrap_err(), TOO_DEEP);
}
#[test]
fn substitute_symbols_replaces_whole_control_words() {
    assert_eq!(substitute_symbols(r"a \oplus b"), "a ⊕ b");
    assert_eq!(substitute_symbols(r"\oplus_1"), "⊕_1");
    assert_eq!(substitute_symbols(r"\oplusx"), r"\oplusx");
    assert_eq!(substitute_symbols(r"\\oplus"), r"\\oplus");
    assert_eq!(substitute_symbols(r"\\\oplus"), "\\\\⊕");
    assert_eq!(substitute_symbols(r"\bigdot"), "\u{2A00}");
    assert_eq!(substitute_symbols(r"% \oplus"), r"% \oplus");
    assert_eq!(substitute_symbols(r"\%\oplus"), "\\%⊕");
    assert!(matches!(substitute_symbols(r"\alpha"), Cow::Borrowed(_)));
    assert_eq!(substitute_symbols(r"\preceqq\cdots\preceqq"), "⪳\\cdots⪳");
    assert_eq!(substitute_symbols(r"\not\gtrapprox"), r"\not⪆");
}
