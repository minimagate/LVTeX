use super::*;
use serde_json::json;

#[test]
fn offline_queue_recovers_and_renders_all_pending_jobs() {
    let mut warmups = 0;
    let worker = Worker::with_backend(
        || {},
        move |request| {
            if request["messages"].as_array().unwrap().is_empty() {
                warmups += 1;
                if warmups == 1 {
                    return Err(Error {
                        message: "offline".into(),
                        offline: true,
                    });
                }
                return Ok(json!({"done":true}));
            }
            Ok(json!({"message":{"content":"x^2"}}))
        },
        Duration::from_millis(5),
    );
    assert!(matches!(
        worker.rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        Reply::Connection(false)
    ));
    for id in 0..10 {
        worker.submit(Job {
            id,
            prompt: Some(format!("square {id}")),
            latex: None,
        });
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut connected = false;
    while ids.len() < 10 {
        match worker.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            Reply::Connection(true) => connected = true,
            Reply::Done(id, result) => {
                assert!(result.is_ok());
                ids.insert(id);
            }
            _ => panic!("unexpected worker reply"),
        }
    }
    assert!(connected);
    assert_eq!(ids, (0..10).collect::<std::collections::BTreeSet<u64>>());
}

#[test]
fn panicking_job_fails_alone_and_worker_survives() {
    let worker = Worker::with_backend(
        || {},
        |request| {
            if request["messages"].as_array().unwrap().is_empty() {
                return Ok(json!({"done":true}));
            }
            panic!("backend exploded")
        },
        Duration::from_millis(5),
    );
    let timeout = Duration::from_secs(5);
    assert!(matches!(
        worker.rx.recv_timeout(timeout).unwrap(),
        Reply::Connection(true)
    ));
    worker.submit(Job {
        id: 1,
        prompt: Some("boom".into()),
        latex: None,
    });
    assert!(matches!(
        worker.rx.recv_timeout(timeout).unwrap(),
        Reply::Done(1, Err(_))
    ));
    worker.submit(Job {
        id: 2,
        prompt: None,
        latex: Some("x^2".into()),
    });
    assert!(matches!(
        worker.rx.recv_timeout(timeout).unwrap(),
        Reply::Done(2, Ok(_))
    ));
}

#[test]
fn local_preview_slot_keeps_only_the_newest_revision() {
    let local = LocalWorker {
        pending: Arc::new((
            Mutex::new(LocalPending {
                jobs: std::collections::VecDeque::new(),
                preview: None,
                shutdown: false,
            }),
            Condvar::new(),
        )),
    };
    local.preview(Preview {
        id: 4,
        revision: 1,
        display_latex: "x".into(),
    });
    local.preview(Preview {
        id: 4,
        revision: 2,
        display_latex: "x^2".into(),
    });
    local.submit(Job {
        id: 5,
        prompt: None,
        latex: Some("y".into()),
    });
    let mut prefer_preview = true;
    {
        let (lock, _) = &*local.pending;
        let mut pending = lock.lock().unwrap();
        let Some(LocalTask::Preview(preview)) = pending.take_next(&mut prefer_preview) else {
            panic!("latest preview should be selected first")
        };
        assert_eq!(preview.revision, 2);
        assert_eq!(preview.display_latex, "x^2");
        assert!(pending.preview.is_none());
        assert!(matches!(
            pending.take_next(&mut prefer_preview),
            Some(LocalTask::Job(_))
        ));
    }
    local.preview(Preview {
        id: 4,
        revision: 3,
        display_latex: "x^3".into(),
    });
    local.cancel_preview(4);
    let (lock, _) = &*local.pending;
    assert!(lock.lock().unwrap().preview.is_none());
}

#[test]
fn local_formulas_and_previews_render_while_model_backend_is_blocked() {
    let (started, wait_started) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    let worker = Worker::with_backend(
        || {},
        move |request| {
            if request["messages"].as_array().unwrap().is_empty() {
                return Ok(json!({"done":true}));
            }
            started.send(()).unwrap();
            wait_release.recv().unwrap();
            Ok(json!({"message":{"content":"x^2"}}))
        },
        Duration::from_millis(5),
    );
    worker.submit(Job {
        id: 1,
        prompt: Some("square x".into()),
        latex: None,
    });
    wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.submit(Job {
        id: 2,
        prompt: None,
        latex: Some("abcd".into()),
    });
    worker.preview(Preview {
        id: 3,
        revision: 7,
        display_latex: "x^2".into(),
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut formula = false;
    let mut preview = false;
    while !(formula && preview) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let reply = worker.rx.recv_timeout(remaining).unwrap();
        match reply {
            Reply::Done(2, Ok(image)) => {
                assert_eq!(image.latex, "abcd");
                formula = true;
            }
            Reply::PreviewDone(3, 7, Ok(image)) => {
                assert_eq!(image.latex, "x^2");
                preview = true;
            }
            Reply::Connection(_) => {}
            _ => panic!("unexpected local reply"),
        }
    }
    release.send(()).unwrap();
}

#[test]
fn prose_is_rejected_for_generated_math_but_words_render_as_user_math() {
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    for source in ["spa", "span", "spanning", "abcd", r"v_1+v_2\in span"] {
        assert_eq!(
            render(&mut renderer, source).unwrap().latex,
            source,
            "user math {source:?}"
        );
    }
    let mut calls = 0;
    let job = Job {
        id: 1,
        prompt: Some("some math".into()),
        latex: None,
    };
    let image = resolve_with(&job, &mut renderer, &mut HashMap::new(), |_| {
        calls += 1;
        if calls == 1 {
            Ok(json!({"message":{"content":"The answer is x"}}))
        } else {
            Ok(json!({"message":{"content":"x"}}))
        }
    })
    .unwrap_or_else(|error| panic!("{}", error.message));
    assert_eq!(calls, 2);
    assert_eq!(image.latex, "x");
}

#[test]
fn cleanup_and_rendered_repair_then_cache() {
    assert_eq!(
        clean("Here is the result:\n```latex\n$\\frac{1}{3}$\n```\nExplanation: a third"),
        r"\frac{1}{3}"
    );
    assert_eq!(clean(r"Here it is: \[x^2\] This is squared."), "x^2");
    let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
    let mut cache = HashMap::new();
    let mut calls = 0;
    let job = Job {
        id: 1,
        prompt: Some("one third".into()),
        latex: None,
    };
    let image = resolve_with(&job, &mut renderer, &mut cache, |request| {
        calls += 1;
        assert_eq!(request["options"]["temperature"], 0);
        assert_eq!(request["stream"], false);
        if calls == 1 {
            Ok(json!({"message":{"content":r"\frac{"}}))
        } else {
            assert!(
                request["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap()
                    .contains("unclosed brace")
            );
            Ok(json!({"message":{"content":r"\frac{1}{3}"}}))
        }
    })
    .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(calls, 2);
    assert!(image.inline.baseline > 0.0 && image.inline.baseline < image.inline.height());
    assert!(image.display.height() > image.inline.height());
    let cached = resolve_with(&job, &mut renderer, &mut cache, |_| {
        panic!("cached prompts must not call Ollama")
    })
    .unwrap_or_else(|e| panic!("{}", e.message));
    assert!(Arc::ptr_eq(&image, &cached));
    let bad = Job {
        id: 2,
        prompt: Some("bad".into()),
        latex: None,
    };
    let mut calls = 0;
    assert!(
        resolve_with(&bad, &mut renderer, &mut cache, |_| {
            calls += 1;
            Ok(json!({"message":{"content":r"\notARealCommand{x}"}}))
        })
        .is_err()
    );
    assert_eq!(calls, 3);
}
