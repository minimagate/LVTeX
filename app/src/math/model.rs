//! The Ollama model: system prompt, examples, requests, and cleaning and checking its replies.

use crate::config::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

const SYSTEM: &str = r#"Convert the user's mathematical description to LaTeX. Output ONLY the LaTeX math body. No dollar signs, delimiters, Markdown fences, explanations, labels, or prose. Use standard amsmath commands. If input is already LaTeX, clean it up. Preserve the intended mathematics; do not evaluate unless asked. Use aligned (not align) for multiple equations. Never include document commands or packages. Follow the examples."#;

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

pub(super) struct Error {
    pub(super) message: String,
    pub(super) offline: bool,
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            message,
            offline: false,
        }
    }
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

/// The empty chat that loads the model without generating text.
pub(super) fn warm_up_request() -> Value {
    json!({"model": OLLAMA_MODEL, "messages": [], "stream": false,
    "keep_alive": KEEP_ALIVE, "options": {"temperature": 0}})
}

/// The system prompt, the worked examples, then the user's prompt.
pub(super) fn conversation(prompt: &str) -> Vec<Value> {
    let mut messages = vec![json!({"role":"system", "content":SYSTEM})];
    for (input, output) in EXAMPLES {
        messages.push(json!({"role":"user", "content":input}));
        messages.push(json!({"role":"assistant", "content":output}));
    }
    messages.push(json!({"role":"user", "content":prompt}));
    messages
}

/// The generation request for a conversation, with deterministic options.
pub(super) fn chat_request(messages: &[Value]) -> Value {
    json!({"model": OLLAMA_MODEL, "stream": false, "keep_alive": KEEP_ALIVE,
    "options": {"temperature":0, "num_predict":2048}, "messages":messages})
}

/// The text of the model's reply, or empty when the reply has none.
pub(super) fn reply_text(response: &Value) -> &str {
    response["message"]["content"].as_str().unwrap_or("")
}

/// Appends the model's rejected reply and the rendering error, asking for a correction.
pub(super) fn push_retry(messages: &mut Vec<Value>, raw: &str, error: &str) {
    messages.push(json!({"role":"assistant", "content":raw}));
    messages.push(json!({"role":"user", "content":format!("Rendering failed: {error}. Correct the expression. Return only the LaTeX math body, without prose or delimiters.")}));
}

pub(super) fn reject_prose(latex: &str) -> Result<(), String> {
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
pub(super) fn chat(body: &Value) -> Result<Value, Error> {
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
            let framed = size.checked_add(2).ok_or("invalid chunk size".to_owned())?;
            if rest.len() < framed {
                return Err("incomplete chunk".to_owned().into());
            }
            decoded.extend_from_slice(&rest[..size]);
            rest = &rest[framed..];
        }
        bytes = decoded;
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_latex_cannot_be_prose() {
        assert!(reject_prose("The answer is x squared").is_err());
        assert!(reject_prose(r"\text{otherwise} + \operatorname{variance}(X)").is_ok());
    }

    #[test]
    fn requests_carry_model_prompt_and_stream_flag() {
        let request = chat_request(&conversation("x"));
        assert_eq!(request["model"], OLLAMA_MODEL);
        assert_eq!(request["stream"], false);
        let messages = request["messages"].as_array().unwrap();
        assert_eq!(messages[0]["content"], SYSTEM);
        assert_eq!(messages.last().unwrap()["content"], "x");
        assert!(warm_up_request()["messages"].as_array().unwrap().is_empty());
    }
}
