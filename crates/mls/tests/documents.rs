//! The sample documents run through OpenRouter (Jev and Haiku) and the Lean REPL. Needs the network,
//! an OpenRouter key and the Lean project built. Run with
//! `cargo test --release -p mathstral-mls --test documents -- --ignored --nocapture`.

use mathstral_mls::{Label, Mls, Paragraph, Status, Worker};

#[path = "fixtures/documents.rs"]
mod documents;

use documents::{DOCUMENTS, assert_outcome, drive, lean_project, lean_ready};

#[test]
#[ignore = "calls OpenRouter: needs OPENROUTER_API_KEY and the Lean project in lean/"]
fn live_documents_end_with_their_dots() {
    // The key is only checked for presence here; it is never printed.
    if !std::env::var("OPENROUTER_API_KEY").is_ok_and(|key| !key.trim().is_empty()) {
        eprintln!("skipping: OPENROUTER_API_KEY is not set");
        return;
    }
    let project = lean_project();
    if !lean_ready(&project) {
        eprintln!(
            "skipping: the Lean REPL is not built in {} (run `lake build Mathlib repl` there)",
            project.display()
        );
        return;
    }
    for document in &DOCUMENTS {
        let worker = Worker::new(project.clone(), || {});
        worker.set_key(std::env::var("OPENROUTER_API_KEY").ok());
        let mut mls = Mls::new();
        let lines: Vec<&str> = document.lines.iter().map(|line| line.text).collect();
        drive(&mut mls, &worker, &lines)
            .unwrap_or_else(|error| panic!("{}: {error}", document.name));
        assert_outcome(&mls, document);
    }
}
