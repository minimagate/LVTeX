//! Background worker: the model thread, the local render thread and their job queues. Nothing here runs on a key event.

use super::model::{
    Error, chat, chat_request, clean, conversation, push_retry, reject_prose, reply_text,
    warm_up_request,
};
use super::render::{FormulaImage, render, render_preview};
use crate::config::*;
use lvtex_render::math::Renderer;
use serde_json::Value;
use std::{
    collections::HashMap,
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::Duration,
};

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
            .name("lvtex-math".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
                let mut cache: HashMap<String, Arc<FormulaImage>> = HashMap::new();
                let send = |reply| {
                    let _ = replies.send(reply);
                    wake();
                };
                let warm = warm_up_request();
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
            .name("lvtex-render".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let mut renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
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
                                render_preview(renderer, &preview.display_latex)
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
        *renderer = Renderer::with_prelude(lvtex_latex::PRELUDE);
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
    let mut messages = conversation(prompt);
    let mut last = String::new();
    for _ in 0..3 {
        let response = request(&chat_request(&messages))?;
        let raw = reply_text(&response);
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
                push_retry(&mut messages, raw, &error);
                last = error;
            }
        }
    }
    Err(last.into())
}

#[cfg(test)]
#[path = "../../tests/unit/math/worker.rs"]
mod tests;
