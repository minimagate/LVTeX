//! OpenRouter: the Jev decision, the split of a paragraph and the formalization, sent through curl.

use crate::{FORMALIZER_MODEL, Failure, Formal, JEV_MODEL, Label, Tag};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    io::Write,
    process::{Command, Stdio},
};

pub(crate) const DECISIONS_URL: &str = "https://openrouter.ai/api/alpha/decisions";
pub(crate) const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
/// Bounds each request, so a stalled call cannot hold the worker indefinitely.
const TIMEOUT_SECONDS: u32 = 120;

/// Neutral about `proof_in_progress` on purpose: a proof often ends without ∎, and an instruction
/// that favours proof while one is open makes every later claim a proof.
const CLASSIFY: &str = "Classify one paragraph of a mathematical document, or one part of a paragraph. block_above is the block of one or more paragraphs just above it. proof_in_progress is true when a proof above has not ended with ∎; that proof may already be complete. The paragraph is a proof only if it argues for the statement being proved. A new claim is a statement even right after a proof, for example one that starts with \"It follows that\" or \"One might guess that\". A remark about the mathematics is skip.";

const JOINS: &str = "Decide whether this paragraph belongs to the same mathematical item (a definition, a statement or a proof) as block_above, so that they must be formalized together. Sharing a topic is not the same item: prose that motivates, previews or comments on a result is a separate item from that result, even when the result follows right after it. A named new lemma, theorem, corollary, definition or proof begins a new item, even directly after prose. Continue only when this paragraph completes block_above: a setup that is still incomplete (\"Let x be ...\" or hypotheses) followed by the claim it sets up, a standalone displayed formula or clause of the statement, or a further paragraph of a proof that is still going.";

const ITEMS: &str = "Decide whether this paragraph holds several items that must be checked separately, for example a statement followed by its proof, a definition followed by a claim about it, or two separate claims. Setup, hypotheses, a heading such as \"Lemma.\" and remarks belong to the item they serve. A statement with its clauses is one item, and so is a proof however many steps it has.";

const SPLIT: &str = "Cut one paragraph of a mathematical document into its items, in order: each definition, each statement and each proof. Every piece is a verbatim, contiguous part of the paragraph. Together the pieces are the whole paragraph, in order, with nothing left out, added, reordered or reworded. Setup that a definition or statement needs stays with it. Connecting words and remarks go with the item next to them. Return only a JSON object with the field \"pieces\": the array of pieces.";

const SYSTEM: &str = r#"You formalize one block of a mathematical document into Lean 4 with Mathlib.

You receive the Lean already accepted for the blocks above, in document order, each under a `-- mathstral:block <id>` comment; the prose paragraph just above the block, if any; the block's label; and the block itself, written in Markdown with LaTeX between dollar signs. A block may span several paragraphs, separated by blank lines: its setup, displayed formulas and clauses belong to it, so formalize them together as one item.

Return only a JSON object with three fields:
- "lean": Lean 4 source for this block alone. It is checked in an environment that already contains `import Mathlib` and the Lean above, so never write `import`, and never repeat a declaration from above. The one exception is a proof block restating its statement, described under proof.
- "lean_name": the name of the main declaration, or null if there is none.
- "assumptions": every assumption of your Lean, in plain language that a mathematician can check against the block: the types chosen (for example "n is a natural number, so n ≥ 0"), each hypothesis, quantifier and domain, and any Lean convention that changes the meaning (for example "division by zero is 0" or "subtraction on ℕ is truncated"). Write them from the Lean, not from the block. One short sentence each.

Conventions by label:
- definition: a `def`, `abbrev`, `structure`, `class` or `inductive`; use `noncomputable def` when needed. No `sorry`. If the block only fixes what letters stand for (for example "Throughout, n is a natural number") and introduces no new notion, return an empty "lean", null "lean_name" and no assumptions.
- statement (a lemma, theorem, corollary or proposition): exactly one `theorem` with a fresh, descriptive snake_case name, whose whole proof is `by sorry`, so the source ends with `:= by sorry`.
- proof: the only block that repeats a declaration. Its "lean" is never empty: it restates the statement given under "Most recent statement" in the prompt, with the same name, binders and type, character for character up to `:=`, and replaces `sorry` with a complete proof. The pending statement, the `by sorry` version of that statement, is absent where this proof is checked, so the proof must not use it to prove itself. Never weaken or change the statement to make it provable. If the argument is wrong or incomplete, still give your most faithful proof attempt and let Lean reject it.

Rules:
- Never use `axiom`, `admit`, `sorry` (except as the whole proof of a statement), `native_decide`, `set_option`, `unsafe`, `#eval` or `#print`.
- Make each declaration self-contained with explicit binders. Do not use `variable`, `section`, `namespace` or `open ... in` blocks that span declarations.
- Reuse definitions and results from the Lean above by their exact name and signature instead of redefining or reproving them: a corollary cites the theorem it follows from. Never cite a name you have not seen above or do not know to exist in Mathlib. Prefer Mathlib's existing notions (Group, Filter.Tendsto, Real, Finset) over new ones.
- Use standard Mathlib names and notation (ℕ, ℤ, ℝ, ∀, ∃, →). Prefer robust tactics such as simp, ring, linarith, nlinarith, omega, positivity, field_simp and gcongr, together with explicit Mathlib lemmas.
- Keep the formalization faithful to the block: the same objects, hypotheses and conclusion.
- Formalize the claim the block asserts or conjectures, even if it is false. Words such as "hope" or "guess" do not weaken the conclusion. Finite examples or computations never replace a universal claim, and never add a bound or hypothesis that the block does not state. A statement keeps its exact form with `by sorry` as its proof, whether or not it is true; a later proof may then fail in Lean."#;

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

/// The caller's key, checked before use. It is never put in an argument, a file or a message.
fn checked_key(key: Option<&str>) -> Result<&str, Failure> {
    let key = match key {
        Some(key) if !key.trim().is_empty() => key,
        _ => {
            return Err(service_failure(
                "No OpenRouter key. Add one in Mathstral → Settings, or export OPENROUTER_API_KEY and launch Mathstral from that terminal.",
            ));
        }
    };
    // A key with these characters would break the quoted curl header or config line.
    if key
        .chars()
        .any(|c| matches!(c, '"' | '\\') || c.is_whitespace() || c.is_control())
    {
        return Err(service_failure("The OpenRouter key is malformed."));
    }
    Ok(key)
}

/// Whether `key` is usable as an OpenRouter key. The app calls it before saving a key, so the rule
/// has one owner.
pub fn key_is_valid(key: &str) -> bool {
    checked_key(Some(key)).is_ok()
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

/// Sends `body` to `url` through curl with `key` and returns the JSON reply.
pub(crate) fn post(url: &str, body: &Value, key: Option<&str>) -> Result<Value, Failure> {
    let key = checked_key(key)?;
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

/// The Jev request that asks for the kind of one paragraph (or piece of one), whether it continues
/// the one above and whether it holds several items. `previous` is the block above (see
/// `Job::Classify`), None for the first paragraph; `proof_open` tells Jev whether the paragraph may
/// continue an unfinished proof.
pub(crate) fn classify_request(previous: Option<&str>, text: &str, proof_open: bool) -> Value {
    json!({
        "model": JEV_MODEL,
        "state": {
            "block_above": previous,
            "paragraph": text,
            "proof_in_progress": proof_open,
        },
        "questions": {
            "kind": {
                "type": "choice",
                "instructions": CLASSIFY,
                "criteria": {
                    (Label::Definition.name()): "Introduces and names a new notion, object or notation, for example \"We call n even when …\". A convention that only fixes what letters stand for, such as \"Throughout, n is a natural number\", is skip.",
                    (Label::Statement.name()): "Asserts a claim that is meant to be proved: a lemma, theorem, corollary or proposition, or a new claim in prose such as \"It follows that …\" or \"One might guess that … holds\".",
                    (Label::Proof.name()): "Argues for the statement being proved, with reasons or computations, or continues that argument.",
                    (Label::Skip.name()): "Anything else: prose, motivation, examples, remarks, headings, conventions that fix notation, or a bare formula.",
                }
            },
            "joins": {
                "type": "choice",
                "instructions": JOINS,
                "criteria": {
                    "continues": "Completes the same definition, statement or proof as block_above: the claim that an incomplete setup (\"Let G be a group.\" or hypotheses) was waiting for, a standalone displayed formula or clause of the statement, or a further paragraph of a proof that is still going.",
                    "new": "Starts a new definition, statement or proof; or is motivational, introductory or other topical prose, even when it leads straight into the next result; or is unrelated prose; or block_above is null.",
                }
            },
            "items": {
                "type": "choice",
                "instructions": ITEMS,
                "criteria": {
                    "one": "A single definition, statement or proof with the setup, heading and remarks that go with it; or prose that is not an item.",
                    "several": "Two or more items that must be checked separately, for example a statement and its proof, a definition and a claim about it, or two separate claims.",
                }
            }
        }
    })
}

/// What Jev chose for one paragraph. A missing or unknown label, or a missing answer about joining
/// or about several items, is a service failure, not a guess.
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
    let split = match response["answers"]["items"]["choice"].as_str() {
        Some("several") => true,
        Some("one") => false,
        _ => {
            return Err(service_failure(
                "Jev did not say whether the paragraph holds several items.",
            ));
        }
    };
    Ok(Tag {
        label,
        joins,
        split,
    })
}

/// The request that cuts one paragraph into its items (see `Job::Split`).
pub(crate) fn split_request(text: &str) -> Value {
    json!({
        "model": FORMALIZER_MODEL,
        "temperature": 0,
        "max_tokens": 8192,
        "messages": [
            {"role": "system", "content": SPLIT},
            {"role": "user", "content": text},
        ],
        "response_format": {"type": "json_schema", "json_schema": {
            "name": "split",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {"pieces": {"type": "array", "items": {"type": "string"}}},
                "required": ["pieces"],
                "additionalProperties": false,
            },
        }}
    })
}

/// The request that formalizes one block. The system prompt and the last Lean
/// segment carry cache_control, so an edit in the middle of the document invalidates
/// only the cached suffix after it. `repair` is the failed Lean and Lean's error for a proof that
/// is asked again (see `Job::Repair`).
pub(crate) fn formalize_request(
    label: Label,
    text: &str,
    prose: Option<&str>,
    lean_above: &[(u64, String)],
    statement: Option<&str>,
    repair: Option<(&str, &str)>,
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
    let repair = repair
        .map(|(failed, error)| {
            format!(
                "\n\nThe Lean formalized for this proof before failed to check:\n{failed}\n\nLean's error:\n{error}\n\nReturn a repaired formalization of the same proof. Fix only what the error shows: keep the statement's name, binders and type exactly, keep the hypotheses and the argument of the block, and never use sorry."
            )
        })
        .unwrap_or_default();
    let prompt = format!(
        "Prose just above:\n{}\n\nLabel: {}\n\n{statement}Block:\n{text}{repair}",
        prose.unwrap_or("(none)"),
        label.name(),
    );
    parts.push(json!({"type": "text", "text": prompt}));
    json!({
        "model": FORMALIZER_MODEL,
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

/// The JSON object in the model's reply to a request for `what`. A reply that was cut off, or whose
/// content is not an object, is a failure of that one block; a reply with no content is the
/// service's.
fn reply_object(response: &Value, what: &str) -> Result<Value, Failure> {
    let choice = &response["choices"][0];
    if choice["finish_reason"] == "length" {
        return Err(block_failure(format!("The {what} was cut off.")));
    }
    let content: Cow<str> = match &choice["message"]["content"] {
        Value::String(text) => Cow::Borrowed(text.as_str()),
        Value::Array(parts) => Cow::Owned(
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect(),
        ),
        _ => return Err(service_failure("The formalizer sent no content.")),
    };
    serde_json::from_str::<Value>(unfenced(content.trim()))
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| block_failure("The formalizer did not return valid JSON."))
}

/// The formalization in the model's reply. Rejects the reply as a whole if a field is
/// missing or of the wrong type; empty names and assumptions are dropped.
pub(crate) fn parse_formal(response: &Value) -> Result<Formal, Failure> {
    let json = reply_object(response, "formalization")?;
    // Blank Lean is an answer: a definition block that only fixes notation declares nothing.
    // `rules::reject` decides where blank Lean is allowed.
    let lean = json["lean"]
        .as_str()
        .ok_or_else(|| block_failure("The formalizer's JSON has no `lean`."))?
        .trim_end()
        .to_string();
    let lean_name = match json.get("lean_name") {
        Some(Value::Null) => None,
        Some(Value::String(name)) => (!name.trim().is_empty()).then(|| name.trim().to_string()),
        _ => return Err(block_failure("The formalizer's JSON has no `lean_name`.")),
    };
    let assumptions = json
        .get("assumptions")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().map(Value::as_str).collect::<Option<Vec<_>>>())
        .ok_or_else(|| block_failure("The formalizer's JSON has no `assumptions`."))?
        .into_iter()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Formal {
        lean,
        lean_name,
        assumptions,
        uses: Vec::new(),
    })
}

/// The pieces in the model's reply to a split of `text`, as the parts of `text` they stand for.
/// Rejects a reply whose pieces are not the whole paragraph in order (see `parts`).
pub(crate) fn parse_split(response: &Value, text: &str) -> Result<Vec<String>, Failure> {
    let json = reply_object(response, "split")?;
    let pieces = json
        .get("pieces")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .map(|piece| piece.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| block_failure("The formalizer's JSON has no `pieces`."))?;
    parts(text, &pieces)
        .ok_or_else(|| block_failure("The paragraph could not be split into its items."))
}

/// The part of `text` that each of `pieces` stands for, when the pieces are `text` cut in order
/// with nothing left out, added or reworded. Whitespace is ignored where comparing, so a piece may
/// differ from its part in spacing at the cuts and within it. Each part runs from its first to its
/// last character in `text`. None when the pieces do not reproduce `text`, or one is blank.
pub(crate) fn parts(text: &str, pieces: &[String]) -> Option<Vec<String>> {
    let mut rest = text.char_indices().filter(|(_, c)| !c.is_whitespace());
    let mut parts = Vec::new();
    for piece in pieces {
        let mut span: Option<(usize, usize)> = None;
        for c in piece.chars().filter(|c| !c.is_whitespace()) {
            let (at, d) = rest.next()?;
            if c != d {
                return None;
            }
            span = Some((span.map_or(at, |(start, _)| start), at + d.len_utf8()));
        }
        let (start, end) = span?;
        parts.push(text[start..end].to_string());
    }
    rest.next().is_none().then_some(parts)
}

#[cfg(test)]
#[path = "../tests/unit/openrouter.rs"]
mod tests;
