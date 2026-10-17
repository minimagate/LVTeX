//! Background-only conversion and rendering. Nothing in this module runs on a key event.
use crate::config::*;
use mathstral_render_spike::math::Renderer;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

#[derive(Debug)]
pub struct Image {
    pub rgba: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub baseline: f32,
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
    pub describe: bool,
}
pub enum Reply {
    Connection(bool),
    Described(u64, Result<String, String>),
    Done(u64, Result<Arc<FormulaImage>, String>),
}
pub struct Worker {
    tx: mpsc::Sender<Job>,
    pub rx: mpsc::Receiver<Reply>,
}

pub const SYSTEM: &str = r#"Convert the user's mathematical description to LaTeX. Output ONLY the LaTeX math body. No dollar signs, delimiters, Markdown fences, explanations, labels, or prose. Use standard amsmath commands. If input is already LaTeX, clean it up. Preserve the intended mathematics; do not evaluate unless asked. Use aligned (not align) for multiple equations. Never include document commands or packages. Follow the examples."#;
const EXAMPLES: &[(&str, &str)] = &[
    ("integral from 0 to 1 of x^2 dx", r"\int_0^1 x^2\,dx"),
    (
        "sum n squared for n from 1 to infinity",
        r"\sum_{n=1}^{\infty} n^2",
    ),
    ("1 over 3", r"\frac{1}{3}"),
    (
        "matrix rows 1 2 and 3 4",
        r"\begin{pmatrix}1&2\\3&4\end{pmatrix}",
    ),
    (
        "f(x) is x squared if x is nonnegative and minus x otherwise",
        r"f(x)=\begin{cases}x^2&x\geq0\\-x&x<0\end{cases}",
    ),
    (
        "align x+y=3 and x-y=1 on separate lines",
        r"\begin{aligned}x+y&=3\\x-y&=1\end{aligned}",
    ),
    (
        "set of real x such that x is greater than 0",
        r"\{x\in\mathbb{R}\mid x>0\}",
    ),
    ("alpha plus beta equals gamma", r"\alpha+\beta=\gamma"),
];

impl Worker {
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        Self::with_backend(wake, chat, Duration::from_secs(2))
    }
    fn with_backend(
        wake: impl Fn() + Send + 'static,
        mut request: impl FnMut(&Value) -> Result<Value, Error> + Send + 'static,
        retry_delay: Duration,
    ) -> Self {
        let (tx, jobs) = mpsc::channel::<Job>();
        let (replies, rx) = mpsc::channel();
        thread::Builder::new()
            .name("mathstral-math".into())
            .spawn(move || {
                let mut renderer = Renderer::with_prelude(latex_spike::PRELUDE);
                let mut cache: HashMap<String, Arc<FormulaImage>> = HashMap::new();
                let send = |reply| {
                    let _ = replies.send(reply);
                    wake();
                };
                // An empty chat loads the model without generating text.
                let warm = json!({"model": OLLAMA_MODEL, "messages": [], "stream": false,
                "keep_alive": KEEP_ALIVE, "options": {"temperature": 0}});
                let mut online = request(&warm).is_ok();
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
                            // Imported formulas and cache hits can render without Ollama.
                            let mut rest = std::collections::VecDeque::new();
                            while let Some(job) = pending.pop_front() {
                                if !job.describe
                                    && (job.latex.is_some()
                                        || job
                                            .prompt
                                            .as_ref()
                                            .is_some_and(|p| cache.contains_key(p)))
                                {
                                    let result =
                                        resolve_with(&job, &mut renderer, &mut cache, &mut request);
                                    send(Reply::Done(job.id, result.map_err(|e| e.message)));
                                } else {
                                    rest.push_back(job);
                                }
                            }
                            pending = rest;
                            online = request(&warm).is_ok();
                            if online {
                                send(Reply::Connection(true));
                            }
                            continue;
                        }
                    }
                    if let Some(job) = pending.pop_front() {
                        if job.describe {
                            match describe(job.latex.as_deref().unwrap_or(""), &mut request) {
                                Ok(prompt) => send(Reply::Described(job.id, Ok(prompt))),
                                Err(error) if error.offline => {
                                    online = false;
                                    send(Reply::Connection(false));
                                    pending.push_front(job);
                                }
                                Err(error) => send(Reply::Described(job.id, Err(error.message))),
                            }
                            continue;
                        }
                        match resolve_with(&job, &mut renderer, &mut cache, &mut request) {
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
        Self { tx, rx }
    }
    pub fn submit(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}
struct Error {
    message: String,
    offline: bool,
}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            message,
            offline: false,
        }
    }
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
        match render(renderer, &latex) {
            Ok(image) => {
                let image = Arc::new(image);
                if cache.len() >= 512 {
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

fn describe(
    latex: &str,
    mut request: impl FnMut(&Value) -> Result<Value, Error>,
) -> Result<String, Error> {
    let response = request(
        &json!({"model":OLLAMA_MODEL,"stream":false,"keep_alive":KEEP_ALIVE,"options":{"temperature":0,"num_predict":512},
        "messages":[{"role":"system","content":"Describe this mathematical expression as a concise, precise plain English math-writing prompt. Preserve every symbol, bound, index and equation. Output only the description, with no introduction, delimiters, slashes, backslashes, dollar signs or LaTeX commands. For example, \\frac{1}{3} becomes: 1 over 3."},{"role":"user","content":latex}]}),
    )?;
    let prompt = response["message"]["content"].as_str().unwrap_or("").trim();
    if prompt.is_empty() || prompt.contains(['\\', '$', '/', '`']) {
        return Err("description was not plain text".to_owned().into());
    }
    Ok(prompt.to_owned())
}

pub fn clean(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(start) = s.find("```") {
        let fenced = &s[start + 3..];
        if let Some(end) = fenced.find("```") {
            s = &fenced[..end];
            for language in ["latex", "tex", "math"] {
                if let Some(rest) = s.strip_prefix(language) {
                    s = rest;
                    break;
                }
            }
        }
    }
    for (open, close) in [("\\[", "\\]"), ("\\(", "\\)"), ("$$", "$$"), ("$", "$")] {
        if let Some(start) = s.find(open) {
            let rest = &s[start + open.len()..];
            if let Some(end) = rest.find(close) {
                s = &rest[..end];
                break;
            }
        }
    }
    let lines: Vec<_> = s
        .lines()
        .filter(|line| {
            let lower = line.trim().to_lowercase();
            ![
                "here is",
                "here's",
                "the latex",
                "latex:",
                "this ",
                "where ",
                "note:",
                "explanation:",
            ]
            .iter()
            .any(|p| lower.starts_with(p))
        })
        .collect();
    lines.join("\n").trim().to_owned()
}

pub fn render(renderer: &mut Renderer, latex: &str) -> Result<FormulaImage, String> {
    if latex.is_empty() || latex.len() > 16384 {
        return Err("empty or oversized expression".into());
    }
    reject_prose(latex)?;
    let body = latex_spike::convert(latex)?;
    let mut image = |display| {
        let equation = if display {
            format!("$ {body} $")
        } else {
            format!("#math.equation(block: true, math.inline(${body}$))")
        };
        let pixels = renderer.render_source(format!(
            "#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(font: \"Libertinus Serif\", size: {FONT_SIZE}pt, fill: rgb(\"{TEXT_COLOR:06x}\"))\n#show math.equation: set text(font: \"New Computer Modern Math\")\n{}\n{equation}", latex_spike::PRELUDE), 2.0)?;
        Ok::<_, String>(Image {
            width: pixels.width() as usize,
            height: pixels.height() as usize,
            rgba: pixels.data().to_vec(),
            baseline: renderer.last_baseline as f32,
        })
    };
    Ok(FormulaImage {
        latex: latex.into(),
        inline: image(false)?,
        display: image(true)?,
    })
}

fn reject_prose(latex: &str) -> Result<(), String> {
    let bytes = latex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let command = &latex[start..i];
            if start == i {
                if i < bytes.len() {
                    i += latex[i..].chars().next().unwrap().len_utf8();
                }
                continue;
            }
            if [
                "text",
                "operatorname",
                "begin",
                "end",
                "mathrm",
                "mathbf",
                "mathbb",
                "mathcal",
                "mathsf",
                "textnormal",
            ]
            .contains(&command)
            {
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'{') {
                    let mut depth = 1;
                    i += 1;
                    while i < bytes.len() && depth > 0 {
                        match bytes[i] {
                            b'{' => depth += 1,
                            b'}' => depth -= 1,
                            _ => {}
                        };
                        i += 1;
                    }
                }
            }
        } else if bytes[i].is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = &latex[start..i];
            if word.len() > 3
                && !["arcsin", "arccos", "arctan", "sinh", "cosh", "tanh"].contains(&word)
            {
                return Err("output contains prose; use only a LaTeX math body".into());
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

// Ollama is loopback HTTP only. Timeouts and I/O are confined to the worker.
fn chat(body: &Value) -> Result<Value, Error> {
    let io_error = |e: std::io::Error| Error {
        message: e.to_string(),
        offline: true,
    };
    let addr: SocketAddr = OLLAMA_ADDRESS.parse().unwrap();
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(io_error)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(io_error)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(io_error)?;
    let body = body.to_string();
    write!(stream, "POST /api/chat HTTP/1.1\r\nHost: localhost:11434\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).map_err(io_error)?;
    let mut response = Vec::new();
    stream
        .take(2 * 1024 * 1024)
        .read_to_end(&mut response)
        .map_err(io_error)?;
    let split = response
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .ok_or_else(|| Error::from("invalid HTTP response".to_owned()))?;
    let headers = String::from_utf8_lossy(&response[..split]);
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        return Err(Error {
            message: String::from_utf8_lossy(&response[split + 4..]).into(),
            offline: true,
        });
    }
    let mut bytes = response[split + 4..].to_vec();
    if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut decoded = Vec::new();
        let mut rest = bytes.as_slice();
        loop {
            let end = rest
                .windows(2)
                .position(|w| w == b"\r\n")
                .ok_or("invalid chunk".to_owned())?;
            let size = usize::from_str_radix(
                std::str::from_utf8(&rest[..end])
                    .unwrap_or("")
                    .split(';')
                    .next()
                    .unwrap(),
                16,
            )
            .map_err(|_| "invalid chunk size".to_owned())?;
            if size == 0 {
                break;
            }
            rest = &rest[end + 2..];
            if rest.len() < size + 2 {
                return Err("incomplete chunk".to_owned().into());
            }
            decoded.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
        bytes = decoded;
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string().into())
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
                describe: false,
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
        assert_eq!(ids, (0..10).collect());
    }
    #[test]
    fn imported_edit_descriptions_cannot_expose_latex() {
        assert_eq!(
            describe("x^2", |_| Ok(json!({"message":{"content":"x squared"}})))
                .ok()
                .as_deref(),
            Some("x squared")
        );
        assert!(describe("x^2", |_| Ok(json!({"message":{"content":"$x^2$"}}))).is_err());
        assert!(reject_prose("The answer is x squared").is_err());
        assert!(reject_prose(r"\text{otherwise} + \operatorname{variance}(X)").is_ok());
    }
    #[test]
    fn cleanup_and_rendered_repair_then_cache() {
        assert_eq!(
            clean("Here is the result:\n```latex\n$\\frac{1}{3}$\n```\nExplanation: a third"),
            r"\frac{1}{3}"
        );
        assert_eq!(clean(r"Here it is: \[x^2\] This is squared."), "x^2");
        let mut renderer = Renderer::with_prelude(latex_spike::PRELUDE);
        let mut cache = HashMap::new();
        let mut calls = 0;
        let job = Job {
            id: 1,
            prompt: Some("one third".into()),
            latex: None,
            describe: false,
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
            describe: false,
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
}
