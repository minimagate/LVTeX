//! Background worker: threads, job queues and formula rendering. Nothing here runs on a key event.

use super::model::{EXAMPLES, Error, SYSTEM, chat, clean, reject_prose};
use crate::config::*;
use mathstral_render::math::Renderer;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::Duration,
};

/// The hand-written caret's box in an image, in pt like `baseline`: the horizontal centre of its
/// stem, its top edge and its height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretBox {
    pub x: f32,
    pub top: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct Image {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub baseline: f32,
    pub caret: Option<CaretBox>,
}
impl Image {
    pub fn width(&self) -> f32 {
        self.width as f32 / 2.0
    }
    pub fn height(&self) -> f32 {
        self.height as f32 / 2.0
    }
}
#[derive(Debug)]
pub struct FormulaImage {
    pub latex: String,
    pub inline: Image,
    pub display: Image,
}
#[derive(Clone)]
pub struct Job {
    pub id: u64,
    pub prompt: Option<String>,
    pub latex: Option<String>,
}
#[derive(Clone)]
pub struct Preview {
    pub id: u64,
    pub revision: u64,
    pub display_latex: String,
}
pub enum Reply {
    Connection(bool),
    Done(u64, Result<Arc<FormulaImage>, String>),
    PreviewDone(u64, u64, Result<Arc<FormulaImage>, String>),
}
pub struct Worker {
    tx: mpsc::Sender<Job>,
    local: LocalWorker,
    pub rx: mpsc::Receiver<Reply>,
}

struct LocalPending {
    jobs: std::collections::VecDeque<Job>,
    preview: Option<Preview>,
    shutdown: bool,
}
struct LocalWorker {
    pending: Arc<(Mutex<LocalPending>, Condvar)>,
}

impl Worker {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_backend(wake, chat, Duration::from_secs(2))
    }
    fn with_backend(
        wake: impl Fn() + Send + Sync + 'static,
        mut request: impl FnMut(&Value) -> Result<Value, Error> + Send + 'static,
        retry_delay: Duration,
    ) -> Self {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (replies, rx) = mpsc::channel();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let local = LocalWorker::new(replies.clone(), wake.clone());
        thread::Builder::new()
            .name("mathstral-math".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
                let mut cache: HashMap<String, Arc<FormulaImage>> = HashMap::new();
                let send = |reply| {
                    let _ = replies.send(reply);
                    wake();
                };
                // An empty chat loads the model without generating text.
                let warm = json!({"model": OLLAMA_MODEL, "messages": [], "stream": false,
                "keep_alive": KEEP_ALIVE, "options": {"temperature": 0}});
                let mut online = warm_up(&mut request, &warm);
                send(Reply::Connection(online));
                let mut pending = std::collections::VecDeque::new();
                loop {
                    while let Ok(job) = jobs.try_recv() {
                        pending.push_back(job);
                    }
                    if pending.is_empty() || !online {
                        match jobs.recv_timeout(retry_delay) {
                            Ok(job) => pending.push_back(job),
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(_) => {}
                        }
                        if !online {
                            // Cached prompts can render without Ollama.
                            let mut rest = std::collections::VecDeque::new();
                            while let Some(job) = pending.pop_front() {
                                if job.prompt.as_ref().is_some_and(|p| cache.contains_key(p)) {
                                    let result = guarded(&mut renderer, |renderer| {
                                        resolve_with(&job, renderer, &mut cache, &mut request)
                                    });
                                    send(Reply::Done(job.id, result.map_err(|e| e.message)));
                                } else {
                                    rest.push_back(job);
                                }
                            }
                            pending = rest;
                            online = warm_up(&mut request, &warm);
                            if online {
                                send(Reply::Connection(true));
                            }
                            continue;
                        }
                    }
                    if let Some(job) = pending.pop_front() {
                        let result = guarded(&mut renderer, |renderer| {
                            resolve_with(&job, renderer, &mut cache, &mut request)
                        });
                        match result {
                            Ok(image) => send(Reply::Done(job.id, Ok(image))),
                            Err(error) if error.offline => {
                                online = false;
                                send(Reply::Connection(false));
                                pending.push_front(job);
                            }
                            Err(error) => send(Reply::Done(job.id, Err(error.message))),
                        }
                    }
                }
            })
            .expect("math worker");
        Self { tx, local, rx }
    }
    pub fn submit(&self, job: Job) {
        if job.latex.is_some() {
            self.local.submit(job);
        } else {
            let _ = self.tx.send(job);
        }
    }
    pub fn preview(&self, preview: Preview) {
        self.local.preview(preview);
    }
    pub fn cancel_preview(&self, id: u64) {
        self.local.cancel_preview(id);
    }
}

impl LocalWorker {
    fn new(replies: mpsc::Sender<Reply>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let pending = Arc::new((
            Mutex::new(LocalPending {
                jobs: std::collections::VecDeque::new(),
                preview: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let worker_pending = pending.clone();
        thread::Builder::new()
            .name("mathstral-render".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
                let mut prefer_preview = true;
                loop {
                    let task = {
                        let (lock, ready) = &*worker_pending;
                        let mut pending = lock.lock().expect("local render queue");
                        while pending.jobs.is_empty()
                            && pending.preview.is_none()
                            && !pending.shutdown
                        {
                            pending = ready.wait(pending).expect("local render queue");
                        }
                        if pending.shutdown {
                            break;
                        }
                        pending.take_next(&mut prefer_preview)
                    };
                    let Some(task) = task else {
                        continue;
                    };
                    let reply = match task {
                        LocalTask::Job(job) => {
                            let result = guarded(&mut renderer, |renderer| {
                                render(renderer, job.latex.as_deref().unwrap_or(""))
                                    .map(Arc::new)
                                    .map_err(Into::into)
                            })
                            .map_err(|error| error.message);
                            Reply::Done(job.id, result)
                        }
                        LocalTask::Preview(preview) => {
                            let result = guarded(&mut renderer, |renderer| {
                                render(renderer, &preview.display_latex)
                                    .map(Arc::new)
                                    .map_err(Into::into)
                            })
                            .map_err(|error| error.message);
                            Reply::PreviewDone(preview.id, preview.revision, result)
                        }
                    };
                    let _ = replies.send(reply);
                    wake();
                }
            })
            .expect("math render worker");
        Self { pending }
    }

    fn submit(&self, job: Job) {
        let (lock, ready) = &*self.pending;
        lock.lock().expect("local render queue").jobs.push_back(job);
        ready.notify_one();
    }

    fn preview(&self, preview: Preview) {
        let (lock, ready) = &*self.pending;
        lock.lock().expect("local render queue").preview = Some(preview);
        ready.notify_one();
    }

    fn cancel_preview(&self, id: u64) {
        let (lock, _) = &*self.pending;
        let mut pending = lock.lock().expect("local render queue");
        if pending
            .preview
            .as_ref()
            .is_some_and(|preview| preview.id == id)
        {
            pending.preview = None;
        }
    }
}

enum LocalTask {
    Job(Job),
    Preview(Preview),
}

impl LocalPending {
    fn take_next(&mut self, prefer_preview: &mut bool) -> Option<LocalTask> {
        let task = if *prefer_preview {
            self.preview
                .take()
                .map(LocalTask::Preview)
                .or_else(|| self.jobs.pop_front().map(LocalTask::Job))
        } else {
            self.jobs
                .pop_front()
                .map(LocalTask::Job)
                .or_else(|| self.preview.take().map(LocalTask::Preview))
        };
        *prefer_preview = !matches!(task, Some(LocalTask::Preview(_)));
        task
    }
}

impl Drop for LocalWorker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.pending;
        lock.lock().expect("local render queue").shutdown = true;
        ready.notify_one();
    }
}

// A panicking backend during warm-up counts as offline.
fn warm_up(request: &mut impl FnMut(&Value) -> Result<Value, Error>, warm: &Value) -> bool {
    panic::catch_unwind(AssertUnwindSafe(|| request(warm).is_ok())).unwrap_or(false)
}
// A panic in mitex/typst (or a backend) fails only the current job. The renderer
// may be left inconsistent, so it is rebuilt.
fn guarded<T>(
    renderer: &mut Renderer,
    work: impl FnOnce(&mut Renderer) -> Result<T, Error>,
) -> Result<T, Error> {
    panic::catch_unwind(AssertUnwindSafe(|| work(renderer))).unwrap_or_else(|_| {
        *renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        Err("renderer panicked".to_owned().into())
    })
}

fn resolve_with(
    job: &Job,
    renderer: &mut Renderer,
    cache: &mut HashMap<String, Arc<FormulaImage>>,
    mut request: impl FnMut(&Value) -> Result<Value, Error>,
) -> Result<Arc<FormulaImage>, Error> {
    if let Some(latex) = &job.latex {
        return render(renderer, latex).map(Arc::new).map_err(Into::into);
    }
    let prompt = job.prompt.as_deref().unwrap_or("");
    if let Some(image) = cache.get(prompt) {
        return Ok(image.clone());
    }
    let mut messages = vec![json!({"role":"system", "content":SYSTEM})];
    for (input, output) in EXAMPLES {
        messages.push(json!({"role":"user", "content":input}));
        messages.push(json!({"role":"assistant", "content":output}));
    }
    messages.push(json!({"role":"user", "content":prompt}));
    let mut last = String::new();
    for _ in 0..3 {
        let response = request(
            &json!({"model": OLLAMA_MODEL, "stream": false, "keep_alive": KEEP_ALIVE,
            "options": {"temperature":0, "num_predict":2048}, "messages":messages}),
        )?;
        let raw = response["message"]["content"].as_str().unwrap_or("");
        let latex = clean(raw);
        match reject_prose(&latex).and_then(|()| render(renderer, &latex)) {
            Ok(image) => {
                let image = Arc::new(image);
                if cache.len() >= MATH_CACHE_LIMIT {
                    cache.clear();
                }
                cache.insert(prompt.to_owned(), image.clone());
                return Ok(image);
            }
            Err(error) => {
                last = error;
                messages.push(json!({"role":"assistant", "content":raw}));
                messages.push(json!({"role":"user", "content":format!("Rendering failed: {last}. Correct the expression. Return only the LaTeX math body, without prose or delimiters.")}));
            }
        }
    }
    Err(last.into())
}

pub fn render(renderer: &mut Renderer, latex: &str) -> Result<FormulaImage, String> {
    if latex.is_empty() || latex.len() > 16384 {
        return Err("empty or oversized expression".into());
    }
    let body = mathstral_latex::convert(latex)?;
    let mut image = |display| {
        let equation = if display {
            format!("$ {body} $")
        } else {
            format!("#math.equation(block: true, math.inline(${body}$))")
        };
        let pixels = renderer.render_source(format!(
            "#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(font: \"Libertinus Serif\", size: {FONT_SIZE}pt, fill: rgb(\"{TEXT_COLOR:06x}\"))\n#show math.equation: set text(font: \"New Computer Modern Math\")\n{}\n{equation}", mathstral_latex::PRELUDE), 2.0)?;
        let (width, height) = (pixels.width() as usize, pixels.height() as usize);
        let mut rgba = pixels.data().to_vec();
        let caret = take_caret(&mut rgba, width);
        Ok::<_, String>(Image {
            width,
            height,
            rgba,
            baseline: renderer.last_baseline as f32,
            caret,
        })
    };
    Ok(FormulaImage {
        latex: latex.into(),
        inline: image(false)?,
        display: image(true)?,
    })
}

/// Whether a premultiplied pixel is the caret sentinel, with a small tolerance for anti-aliasing:
/// green is 0 and red and blue equal the alpha.
fn is_sentinel(pixel: &[u8]) -> bool {
    let alpha = pixel[3] as i32;
    alpha > 0
        && pixel[1] <= 2
        && (pixel[0] as i32 - alpha).abs() <= 2
        && (pixel[2] as i32 - alpha).abs() <= 2
}

/// Where the math baseline falls inside the `|` bar, as a fraction of the bar's height from its top.
/// Measured in New Computer Modern Math at base size (FONT_SIZE): the bar runs from 2 pt to 21 pt and
/// the baseline is at 17 pt, so the bar sits from about -0.21 em to +0.79 em.
const BAR_BASELINE_FROM_TOP: f32 = 15.0 / 19.0;
/// The prose caret after "A" at base size, cap height to baseline, measured with `Typesetter`. The
/// math caret takes the same share of its bar's height, so a base-level math caret matches it.
const PROSE_CAP_CARET: f32 = 12.5;

/// Erases every sentinel pixel and returns the caret for the solid ones. Faint edge pixels are erased
/// but do not widen the box, so grey text fringe cannot move it. The caret sits on the bar's own
/// baseline with the prose cap-height proportion of the bar.
fn take_caret(rgba: &mut [u8], width: usize) -> Option<CaretBox> {
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (usize::MAX, 0, usize::MAX, 0);
    let mut solid = false;
    for (i, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if !is_sentinel(pixel) {
            continue;
        }
        if pixel[3] >= 32 {
            let (x, y) = (i % width, i / width);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
            solid = true;
        }
        pixel.fill(0);
    }
    // Pixels are 2 per pt, so a pixel index divided by 2 is pt.
    solid.then(|| {
        let bar_top = min_y as f32 / 2.0;
        let bar_height = (max_y + 1 - min_y) as f32 / 2.0;
        let bottom = bar_top + BAR_BASELINE_FROM_TOP * bar_height;
        let height = bar_height * PROSE_CAP_CARET / FONT_SIZE;
        CaretBox {
            x: (min_x + max_x + 1) as f32 / 4.0,
            top: bottom - height,
            height,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
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
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
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

    #[test]
    fn hand_written_caret_is_located_erased_and_scales_with_script_size() {
        let caret = format!(r"\mathord{{\textcolor{{#{CARET_SENTINEL:06X}}}{{|}}}}");
        let mut renderer = Renderer::with_prelude(mathstral_latex::PRELUDE);
        let base = render(&mut renderer, &format!(r"x{caret}")).unwrap();
        let script = render(&mut renderer, &format!(r"x^{{2{caret}}}")).unwrap();
        let plain = render(&mut renderer, "x^2").unwrap();
        for image in [&base.inline, &base.display, &script.inline, &script.display] {
            assert!(
                image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| !is_sentinel(pixel)),
                "sentinel pixels remain"
            );
        }
        let base_box = base.inline.caret.expect("base caret");
        let script_box = script.inline.caret.expect("script caret");
        let prose = crate::view::layout::Typesetter::new()
            .layout("A", 680.0)
            .caret_extent(0, 1)
            .1;
        assert!(
            (base_box.height - prose).abs() <= 0.5,
            "base math caret {} vs prose cap caret {prose}",
            base_box.height
        );
        assert!(
            (base_box.top + base_box.height - base.inline.baseline).abs() <= 0.5,
            "caret bottom must sit on the baseline"
        );
        for (b, image) in [(base_box, &base.inline), (script_box, &script.inline)] {
            assert!(b.x >= 0.0 && b.x <= image.width());
            assert!(b.top >= 0.0 && b.top + b.height <= image.height());
        }
        assert!(
            script_box.height < base_box.height,
            "script {} vs base {}",
            script_box.height,
            base_box.height
        );
        assert!(plain.inline.caret.is_none() && plain.display.caret.is_none());
    }
}
