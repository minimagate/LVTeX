use super::*;
use crate::document::ATOM;

fn letters(s: &Span) -> String {
    let st = s.style;
    [
        (st.bold, 'b'),
        (st.italic, 'i'),
        (st.code, 'c'),
        (st.strike, 's'),
        (st.link, 'l'),
        (s.marker, 'm'),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, c)| c)
    .collect()
}

fn assert_covers(text: &str) {
    let spans = inline(text);
    let mut pos = 0;
    for s in &spans {
        assert!(!s.range.is_empty(), "inline({text:?}): empty span {s:?}");
        assert_eq!(s.range.start, pos, "inline({text:?}): spans not contiguous");
        pos = s.range.end;
    }
    assert_eq!(
        pos,
        text.len(),
        "inline({text:?}): spans do not cover the text"
    );
}

fn check_spans(text: &str, expected: &[(&str, &str)]) {
    assert_covers(text);
    let got: Vec<(&str, String)> = inline(text)
        .iter()
        .map(|s| (&text[s.range.clone()], letters(s)))
        .collect();
    let want: Vec<(&str, String)> = expected.iter().map(|(t, f)| (*t, f.to_string())).collect();
    assert_eq!(got, want, "inline({text:?})");
}

#[test]
fn block_markers() {
    let cases: &[(&str, Block, &str)] = &[
        ("# Title", Block::Heading(1), "# "),
        ("   # Title", Block::Heading(1), "# "),
        ("    # Title", Block::Paragraph, ""),
        ("#hashtag", Block::Paragraph, ""),
        ("## Sub", Block::Heading(2), "## "),
        ("####### x", Block::Paragraph, ""),
        ("#", Block::Heading(1), "#"),
        ("#\tTitle", Block::Heading(1), "#\t"),
        ("- item", Block::Bullet, "- "),
        ("  - nested", Block::Bullet, "- "),
        ("    - deeper", Block::Bullet, "- "),
        ("* item", Block::Bullet, "* "),
        ("+ item", Block::Bullet, "+ "),
        ("-item", Block::Paragraph, ""),
        ("- [ ] todo", Block::Task(false), "- [ ] "),
        ("- [x] done", Block::Task(true), "- [x] "),
        ("- [X] done", Block::Task(true), "- [X] "),
        ("- [ ]", Block::Task(false), "- [ ]"),
        ("- [ ]x", Block::Bullet, "- "),
        ("1. one", Block::Ordered(1), "1. "),
        ("12) twelve", Block::Ordered(12), "12) "),
        ("1234567890. x", Block::Paragraph, ""),
        ("1.5 apples", Block::Paragraph, ""),
        ("> quote", Block::Quote, "> "),
        (">quote", Block::Quote, ">"),
        ("---", Block::Rule, "---"),
        ("***", Block::Rule, "***"),
        ("* * *", Block::Rule, "* * *"),
        ("- - -", Block::Rule, "- - -"),
        ("   ___", Block::Rule, "___"),
        ("    ---", Block::Paragraph, ""),
        ("--", Block::Paragraph, ""),
        ("-- x", Block::Paragraph, ""),
        ("", Block::Paragraph, ""),
    ];
    for &(text, block, marker) in cases {
        let l = line(text);
        assert_eq!(l.block, block, "line({text:?}).block");
        assert_eq!(&text[l.marker.clone()], marker, "line({text:?}).marker");
        assert_eq!(l.content(), l.marker.end, "line({text:?}).content");
    }
}

#[test]
fn enter_at_line_end() {
    use Enter::{Continue, End};
    let cases: &[(&str, Option<Enter>)] = &[
        ("- a", Some(Continue("- ".into()))),
        ("* a", Some(Continue("* ".into()))),
        ("+ a", Some(Continue("+ ".into()))),
        ("  - nested", Some(Continue("  - ".into()))),
        ("1. a", Some(Continue("2. ".into()))),
        ("9) a", Some(Continue("10) ".into()))),
        ("- [x] done", Some(Continue("- [ ] ".into()))),
        ("- [ ] a", Some(Continue("- [ ] ".into()))),
        ("> a", Some(Continue("> ".into()))),
        (">a", Some(Continue(">".into()))),
        ("- ", Some(End)),
        ("1. ", Some(End)),
        ("- [ ] ", Some(End)),
        ("> ", Some(End)),
        ("-   ", Some(End)),
        ("  - ", Some(End)),
        ("# Title", None),
        ("plain", None),
        ("---", None),
    ];
    for (text, want) in cases {
        let at_end = enter(text, text.len());
        assert_eq!(&at_end, want, "enter({text:?}, end)");
    }
    assert_eq!(enter("  - a", 1), None, "caret inside the indent");
    assert_eq!(enter("- a", 1), None, "caret inside the marker");
    assert_eq!(enter("1. a", 2), None, "caret inside the number");
    assert_eq!(
        enter("- ab", 3),
        Some(Continue("- ".into())),
        "caret in the middle of the content"
    );
}

#[test]
fn enter_edit_ranges() {
    let text = "x\n- a\ny";
    assert_eq!(
        enter_edit(text, 5),
        Some((5..5, "\n- ".to_string())),
        "continue at the end of the middle line"
    );
    assert_eq!(enter_edit(text, 0), None, "paragraph above the list");
    let empty = "- a\n- \nz";
    assert_eq!(
        enter_edit(empty, 6),
        Some((4..6, String::new())),
        "end removes the empty item's whole line"
    );
}

#[test]
fn inline_spans() {
    let cases: &[(&str, &[(&str, &str)])] = &[
        ("**bold**", &[("**", "bm"), ("bold", "b"), ("**", "bm")]),
        ("*it*", &[("*", "im"), ("it", "i"), ("*", "im")]),
        ("_it_", &[("_", "im"), ("it", "i"), ("_", "im")]),
        (
            "***both***",
            &[("***", "bim"), ("both", "bi"), ("***", "bim")],
        ),
        (
            "**bold *both* bold**",
            &[
                ("**", "bm"),
                ("bold ", "b"),
                ("*", "bim"),
                ("both", "bi"),
                ("*", "bim"),
                (" bold", "b"),
                ("**", "bm"),
            ],
        ),
        (
            "**a *b** c*",
            &[("**", "bm"), ("a *b", "b"), ("**", "bm"), (" c*", "")],
        ),
        (
            "a*b*c",
            &[("a", ""), ("*", "im"), ("b", "i"), ("*", "im"), ("c", "")],
        ),
        ("snake_case_name", &[("snake_case_name", "")]),
        ("2 * 3 * 4", &[("2 * 3 * 4", "")]),
        ("*unclosed", &[("*unclosed", "")]),
        (
            "`code *not* em`",
            &[("`", "cm"), ("code *not* em", "c"), ("`", "cm")],
        ),
        ("``a`b``", &[("``", "cm"), ("a`b", "c"), ("``", "cm")]),
        (
            "a\\b *it*",
            &[("a\\b ", ""), ("*", "im"), ("it", "i"), ("*", "im")],
        ),
        ("C:\\path", &[("C:\\path", "")]),
        ("~~gone~~", &[("~~", "sm"), ("gone", "s"), ("~~", "sm")]),
        (
            "[link](http://x.y)",
            &[("[", "lm"), ("link", "l"), ("](http://x.y)", "lm")],
        ),
        ("[text]", &[("[text]", "")]),
        ("**é**", &[("**", "bm"), ("é", "b"), ("**", "bm")]),
        ("*日本*", &[("*", "im"), ("日本", "i"), ("*", "im")]),
    ];
    for (text, expected) in cases {
        check_spans(text, expected);
    }
}

#[test]
fn formula_atom_is_ordinary_text() {
    let spaced = format!("**area {ATOM}**");
    let inner = format!("area {ATOM}");
    check_spans(
        &spaced,
        &[("**", "bm"), (inner.as_str(), "b"), ("**", "bm")],
    );
    let bare = format!("**{ATOM}**");
    check_spans(&bare, &[("**", "bm"), (ATOM, "b"), ("**", "bm")]);
}

#[test]
fn edge_inputs_cover_text_without_panic() {
    let inputs = [
        "", "\\", "`", "``", "````", "[", "]", "](", "[]()", "[a](", "[a]()", "~", "~~", "~~~",
        "*", "***", "****", "_*_", "__", "é*", "*é", "\\é", "a\\", "\\\\", "*[a*](u)",
    ];
    for text in inputs {
        assert_covers(text);
        line(text);
    }
}

#[test]
fn long_line_stays_linear() {
    let text = "*a* ".repeat(20_000);
    let spans = inline(&text);
    assert_eq!(spans.last().map(|s| s.range.end), Some(text.len()));
    assert_covers(&text);
}
