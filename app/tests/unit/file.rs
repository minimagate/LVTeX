use super::*;
#[test]
fn window_geometry_keeps_screen_filling_size_position_and_legacy_settings() {
    let g = WindowGeometry {
        width: 1512.0,
        height: 944.0,
        position: Some((0.0, 38.0)),
    };
    assert_eq!(WindowGeometry::parse(&g.encode()), Some(g));
    let secondary = WindowGeometry {
        position: Some((-1512.0, -200.0)),
        ..g
    };
    assert_eq!(WindowGeometry::parse(&secondary.encode()), Some(secondary));
    assert_eq!(WindowGeometry::parse("1495 944\n").unwrap().position, None);
    assert!(WindowGeometry::parse("NaN 944").is_none());
}
#[test]
fn export_appends_pdf_without_replacing_existing_extension() {
    assert_eq!(with_pdf_extension("a".into()), PathBuf::from("a.pdf"));
    assert_eq!(
        with_pdf_extension("Report 2026.10".into()),
        PathBuf::from("Report 2026.10.pdf")
    );
    assert_eq!(with_pdf_extension("x.PDF".into()), PathBuf::from("x.PDF"));
    assert_eq!(with_pdf_extension("x.pdf".into()), PathBuf::from("x.pdf"));
}
#[test]
fn saves_unicode_and_replaces_existing_document() {
    let p = std::env::temp_dir().join(format!("mathstral-test-{}.md", std::process::id()));
    write(&p, "α\n$\\int_0^1 x dx$").unwrap();
    assert_eq!(read(&p).unwrap(), "α\n$\\int_0^1 x dx$");
    write(&p, "new").unwrap();
    assert_eq!(read(&p).unwrap(), "new");
    fs::remove_file(p).unwrap();
}
