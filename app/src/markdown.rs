//! Markdown line structure: block markers and inline styles, in byte ranges of one line.
#![allow(dead_code)] // Used by layout and export in follow-up changes.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph,
    Heading(u8),
    Bullet,
    Ordered(u32),
    Task(bool),
    Quote,
    Rule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub block: Block,
    /// The block marker bytes including its trailing space; empty for a paragraph.
    pub marker: Range<usize>,
}

impl Line {
    /// First byte of the inline content.
    pub fn content(&self) -> usize {
        self.marker.end
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
    /// A syntax character, drawn dimmed.
    pub marker: bool,
}

/// Block structure of one line. `text` must not contain `\n`.
pub fn line(text: &str) -> Line {
    let indent = text.len() - text.trim_start_matches(' ').len();
    let rest = &text[indent..];
    let shallow = indent <= 3;
    let found = match rest.as_bytes().first() {
        _ if shallow && is_rule(rest) => Some((Block::Rule, rest.len())),
        Some(b'#') if shallow => heading(rest),
        Some(b'-' | b'*' | b'+') => bullet(rest),
        Some(b'0'..=b'9') => ordered(rest),
        Some(b'>') if shallow => Some((
            Block::Quote,
            1 + usize::from(rest.as_bytes().get(1) == Some(&b' ')),
        )),
        _ => None,
    };
    match found {
        Some((block, len)) => Line {
            block,
            marker: indent..indent + len,
        },
        None => Line {
            block: Block::Paragraph,
            marker: 0..0,
        },
    }
}

fn is_rule(rest: &str) -> bool {
    let mut marks = rest.chars().filter(|&c| c != ' ');
    let Some(first) = marks.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_') && marks.clone().count() >= 2 && marks.all(|c| c == first)
}

fn heading(rest: &str) -> Option<(Block, usize)> {
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let level = Block::Heading(hashes as u8);
    match rest.as_bytes().get(hashes) {
        None => Some((level, hashes)),
        Some(b' ' | b'\t') => Some((level, hashes + 1)),
        Some(_) => None,
    }
}

fn bullet(rest: &str) -> Option<(Block, usize)> {
    let body = rest.get(1..)?.strip_prefix(' ')?;
    let task = match body.get(..3) {
        Some("[ ]") => Some(false),
        Some("[x]" | "[X]") => Some(true),
        _ => None,
    }
    .filter(|_| body.len() == 3 || body.as_bytes()[3] == b' ');
    Some(match task {
        Some(checked) => (Block::Task(checked), 2 + body.len().min(4)),
        None => (Block::Bullet, 2),
    })
}

fn ordered(rest: &str) -> Option<(Block, usize)> {
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=9).contains(&digits) {
        return None;
    }
    let number: u32 = rest[..digits].parse().ok()?;
    match rest.as_bytes().get(digits..digits + 2) {
        Some([b'.' | b')', b' ']) => Some((Block::Ordered(number), digits + 2)),
        _ => None,
    }
}

/// Inline spans of one line's content. Pass the content slice (`&line[l.content()..]`); the
/// returned ranges are byte offsets into `text`.
pub fn inline(text: &str) -> Vec<Span> {
    let mut intervals = Vec::new();
    scan(text, 0..text.len(), &mut intervals);
    // Turn interval starts and ends into a running count per byte, then read it at char starts.
    let mut cover = vec![[0i32; 6]; text.len() + 1];
    for (range, flags) in &intervals {
        for (k, on) in flags.iter().enumerate() {
            if *on {
                cover[range.start][k] += 1;
                cover[range.end][k] -= 1;
            }
        }
    }
    let mut acc = [0i32; 6];
    for c in &mut cover {
        for (a, d) in acc.iter_mut().zip(c.iter_mut()) {
            *a += *d;
            *d = *a;
        }
    }
    let mut spans: Vec<Span> = Vec::new();
    for (start, ch) in text.char_indices() {
        let c = cover[start];
        let style = Style {
            bold: c[0] > 0,
            italic: c[1] > 0,
            code: c[2] > 0,
            strike: c[3] > 0,
            link: c[4] > 0,
        };
        let marker = c[5] > 0;
        let end = start + ch.len_utf8();
        match spans.last_mut() {
            Some(last) if last.style == style && last.marker == marker => last.range.end = end,
            _ => spans.push(Span {
                range: start..end,
                style,
                marker,
            }),
        }
    }
    spans
}

/// Style flags in the order bold, italic, code, strike, link, marker.
type Flags = [bool; 6];
type Intervals = Vec<(Range<usize>, Flags)>;

struct Delim {
    ch: u8,
    len: usize,
    start: usize,
}

fn flags(style: Style, marker: bool) -> Flags {
    [
        style.bold,
        style.italic,
        style.code,
        style.strike,
        style.link,
        marker,
    ]
}

/// Scans one scope (the whole line, or the text of a link). Emphasis pairs only within a scope,
/// so a link's text never pairs with delimiters outside it.
fn scan(text: &str, scope: Range<usize>, out: &mut Intervals) {
    let bytes = text.as_bytes();
    let mut stack: Vec<Delim> = Vec::new();
    let mut i = scope.start;
    while i < scope.end {
        match bytes[i] {
            b'\\' if i + 1 < scope.end && bytes[i + 1].is_ascii_punctuation() => {
                out.push((i..i + 1, flags(Style::default(), true)));
                i += 2;
            }
            b'`' => {
                let n = run_len(bytes, i, scope.end, b'`');
                if let Some(close) = closing_run(bytes, i + n, scope.end, n) {
                    let code = Style {
                        code: true,
                        ..Style::default()
                    };
                    out.push((i..i + n, flags(code, true)));
                    out.push((i + n..close, flags(code, false)));
                    out.push((close..close + n, flags(code, true)));
                    i = close + n;
                } else {
                    i += n;
                }
            }
            b'[' => match link_end(bytes, i, scope.end) {
                Some((close, end)) => {
                    let link = Style {
                        link: true,
                        ..Style::default()
                    };
                    out.push((i..end, flags(link, false)));
                    out.push((i..i + 1, flags(link, true)));
                    out.push((close..end, flags(link, true)));
                    scan(text, i + 1..close, out);
                    i = end;
                }
                None => i += 1,
            },
            b @ (b'*' | b'_' | b'~') => {
                let n = run_len(bytes, i, scope.end, b);
                let valid = if b == b'~' {
                    n == 2
                } else {
                    (1..=3).contains(&n)
                };
                if valid {
                    delimiter(text, i..i + n, &mut stack, out);
                }
                i += n;
            }
            _ => i += 1,
        }
    }
}

/// Emphasis or strikethrough run: closes the nearest open run of the same char and length, or
/// opens a new one. Unmatched runs stay plain text.
fn delimiter(text: &str, run: Range<usize>, stack: &mut Vec<Delim>, out: &mut Intervals) {
    let ch = text.as_bytes()[run.start];
    let n = run.len();
    let before = text[..run.start].chars().next_back();
    let after = text[run.end..].chars().next();
    let can_open = after.is_some_and(|c| !c.is_whitespace())
        && !(ch == b'_' && before.is_some_and(char::is_alphanumeric));
    let can_close = before.is_some_and(|c| !c.is_whitespace())
        && !(ch == b'_' && after.is_some_and(char::is_alphanumeric));
    let closer = if can_close {
        stack.iter().rposition(|d| d.ch == ch && d.len == n)
    } else {
        None
    };
    if let Some(k) = closer {
        let open = stack[k].start;
        stack.truncate(k);
        let style = delim_style(ch, n);
        out.push((open..run.end, flags(style, false)));
        out.push((open..open + n, flags(style, true)));
        out.push((run.clone(), flags(style, true)));
    } else if can_open {
        stack.push(Delim {
            ch,
            len: n,
            start: run.start,
        });
    }
}

fn delim_style(ch: u8, n: usize) -> Style {
    if ch == b'~' {
        Style {
            strike: true,
            ..Style::default()
        }
    } else {
        Style {
            bold: n >= 2,
            italic: n != 2,
            ..Style::default()
        }
    }
}

/// The `]` that closes a link opened at `open`, and the byte after its `)`.
fn link_end(bytes: &[u8], open: usize, end: usize) -> Option<(usize, usize)> {
    let mut j = open + 1;
    while j < end {
        match bytes[j] {
            b'\\' if j + 1 < end && bytes[j + 1].is_ascii_punctuation() => j += 2,
            b']' => {
                if j + 1 >= end || bytes[j + 1] != b'(' {
                    return None;
                }
                let close = bytes[j + 2..end].iter().position(|&b| b == b')')?;
                return Some((j, j + 2 + close + 1));
            }
            _ => j += 1,
        }
    }
    None
}

fn run_len(bytes: &[u8], at: usize, end: usize, ch: u8) -> usize {
    bytes[at..end].iter().take_while(|&&b| b == ch).count()
}

/// Start of the next backtick run of exactly `n` backticks, if any.
fn closing_run(bytes: &[u8], mut at: usize, end: usize, n: usize) -> Option<usize> {
    while at < end {
        if bytes[at] == b'`' {
            let m = run_len(bytes, at, end, b'`');
            if m == n {
                return Some(at);
            }
            at += m;
        } else {
            at += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ATOM;

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
                "\\*literal\\*",
                &[("\\", "m"), ("*literal", ""), ("\\", "m"), ("*", "")],
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
}
