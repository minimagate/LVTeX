use super::*;
use std::collections::HashSet;

fn fade(from: Option<u32>, to: Option<u32>, start: Instant) -> Fade {
    Fade {
        paragraph: 0,
        from,
        to,
        start,
        anchor: None,
    }
}

fn at(start: Instant, fraction: f32) -> Instant {
    start + Duration::from_millis((DOT_FADE_MS as f32 * fraction) as u64)
}

#[test]
fn a_new_dot_fades_in_from_transparent() {
    let start = Instant::now();
    let f = fade(None, Some(DOT_SORRY), start);
    assert_eq!(shown(&f, start), (Some(DOT_SORRY), 0.0));
    assert_eq!(shown(&f, at(start, 1.0)), (Some(DOT_SORRY), 1.0));
}

#[test]
fn a_colour_change_passes_strictly_between_its_ends() {
    let start = Instant::now();
    let f = fade(Some(DOT_CHECKING), Some(DOT_VERIFIED), start);
    assert_eq!(shown(&f, start).0, Some(DOT_CHECKING));
    assert_eq!(shown(&f, at(start, 1.0)), (Some(DOT_VERIFIED), 1.0));
    let (mid, alpha) = shown(&f, at(start, 0.5));
    assert_eq!(alpha, 1.0);
    let mid = mid.expect("a colour is shown mid-change");
    for shift in [16, 8, 0] {
        let channel = |c: u32| (c >> shift) & 255;
        let (a, b, m) = (channel(DOT_CHECKING), channel(DOT_VERIFIED), channel(mid));
        assert!(a.min(b) < m && m < a.max(b), "channel {shift}: {a} {m} {b}");
    }
}

#[test]
fn a_dot_that_goes_away_fades_out_in_its_colour() {
    let start = Instant::now();
    let f = fade(Some(DOT_FAILED), None, start);
    assert_eq!(shown(&f, start), (Some(DOT_FAILED), 1.0));
    assert_eq!(shown(&f, at(start, 1.0)), (Some(DOT_FAILED), 0.0));
}

#[test]
fn a_dot_that_is_absent_at_both_ends_shows_nothing() {
    let start = Instant::now();
    assert_eq!(shown(&fade(None, None, start), start), (None, 0.0));
    assert_eq!(shown(&fade(None, None, start), at(start, 1.0)), (None, 0.0));
}

#[test]
fn each_status_has_its_own_colour() {
    let colours = [
        Status::Checking,
        Status::Sorry,
        Status::Verified,
        Status::Failed,
    ]
    .map(status_color);
    let distinct: HashSet<u32> = colours.into_iter().collect();
    assert_eq!(distinct.len(), 4);
}

#[test]
fn mix_returns_its_ends_and_blends_between_them() {
    assert_eq!(mix(DOT_CHECKING, DOT_VERIFIED, 0.0), DOT_CHECKING);
    assert_eq!(mix(DOT_CHECKING, DOT_VERIFIED, 1.0), DOT_VERIFIED);
    assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
}

#[test]
fn lean_path_sits_beside_the_document() {
    assert_eq!(
        lean_path(Path::new("/docs/notes.md")),
        PathBuf::from("/docs/notes.mls.lean")
    );
    assert_eq!(
        lean_path(Path::new("/docs/proof.lean")),
        PathBuf::from("/docs/proof.mls.lean")
    );
    assert_eq!(
        lean_path(Path::new("/docs/Meeting 2026.10.20")),
        PathBuf::from("/docs/Meeting 2026.10.mls.lean")
    );
}

#[test]
fn a_panel_covers_only_its_drawn_area() {
    let mut panel = Panel::new(1, false);
    // Not yet drawn: it has no area, so it covers nothing.
    assert!(!panel_covers(Some(&panel), 0.0, 0.0));
    panel.area = (10.0, 10.0, 20.0, 20.0);
    assert!(panel_covers(Some(&panel), 10.0, 10.0));
    assert!(panel_covers(Some(&panel), 29.9, 29.9));
    assert!(
        !panel_covers(Some(&panel), 30.0, 15.0),
        "right edge is outside"
    );
    assert!(!panel_covers(None, 15.0, 15.0));
}

#[test]
fn lean_project_resolves_env_then_installed_copy_then_checkout() {
    let env = PathBuf::from("/env/lean");
    let user = PathBuf::from("/Users/u/Library/Application Support/Mathstral/lean");
    let checkout = PathBuf::from("/src/mathstral/lean");
    let cases = [
        (
            "the environment variable wins over both copies",
            Some(env.clone()),
            Some(user.clone()),
            vec![user.clone(), checkout.clone()],
            env.clone(),
        ),
        (
            "an installed copy wins over the checkout",
            None,
            Some(user.clone()),
            vec![user.clone(), checkout.clone()],
            user.clone(),
        ),
        (
            "the checkout is used when there is no installed copy",
            None,
            Some(user.clone()),
            vec![checkout.clone()],
            checkout.clone(),
        ),
        (
            "with neither present, the installed path names where to put the project",
            None,
            Some(user.clone()),
            vec![],
            user.clone(),
        ),
        (
            "without HOME the checkout is the fallback",
            None,
            None,
            vec![],
            checkout.clone(),
        ),
    ];
    for (case, env, user, existing, expected) in cases {
        let chosen = choose_lean_project(env, user, checkout.clone(), |path| {
            existing.iter().any(|dir| dir.as_path() == path)
        });
        assert_eq!(chosen, expected, "{case}");
    }
}

#[test]
fn lean_path_never_names_the_document_or_a_users_own_lean_file() {
    for document in ["/docs/x.lean", "/docs/x.md", "/docs/Main.md"] {
        assert_ne!(lean_path(Path::new(document)), Path::new(document));
    }
    assert_ne!(
        lean_path(Path::new("/docs/Main.md")),
        Path::new("/docs/Main.lean")
    );
}
