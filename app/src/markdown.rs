//! Markdown line structure: block markers and inline styles, in byte ranges of one line.
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

/// What Enter does inside a block line.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Enter {
    /// Start the next line with this prefix.
    Continue(String),
    /// The item is empty: drop its marker and end the list.
    End,
}

/// What Enter does at byte `caret` of `line` (no `\n` in `line`).
fn enter(line: &str, caret: usize) -> Option<Enter> {
    let l = self::line(line);
    if caret < l.content() {
        return None;
    }
    let indent = &line[..l.marker.start];
    let marker = &line[l.marker.clone()];
    let bullet = || char::from(line.as_bytes()[l.marker.start]);
    let prefix = match l.block {
        Block::Paragraph | Block::Heading(_) | Block::Rule => return None,
        _ if line[l.content()..].trim().is_empty() => return Some(Enter::End),
        Block::Bullet => format!("{indent}{} ", bullet()),
        Block::Task(_) => format!("{indent}{} [ ] ", bullet()),
        Block::Ordered(n) => {
            let delimiter = char::from(marker.as_bytes()[marker.len() - 2]);
            format!("{indent}{}{delimiter} ", n.saturating_add(1))
        }
        Block::Quote => format!("{indent}{marker}"),
    };
    Some(Enter::Continue(prefix))
}

/// The edit for Enter at byte `caret` of `text`: the byte range to replace and its replacement.
/// `None` means a plain newline.
pub fn enter_edit(text: &str, caret: usize) -> Option<(Range<usize>, String)> {
    let start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let end = text[caret..].find('\n').map_or(text.len(), |i| caret + i);
    match enter(&text[start..end], caret - start)? {
        Enter::Continue(prefix) => Some((caret..caret, format!("\n{prefix}"))),
        Enter::End => Some((start..end, String::new())),
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
#[path = "../tests/unit/markdown.rs"]
mod tests;
