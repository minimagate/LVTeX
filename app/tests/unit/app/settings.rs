use super::*;

#[test]
fn bullets_show_one_per_character_up_to_the_limit() {
    let long = "k".repeat(SETTINGS_BULLETS + 5);
    let cases = [
        ("", 0),
        ("sk", 2),
        // One bullet per character, not per byte.
        ("é", 1),
        (long.as_str(), SETTINGS_BULLETS),
    ];
    for (draft, count) in cases {
        assert_eq!(
            bullets(draft),
            "•".repeat(count),
            "draft of {} characters",
            draft.chars().count()
        );
    }
}

#[test]
fn typed_keeps_only_the_visible_key_characters() {
    let cases = [
        ("", ""),
        ("sk-or-v1-abc", "sk-or-v1-abc"),
        (" sk-or\tv1\n", "sk-orv1"),
        ("\u{7}ab\u{1b}", "ab"),
        ("\r\n \t", ""),
    ];
    for (text, expected) in cases {
        assert_eq!(typed(text), expected, "input {text:?}");
    }
}
