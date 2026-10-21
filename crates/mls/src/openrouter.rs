//! OpenRouter: the Jev decision and the Haiku formalization, sent through curl.

use crate::{Failure, Formal, HAIKU_MODEL, JEV_MODEL, Label, Tag};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    io::Write,
    process::{Command, Stdio},
};

pub(crate) const DECISIONS_URL: &str = "https://openrouter.ai/api/alpha/decisions";
pub(crate) const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const KEY_VAR: &str = "OPENROUTER_API_KEY";
/// Bounds each request, so a stalled call cannot hold the worker indefinitely.
const TIMEOUT_SECONDS: u32 = 120;

const CLASSIFY: &str = "Classify one paragraph of a mathematical document. When proof_in_progress is true, the paragraph follows an unfinished proof: answer proof if it continues that argument.";

const JOINS: &str = "Decide whether this paragraph belongs to the same mathematical item (a definition, a statement or a proof) as previous_paragraph, so that the two must be formalized together.";

const SYSTEM: &str = r#"You formalize one block of a mathematical document into Lean 4 with Mathlib.

You receive the Lean already accepted for the blocks above, in document order, each under a `-- mathstral:block <id>` comment; the prose paragraph just above the block, if any; the block's label; and the block itself, written in Markdown with LaTeX between dollar signs. A block may span several paragraphs, separated by blank lines: its setup, displayed formulas and clauses belong to it, so formalize them together as one item.

Return only a JSON object with three fields:
- "lean": Lean 4 source for this block alone. It is checked in an environment that already contains `import Mathlib` and the Lean above, so never write `import`, and never repeat a declaration from above.
- "lean_name": the name of the main declaration, or null if there is none.
- "assumptions": every assumption of your Lean, in plain language that a mathematician can check against the block: the types chosen (for example "n is a natural number, so n ≥ 0"), each hypothesis, quantifier and domain, and any Lean convention that changes the meaning (for example "division by zero is 0" or "subtraction on ℕ is truncated"). Write them from the Lean, not from the block. One short sentence each.

Conventions by label:
- definition: a `def`, `abbrev`, `structure`, `class` or `inductive`; use `noncomputable def` when needed. No `sorry`.
- statement (a lemma, theorem, corollary or proposition): exactly one `theorem` with a fresh, descriptive snake_case name, whose whole proof is `by sorry`, so the source ends with `:= by sorry`.
- proof: restate the most recent statement exactly as it appears in the Lean above, with the same name, binders and type, character for character up to `:=`, and replace `sorry` with a complete proof. Never weaken or change the statement to make it provable. If the argument is wrong or incomplete, still give your most faithful proof attempt and let Lean reject it.

Rules:
- Never use `axiom`, `admit`, `sorry` (except as the whole proof of a statement), `native_decide`, `set_option`, `unsafe`, `#eval` or `#print`.
- Make each declaration self-contained with explicit binders. Do not use `variable`, `section`, `namespace` or `open ... in` blocks that span declarations.
- Reuse definitions from the Lean above by name instead of redefining them, and prefer Mathlib's existing notions (Group, Filter.Tendsto, Real, Finset) over new ones.
- Use standard Mathlib names and notation (ℕ, ℤ, ℝ, ∀, ∃, →). Prefer robust tactics such as simp, ring, linarith, nlinarith, omega, positivity, field_simp and gcongr, together with explicit Mathlib lemmas.
- Keep the formalization faithful to the block: the same objects, hypotheses and conclusion."#;

fn service_failure(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        service: true,
    }
}

/// A failure of one block's reply. The service itself is still usable.
fn block_failure(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        service: false,
    }
}

/// The key from the environment. It is never put in an argument, a file or a message.
fn checked_key(key: Option<&str>) -> Result<&str, Failure> {
    let key = match key {
        Some(key) if !key.trim().is_empty() => key,
        _ => {
            return Err(service_failure(
                "OPENROUTER_API_KEY is not set. Export it and launch Mathstral from that terminal.",
            ));
        }
    };
    // A key with these characters would break the quoted curl header or config line.
    if key
        .chars()
        .any(|c| matches!(c, '"' | '\\') || c.is_whitespace() || c.is_control())
    {
        return Err(service_failure("OPENROUTER_API_KEY is malformed."));
    }
    Ok(key)
}

/// Escapes a value for a double-quoted curl config string.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// The curl config that sends `body` to `url`. curl reads it from stdin, so the key
/// stays out of the process arguments and off the disk.
fn curl_config(url: &str, key: &str, body: &str) -> String {
    format!(
        "url = \"{}\"\nheader = \"Authorization: Bearer {}\"\nheader = \"Content-Type: application/json\"\nheader = \"X-Title: Mathstral\"\ndata-binary = \"{}\"\n",
        escape(url),
        escape(key),
        escape(body),
    )
}

/// The reply of a curl call: the body, then the HTTP status on the last line.
fn response(stdout: &str) -> Result<Value, Failure> {
    let (body, status) = stdout.rsplit_once('\n').unwrap_or(("", stdout));
    let status = status.trim();
    if status != "200" {
        let detail = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|json| json["error"]["message"].as_str().map(str::to_string))
            .unwrap_or_else(|| body.chars().take(200).collect());
        return Err(service_failure(format!(
            "OpenRouter answered {status}: {detail}"
        )));
    }
    serde_json::from_str(body).map_err(|_| service_failure("OpenRouter sent invalid JSON."))
}

/// Sends `body` to `url` through curl and returns the JSON reply.
pub(crate) fn post(url: &str, body: &Value) -> Result<Value, Failure> {
    let key = std::env::var(KEY_VAR).ok();
    let key = checked_key(key.as_deref())?;
    let timeout = TIMEOUT_SECONDS.to_string();
    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            timeout.as_str(),
            "--config",
            "-",
            "--write-out",
            "\n%{http_code}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| service_failure(format!("curl is not available: {error}")))?;
    let config = curl_config(url, key, &body.to_string());
    // Spawned above with piped stdin, so the handle is always present.
    let mut stdin = child.stdin.take().expect("stdin was piped at spawn");
    if let Err(error) = stdin.write_all(config.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(service_failure(format!("curl failed: {error}")));
    }
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| service_failure(format!("curl failed: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(service_failure(format!(
            "OpenRouter is unreachable: {}",
            stderr.trim()
        )));
    }
    response(&String::from_utf8_lossy(&output.stdout))
}

/// The Jev request that asks for the kind of one paragraph and whether it continues the one above.
/// `previous` is None for the first paragraph; `proof_open` tells Jev whether the paragraph may
/// continue an unfinished proof.
pub(crate) fn classify_request(previous: Option<&str>, text: &str, proof_open: bool) -> Value {
    json!({
        "model": JEV_MODEL,
        "state": {
            "previous_paragraph": previous,
            "paragraph": text,
            "proof_in_progress": proof_open,
        },
        "questions": {
            "kind": {
                "type": "choice",
                "instructions": CLASSIFY,
                "criteria": {
                    (Label::Definition.name()): "Defines a new mathematical object, notion or notation.",
                    (Label::Statement.name()): "States a lemma, theorem, corollary or proposition that is meant to be proved.",
                    (Label::Proof.name()): "Proves the preceding statement, or continues a proof in progress.",
                    (Label::Skip.name()): "Anything else: prose, motivation, examples, remarks, headings or a bare formula.",
                }
            },
            "joins": {
                "type": "choice",
                "instructions": JOINS,
                "criteria": {
                    "continues": "Belongs to the same definition, statement or proof as the previous paragraph: its setup (\"Let G be a group.\" followed by the claim), a displayed formula, a clause or list item, or the rest of a sentence the previous paragraph started.",
                    "new": "Starts a new definition, statement or proof, or is unrelated prose, or previous_paragraph is null.",
                }
            }
        }
    })
}

/// What Jev chose for one paragraph. A missing or unknown label, or a missing answer about joining,
/// is a service failure, not a guess.
pub(crate) fn parse_tag(response: &Value) -> Result<Tag, Failure> {
    let label = response["answers"]["kind"]["choice"]
        .as_str()
        .and_then(Label::from_name)
        .ok_or_else(|| service_failure("Jev gave no label."))?;
    let joins = match response["answers"]["joins"]["choice"].as_str() {
        Some("continues") => true,
        Some("new") => false,
        _ => {
            return Err(service_failure(
                "Jev did not say whether the paragraph continues the one above.",
            ));
        }
    };
    Ok(Tag { label, joins })
}

/// The Haiku request that formalizes one block. The system prompt and the last Lean
/// segment carry cache_control, so an edit in the middle of the document invalidates
/// only the cached suffix after it.
pub(crate) fn formalize_request(
    label: Label,
    text: &str,
    prose: Option<&str>,
    lean_above: &[(u64, String)],
    statement: Option<&str>,
) -> Value {
    let system = json!({"role": "system", "content": [
        {"type": "text", "text": SYSTEM, "cache_control": {"type": "ephemeral"}}
    ]});
    let mut parts: Vec<Value> = lean_above
        .iter()
        .map(|(id, lean)| json!({"type": "text", "text": format!("-- mathstral:block {id}\n{lean}\n")}))
        .collect();
    if let Some(last) = parts.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    let statement = statement
        .map(|statement| format!("Most recent statement:\n{statement}\n\n"))
        .unwrap_or_default();
    let prompt = format!(
        "Prose just above:\n{}\n\nLabel: {}\n\n{statement}Block:\n{text}",
        prose.unwrap_or("(none)"),
        label.name(),
    );
    parts.push(json!({"type": "text", "text": prompt}));
    json!({
        "model": HAIKU_MODEL,
        "temperature": 0,
        "max_tokens": 8192,
        "messages": [system, {"role": "user", "content": parts}],
        "response_format": {"type": "json_schema", "json_schema": {
            "name": "formalization",
            "strict": true,
            "schema": schema(),
        }}
    })
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "lean": {"type": "string"},
            "lean_name": {"anyOf": [{"type": "string"}, {"type": "null"}]},
            "assumptions": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["lean", "lean_name", "assumptions"],
        "additionalProperties": false,
    })
}

/// The reply without a Markdown code fence: the opening fence line and a closing fence go.
fn unfenced(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let body = rest.split_once('\n').map_or("", |(_, body)| body);
    body.trim_end().strip_suffix("```").unwrap_or(body).trim()
}

/// The formalization in Haiku's reply. Rejects the reply as a whole if a field is
/// missing or of the wrong type; empty names and assumptions are dropped.
pub(crate) fn parse_formal(response: &Value) -> Result<Formal, Failure> {
    let choice = &response["choices"][0];
    if choice["finish_reason"] == "length" {
        return Err(block_failure("The formalization was cut off."));
    }
    let content: Cow<str> = match &choice["message"]["content"] {
        Value::String(text) => Cow::Borrowed(text.as_str()),
        Value::Array(parts) => Cow::Owned(
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect(),
        ),
        _ => return Err(service_failure("Haiku sent no content.")),
    };
    let json = serde_json::from_str::<Value>(unfenced(content.trim()))
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| block_failure("Haiku did not return valid JSON."))?;
    let lean = json["lean"]
        .as_str()
        .filter(|lean| !lean.trim().is_empty())
        .ok_or_else(|| block_failure("Haiku's JSON has no `lean`."))?
        .trim_end()
        .to_string();
    let lean_name = match json.get("lean_name") {
        Some(Value::Null) => None,
        Some(Value::String(name)) => (!name.trim().is_empty()).then(|| name.trim().to_string()),
        _ => return Err(block_failure("Haiku's JSON has no `lean_name`.")),
    };
    let assumptions = json
        .get("assumptions")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
        .ok_or_else(|| block_failure("Haiku's JSON has no `assumptions`."))?
        .into_iter()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Formal {
        lean,
        lean_name,
        assumptions,
    })
}

#[cfg(test)]
#[path = "../tests/unit/openrouter.rs"]
mod tests;
