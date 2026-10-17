use super::*;

// The glyphs drawn on one visual row.
fn row(layout: &Layout, index: usize) -> Vec<&Placed> {
    let line = &layout.lines[index];
    layout
        .glyphs
        .iter()
        .filter(|g| g.baseline >= line.top && g.baseline < line.top + line.height)
        .collect()
}

// Expected (face, color) per glyph, one glyph per character of each piece.
fn expect(t: &mut Typesetter, table: &[(&str, Face, u32)]) -> Vec<(cosmic_text::fontdb::ID, u32)> {
    let mut want = Vec::new();
    for &(piece, face, color) in table {
        let id = t.word("x", face).glyphs[0].font_id;
        want.extend(piece.chars().map(|_| (id, color)));
    }
    want
}

#[test]
fn heading_levels_pick_their_face_and_dim_the_marker() {
    let mut t = Typesetter::new();
    let cases = [
        ("# Title", Face::Bold),
        ("## Title", Face::Italic),
        ("### Title", Face::BoldItalic),
        ("###### Title", Face::BoldItalic),
    ];
    for (text, face) in cases {
        let layout = t.layout(text, 680.0);
        let id = t.word("x", face).glyphs[0].font_id;
        let glyphs = row(&layout, 0);
        let marker = text.len() - "Title".len();
        assert!(glyphs.iter().all(|g| g.glyph.font_id == id), "{text:?}");
        assert!(
            glyphs[..marker].iter().all(|g| g.color == MARKER_COLOR),
            "{text:?} marker"
        );
        assert!(
            glyphs[marker..].iter().all(|g| g.color == TEXT_COLOR),
            "{text:?} content"
        );
        assert!(layout.lines.iter().all(|line| line.height == LINE_HEIGHT));
    }
}

#[test]
fn inline_styles_pick_faces_and_colors() {
    let mut t = Typesetter::new();
    let table = [
        ("**", Face::Bold, MARKER_COLOR),
        ("b", Face::Bold, TEXT_COLOR),
        ("**", Face::Bold, MARKER_COLOR),
        (" ", Face::Regular, TEXT_COLOR),
        ("*", Face::Italic, MARKER_COLOR),
        ("i", Face::Italic, TEXT_COLOR),
        ("*", Face::Italic, MARKER_COLOR),
        (" ", Face::Regular, TEXT_COLOR),
        ("***", Face::BoldItalic, MARKER_COLOR),
        ("bi", Face::BoldItalic, TEXT_COLOR),
        ("***", Face::BoldItalic, MARKER_COLOR),
        (" ", Face::Regular, TEXT_COLOR),
        ("`", Face::Mono, MARKER_COLOR),
        ("code", Face::Mono, CODE_COLOR),
        ("`", Face::Mono, MARKER_COLOR),
        (" ", Face::Regular, TEXT_COLOR),
        ("~~", Face::Regular, MARKER_COLOR),
        ("s", Face::Regular, TEXT_COLOR),
        ("~~", Face::Regular, MARKER_COLOR),
        (" ", Face::Regular, TEXT_COLOR),
        ("[", Face::Regular, MARKER_COLOR),
        ("l", Face::Regular, LINK_COLOR),
        ("](u)", Face::Regular, MARKER_COLOR),
    ];
    let text: String = table.iter().map(|t| t.0).collect();
    assert_eq!(text, "**b** *i* ***bi*** `code` ~~s~~ [l](u)");
    let layout = t.layout(&text, 680.0);
    let got: Vec<_> = row(&layout, 0)
        .iter()
        .map(|g| (g.glyph.font_id, g.color))
        .collect();
    assert_eq!(got, expect(&mut t, &table));

    // One strike mark, exactly over the struck "s" and not over its markers.
    assert_eq!(layout.marks.len(), 1);
    let mark = &layout.marks[0];
    let index = table.iter().position(|p| p.0 == "s").unwrap();
    let s: usize = table[..index].iter().map(|p| p.0.chars().count()).sum();
    let glyphs = row(&layout, 0);
    assert_eq!(mark.color, MARKER_COLOR);
    assert!((mark.x - glyphs[s].x).abs() < 0.01);
    assert!((mark.x + mark.width - glyphs[s + 1].x).abs() < 0.01);
    assert!((mark.y - (glyphs[s].baseline - FONT_SIZE * STRIKE_LIFT)).abs() < 0.01);
}

#[test]
fn code_is_smaller_mono_and_keeps_line_height() {
    let mut t = Typesetter::new();
    for text in ["`code`", "# `code`", "- `code` item", "a `x` b"] {
        let layout = t.layout(text, 680.0);
        assert!(
            layout.lines.iter().all(|line| line.height == LINE_HEIGHT),
            "{text:?}"
        );
    }
    let layout = t.layout("`code`", 680.0);
    assert_eq!(row(&layout, 0)[1].glyph.font_size, FONT_SIZE * 0.85);
}

#[test]
fn caret_height_follows_the_last_letter_ink() {
    let mut t = Typesetter::new();
    // Bytes: A=0, l=1, e=2, y=3, '.'=4. The caret after byte b sits after the letter at b-1.
    let layout = t.layout("Aley.", 680.0);
    let (a, l, e, y, dot) = (
        layout.caret_extent(0, 1),
        layout.caret_extent(0, 2),
        layout.caret_extent(0, 3),
        layout.caret_extent(0, 4),
        layout.caret_extent(0, 5),
    );
    let bottom = |(top, height): (f32, f32)| top + height;
    let baseline = layout.lines[0].baseline;
    assert!(a.0 < e.0, "cap is above x-height");
    assert!(l.0 < e.0, "ascender is above x-height");
    // Measured at quarter-point precision: l 13.25, A 12.5.
    assert!(l.1 > a.1, "ascender is taller than the cap");
    assert!(
        bottom(y) > baseline + 1.0,
        "descender reaches below the baseline"
    );
    assert!(
        (bottom(e) - baseline).abs() < 1.0,
        "x-height ends at the baseline"
    );
    assert!(
        (dot.1 - e.1).abs() < 1.0,
        "a period is as tall as an x-height letter"
    );
    let empty = t.layout("", 680.0);
    let line = &empty.lines[0];
    let (top, height) = empty.caret_extent(0, 0);
    assert!((height - a.1).abs() < 0.01 && (top - a.0).abs() < 0.01);
    assert!(top >= line.top && top + height <= line.top + line.height);
    // A closed one-line prompt's caret follows its last letter, not the full line box.
    let doc = Document::new("a <!--mathstral:pending:eCBzcXVhcmVk--> b");
    assert!(doc.formulas.contains_key(&2));
    let label = t.document(&doc, 680.0);
    let (_, label_height) = label.caret_extent(0, 2 + ATOM.len());
    assert!(
        label_height <= y.1 + 1.0,
        "label caret {label_height} vs y {}",
        y.1
    );
}

#[test]
fn block_colors_and_faces() {
    let mut t = Typesetter::new();
    let quote = t.layout("> quoted", 680.0);
    let italic = t.word("x", Face::Italic).glyphs[0].font_id;
    assert!(row(&quote, 0).iter().all(|g| g.glyph.font_id == italic));
    let done = t.layout("- [x] done", 680.0);
    assert!(row(&done, 0).iter().all(|g| g.color == MARKER_COLOR));
    let todo = t.layout("- [ ] todo", 680.0);
    assert!(row(&todo, 0)[6..].iter().all(|g| g.color == TEXT_COLOR));
}

#[test]
fn open_ai_prompt_is_dimmed_like_a_pending_prompt() {
    let mut t = Typesetter::new();
    let mut doc = Document::new("left right");
    doc.anchor = 4;
    doc.caret = 4;
    doc.open_ai_prompt();
    doc.replace("sum", true);
    let open = doc.open_range().expect("open prompt");
    let layout = t.document(&doc, 680.0);
    let (_, start) = layout.caret(open.start);
    let (_, end) = layout.caret(open.end);
    let glyphs = row(&layout, 0);
    let inside: Vec<_> = glyphs
        .iter()
        .filter(|g| g.x >= start && g.x < end)
        .collect();
    assert!(!inside.is_empty());
    assert!(inside.iter().all(|g| g.color == PENDING_COLOR));
    let before: Vec<_> = glyphs.iter().filter(|g| g.x < start).collect();
    assert!(!before.is_empty());
    assert!(before.iter().all(|g| g.color == TEXT_COLOR));
}

#[test]
fn open_prompt_alone_on_its_line_is_centered_like_display_math() {
    let mut t = Typesetter::new();
    let mut doc = Document::new("before\n\nafter");
    doc.anchor = 7;
    doc.caret = 7;
    doc.open_ai_prompt();
    doc.replace("x squared", true);
    let open = doc.open_range().expect("open prompt");
    let layout = t.document(&doc, 680.0);
    let (row, _) = layout.caret(open.start);
    let line = &layout.lines[row];
    let left = line.stops.first().expect("stops").1;
    let right = 680.0 - line.stops.last().expect("stops").1;
    assert!(left > 0.0);
    assert!((left - right).abs() <= 1.0, "left {left} right {right}");
    let before = layout.lines[0].height;
    assert!(line.height > before, "display padding");
}

#[test]
fn empty_open_prompt_on_an_empty_line_starts_centered() {
    let mut t = Typesetter::new();
    let mut doc = Document::new("before\n\nafter");
    doc.anchor = 7;
    doc.caret = 7;
    doc.open_ai_prompt();
    let open = doc.open_range().expect("open prompt");
    let layout = t.document(&doc, 680.0);
    let (row, _) = layout.caret(open.start);
    let line = &layout.lines[row];
    assert_eq!(line.stops.len(), 1);
    assert!(
        (line.stops[0].1 - 340.0).abs() <= 1.0,
        "x {}",
        line.stops[0].1
    );
}

#[test]
fn open_prompt_spanning_two_lines_stays_inline_on_both() {
    let mut t = Typesetter::new();
    let mut doc = Document::new("a\nb");
    doc.anchor = 0;
    doc.caret = 3;
    doc.open_ai_prompt();
    let layout = t.document(&doc, 680.0);
    assert!(layout.lines.len() >= 2, "lines {}", layout.lines.len());
    for line in &layout.lines[..2] {
        assert_eq!(line.stops.first().expect("stops").1, 0.0);
    }
}

#[test]
fn open_prompt_inside_a_sentence_stays_inline() {
    let mut t = Typesetter::new();
    let mut doc = Document::new("left right");
    doc.anchor = 4;
    doc.caret = 4;
    doc.open_ai_prompt();
    doc.replace("sum", true);
    let open = doc.open_range().expect("open prompt");
    let layout = t.document(&doc, 680.0);
    let (row, _) = layout.caret(open.start);
    assert_eq!(layout.lines[row].stops[0].1, 0.0);
}

#[test]
fn wrapped_items_hang_under_their_content_and_paragraphs_do_not() {
    let mut t = Typesetter::new();
    let words = "alpha beta gamma delta epsilon zeta eta theta";
    let cases: [(&str, Option<usize>); 5] = [
        ("- ", Some(2)),
        ("1. ", Some(3)),
        ("- [x] ", Some(6)),
        ("> ", Some(2)),
        ("", None),
    ];
    for (prefix, content) in cases {
        let text = format!("{prefix}{words}");
        let layout = t.layout(&text, 120.0);
        assert!(layout.lines.len() > 2, "{text:?} must wrap");
        let hang = content.map_or(0.0, |byte| layout.caret(byte).1);
        for line in &layout.lines[1..] {
            assert!((line.stops[0].1 - hang).abs() < 0.01, "{text:?}");
        }
        for line in &layout.lines {
            assert!(line.stops.last().unwrap().1 <= 120.0 + 0.01, "{text:?}");
        }
    }
}

#[test]
fn rules_draw_a_mark_from_the_marker_to_the_column() {
    let mut t = Typesetter::new();
    for text in ["---", "***", "* * *", "___"] {
        let layout = t.layout(text, 300.0);
        assert_eq!(layout.marks.len(), 1, "{text:?}");
        let line = &layout.lines[0];
        let mark = &layout.marks[0];
        assert_eq!(mark.color, MARKER_COLOR);
        assert!(
            (mark.x - layout.caret(text.len()).1).abs() < 0.01,
            "{text:?}"
        );
        assert!((mark.x + mark.width - 300.0).abs() < 0.01, "{text:?}");
        assert!((mark.y - (line.top + line.height / 2.0)).abs() < 0.01);
    }
}

#[test]
fn mixed_markdown_keeps_line_heights_and_editable_stops() {
    let mut t = Typesetter::new();
    let source = "# Title\n## Sub\n### Deep\nSome **bold** *it* ***both*** `code` ~~gone~~ [link](url) and a paragraph that wraps over several rows\n- item with words that wrap around the column\n- [x] done task\n1. first\n> quoted text that also wraps around\n---\nplain \\$5 end";
    let doc = Document::new(source);
    let text = doc.text();
    let layout = t.document(&doc, 160.0);
    assert!(layout.lines.len() > 12);
    assert!(layout.lines.iter().all(|line| line.height == LINE_HEIGHT));
    for (row, line) in layout.lines.iter().enumerate() {
        assert_eq!(layout.row_at(line.top + line.height / 2.0), row);
    }
    for (byte, _) in text.grapheme_indices(true) {
        let (row, x) = layout.caret(byte);
        assert_eq!(layout.hit(row, x), byte, "byte {byte} of {text:?}");
    }
}

#[test]
fn every_face_loads_its_own_font() {
    let mut t = Typesetter::new();
    let faces = [
        Face::Regular,
        Face::Italic,
        Face::Bold,
        Face::BoldItalic,
        Face::Mono,
    ];
    let ids: Vec<_> = faces
        .iter()
        .map(|&face| t.word("Title", face).glyphs[0].font_id)
        .collect();
    for (i, id) in ids.iter().enumerate() {
        assert!(
            !ids[..i].contains(id),
            "{:?} reuses another face's font",
            faces[i]
        );
    }
}
#[test]
fn wraps_and_hits_unicode_without_splitting_clusters() {
    let mut t = Typesetter::new();
    let text = "Hello e\u{301} world\n\nlast";
    let l = t.layout(text, 65.0);
    assert!(l.lines.len() > 3);
    for (i, _) in text.grapheme_indices(true) {
        let (row, x) = l.caret(i);
        assert_eq!(l.hit(row, x), i, "byte {i}");
    }
    assert_eq!(l.caret(text.len()).0, l.lines.len() - 1);
}
#[test]
fn stray_atoms_without_formulas_terminate() {
    let mut t = Typesetter::new();
    let layout = t.layout(&format!("{ATOM}a{ATOM}"), 300.0);
    assert!(!layout.lines.is_empty());
}

impl Layout {
    pub fn caret(&self, byte: usize) -> (usize, f32) {
        self.caret_on_line(byte, None)
    }
}
