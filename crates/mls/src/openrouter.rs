//! OpenRouter: the Jev decision, the split of a paragraph and the formalization, sent through curl.

use crate::{FORMALIZER_MODEL, Failure, Formal, JEV_MODEL, Label, Segment, SplitContext, Tag};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    io::Write,
    process::{Command, Stdio},
};

pub(crate) const DECISIONS_URL: &str = "https://openrouter.ai/api/alpha/decisions";
pub(crate) const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
/// Bounds each request, so a stalled call cannot hold the worker indefinitely. It covers a reply
/// that may run to `MAX_TOKENS` tokens.
const TIMEOUT_SECONDS: u32 = 300;
/// The most tokens one chat request may produce, reasoning included.
const MAX_TOKENS: u32 = 32_000;
/// The most of those tokens the model may spend reasoning, so at least the rest is left for the
/// answer: a reasoning model that spends the whole budget thinking returns nothing.
const REASONING_TOKENS: u32 = 16_000;

/// Neutral about `proof_in_progress` on purpose: a proof often ends without ∎, and an instruction
/// that favours proof while one is open makes every later claim a proof.
const CLASSIFY: &str = "Classify one paragraph of a mathematical document, or one part of a paragraph. block_above is the block of one or more paragraphs just above it. proof_in_progress is true when a proof above has not ended with ∎; that proof may already be complete. The paragraph is a proof only if it argues for the statement being proved. A new claim is a statement even right after a proof, for example one that starts with \"It follows that\" or \"One might guess that\". A remark about the mathematics is skip.";

const JOINS: &str = "Decide whether this paragraph belongs to the same mathematical item (a definition, a statement or a proof) as block_above, so that they must be formalized together. Sharing a topic is not the same item: prose that motivates, previews or comments on a result is a separate item from that result, even when the result follows right after it. A named new lemma, theorem, corollary, definition or proof begins a new item, even directly after prose. Continue only when this paragraph completes block_above: a setup that is still incomplete (\"Let x be ...\" or hypotheses) followed by the claim it sets up, a standalone displayed formula or clause of the statement, or a further paragraph of a proof that is still going. A standing convention or hypothesis for the rest of the section or document (\"Throughout\", \"In this section\", \"From now on\", \"Standing assumptions\") is prose of its own and never the setup of the item after it.";

const ITEMS: &str = "Decide whether paragraph contains one mathematical item or several independently checkable mathematical items. Judge only paragraph, not block_above. A statement and its supporting proof are separate items, even when they share a sentence. Intermediate claims and computations within a proof belong to that proof. Separate statements are separate items even when they have the same role. Setup, hypotheses, defining conditions and clauses of one statement belong to that item. Use block_above only to interpret what paragraph is doing, never count its contents.";

const SPLIT: &str = r#"Identify the independently checkable definitions, statements and proofs in the current paragraph. Return its labeled items in document order, with explicit targets for proofs. Do not formalize into Lean.

The input contains paragraph, the current text, and block_above, either null or the preceding paragraph with its role and text. Use block_above as context only; never include its text among the returned items.

A statement and the supporting proof offered to establish it are separate items, even when they share a sentence. Intermediate claims and computations within an existing proof belong to that proof. Separate statements remain separate items even when they have the same role. Setup, hypotheses, defining conditions and clauses of one statement stay with the item they serve. A definition and a separate asserted property of it are separate items. Preserve the context of earlier definitions when interpreting later statements; a statement need not repeat that context in its text. Prose that is not a mathematical item has role skip; connecting prose serving an item stays with that item. At a cut between an assertion and its supporting argument, put the connective with the argument.

Each item has a unique positive integer id, role (definition, statement, proof or skip), text, proves, and proves_previous. Every text is a verbatim contiguous part of paragraph. Concatenating the item texts in order must reproduce the entire paragraph exactly, including whitespace. Leave nothing out, and add, reorder or reword nothing. Assign whitespace at a cut to one of its adjacent items. Every item must contain a non-whitespace character.

For a proof, proves is the id of an earlier returned statement it establishes, with proves_previous false; alternatively proves is null and proves_previous is true if it establishes block_above and block_above has role statement. Never target the proof itself or a definition. A proof cannot use both target forms. If the text is a proof but no preceding statement target is available, keep its proof role with proves null and proves_previous false. For every non-proof item, proves is null and proves_previous is false.

Return only the JSON object required by the schema."#;

const SYSTEM: &str = r#"You formalize one block of a mathematical document into Lean 4 with Mathlib.

You receive the Lean already accepted for the blocks above, in document order, each under a `-- lvtex:block <id>` comment; the document text above the block, if any, which fixes its notation, conventions and standing hypotheses; the block's label; and the block itself, written in Markdown with LaTeX between dollar signs. A block may span several paragraphs, separated by blank lines: its setup, displayed formulas and clauses belong to it, so formalize them together as one item.

Return only a JSON object with three fields:
- "lean": Lean 4 source for this block alone. It is checked in an environment that already contains `import Mathlib` and the Lean above, so never write `import`, and never repeat a declaration from above. The one exception is a proof block restating its statement, described under proof.
- "lean_name": the name of the main declaration, or null if there is none.
- "assumptions": every assumption of your Lean, in plain language that a mathematician can check against the block: the types chosen (for example "n is a natural number, so n ≥ 0"), each hypothesis, quantifier and domain, and any Lean convention that changes the meaning (for example "division by zero is 0" or "subtraction on ℕ is truncated"). Write them from the Lean, not from the block. One short sentence each. They state only what the Lean assumes: never say whether the claim is true or false, whether a proof exists, or what Lean will accept.

Conventions by label:
- definition: a `def`, `abbrev`, `structure`, `class` or `inductive`; use `noncomputable def` when needed. No `sorry`. If the block only fixes what letters stand for (for example "Throughout, n is a natural number") and introduces no new notion, return an empty "lean", null "lean_name" and no assumptions.
- statement (a lemma, theorem, corollary or proposition): exactly one `theorem` with a fresh, descriptive snake_case name, whose whole proof is `by sorry`, so the source ends with `:= by sorry`.
- proof: the only block that repeats a declaration. Its "lean" is never empty: it restates the statement given under "Most recent statement" in the prompt, with the same name, binders and type, character for character up to `:=`, and replaces `sorry` with a complete proof. It is checked after every block above, where that statement is already declared with `sorry`: never cite the statement itself, but use any result declared above, including results stated after it. Never weaken or change the statement to make it provable. If the argument is wrong or incomplete, still give your most faithful proof attempt and let Lean reject it.

Rules:
- Never use `axiom`, `admit`, `sorry` (except as the whole proof of a statement), `native_decide`, `set_option`, `unsafe`, `#eval` or `#print`.
- Make each declaration self-contained with explicit binders. Do not use `variable`, `section`, `namespace` or `open ... in` blocks that span declarations.
- Reuse definitions and results from the Lean above by their exact name and signature instead of redefining or reproving them: a corollary cites the theorem it follows from. Never cite a name you have not seen above or do not know to exist in Mathlib. Prefer Mathlib's existing notions (Group, Filter.Tendsto, Real, Finset) over new ones.
- Use standard Mathlib names and notation (ℕ, ℤ, ℝ, ∀, ∃, →). Prefer robust tactics such as simp, ring, linarith, nlinarith, omega, positivity, field_simp and gcongr, together with explicit Mathlib lemmas.
- Notation scoped to a namespace, such as `ᵀ` or `⬝ᵥ` from `Matrix`, needs that namespace opened for the declaration: write `open Matrix in` (or the namespace concerned) directly before it.
- Keep the formalization faithful to the block: the same objects, hypotheses and conclusion.
- Carry the standing hypotheses and conventions fixed in the document above (for example "Let $G$ be a finite group" or "Throughout, $A$ is a symmetric matrix") into every block they govern, as explicit binders and hypotheses, unless the block overrides them. The document above is context only: formalize nothing from it except what this block needs.
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
                "No OpenRouter key. Add one in LVTeX → Settings, or export OPENROUTER_API_KEY and launch LVTeX from that terminal.",
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
        "url = \"{}\"\nheader = \"Authorization: Bearer {}\"\nheader = \"Content-Type: application/json\"\nheader = \"X-Title: LVTeX\"\ndata-binary = \"{}\"\n",
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

/// The Jev request that asks for the kind of one paragraph and whether it continues
/// the one above. `previous` is the block above (see
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
            }
        }
    })
}

/// What Jev chose for one item. A missing or unknown label or joining answer is a service failure.
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
    Ok(Tag {
        label,
        joins,
        split: false,
    })
}

/// A standalone gate: its answer is never coupled to kind or joins decisions.
pub(crate) fn items_request(previous: Option<&str>, text: &str, proof_open: bool) -> Value {
    json!({
        "model": JEV_MODEL,
        "state": {"block_above": previous, "paragraph": text, "proof_in_progress": proof_open},
        "questions": {"independent_items": {
            "type": "choice", "instructions": ITEMS,
            "criteria": {
                "one": "One definition, one statement, one proof, or prose without mathematical items.",
                "several": "Two or more items, such as separate statements, or a statement with its proof, or a definition followed by a separate claim."
            }
        }}
    })
}

pub(crate) fn parse_items(response: &Value) -> Result<bool, Failure> {
    match response["answers"]["independent_items"]["choice"].as_str() {
        Some("several") => Ok(true),
        Some("one") => Ok(false),
        _ => Err(service_failure(
            "Jev did not say whether the paragraph holds several items.",
        )),
    }
}

/// The request that cuts one paragraph into its items (see `Job::Split`).
pub(crate) fn split_request(text: &str, context: &SplitContext) -> Value {
    let above = context
        .statement
        .as_ref()
        .map(|text| (Label::Statement, text))
        .or_else(|| {
            context
                .previous
                .as_ref()
                .map(|(label, text)| (*label, text))
        })
        .map(|(label, text)| json!({"role": label.name(), "text": text}));
    json!({
        "model": FORMALIZER_MODEL,
        "temperature": 0,
        "max_tokens": MAX_TOKENS,
        "reasoning": {"max_tokens": REASONING_TOKENS},
        "messages": [
            {"role": "system", "content": SPLIT},
            {"role": "user", "content": json!({"block_above": above, "paragraph": text}).to_string()},
        ],
        "response_format": {"type": "json_schema", "json_schema": {
            "name": "labeled_items",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {"items": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "integer", "minimum": 1},
                        "role": {"type": "string", "enum": ["definition", "statement", "proof", "skip"]},
                        "text": {"type": "string"},
                        "proves": {"anyOf": [{"type": "integer"}, {"type": "null"}]},
                        "proves_previous": {"type": "boolean"}
                    },
                    "required": ["id", "role", "text", "proves", "proves_previous"],
                    "additionalProperties": false
                }}},
                "required": ["items"],
                "additionalProperties": false,
            },
        }}
    })
}

/// Document text above a block longer than this many characters is cut to its start and its end.
const ABOVE_LIMIT: usize = 40_000;
/// The characters of a rejection's reason that a retry sends back, so a long Lean error stays bounded.
const RETRY_REASON_CHARS: usize = 4_000;
/// The characters kept from the start of a cut document: the notation it fixes is set up there.
const ABOVE_HEAD: usize = 8_000;
/// The characters kept from the end of a cut document: the conventions nearest the block.
const ABOVE_TAIL: usize = 32_000;

/// The document text above a block, cut to its first and last characters when it is too long.
fn bounded_above(above: &str) -> Cow<'_, str> {
    if above.chars().count() <= ABOVE_LIMIT {
        return Cow::Borrowed(above);
    }
    let head = above
        .char_indices()
        .nth(ABOVE_HEAD)
        .map_or(above.len(), |(at, _)| at);
    let tail = above
        .char_indices()
        .nth_back(ABOVE_TAIL - 1)
        .map_or(0, |(at, _)| at);
    Cow::Owned(format!("{}\n\n[…]\n\n{}", &above[..head], &above[tail..]))
}

/// The request that formalizes one block. The system prompt and the last Lean
/// segment carry cache_control, so an edit in the middle of the document invalidates
/// only the cached suffix after it. `above` is the document text above the block (see
/// `Job::Formalize`). `retry` is the failed Lean and the reason it was not accepted, for a block
/// that is asked again (see `worker`).
pub(crate) fn formalize_request(
    label: Label,
    text: &str,
    above: Option<&str>,
    lean_above: &[(u64, String)],
    statement: Option<&str>,
    retry: Option<(&str, &str)>,
) -> Value {
    let system = json!({"role": "system", "content": [
        {"type": "text", "text": SYSTEM, "cache_control": {"type": "ephemeral"}}
    ]});
    let mut parts: Vec<Value> = lean_above
        .iter()
        .map(|(id, lean)| json!({"type": "text", "text": format!("-- lvtex:block {id}\n{lean}\n")}))
        .collect();
    if let Some(last) = parts.last_mut() {
        last["cache_control"] = json!({"type": "ephemeral"});
    }
    let statement = statement
        .map(|statement| format!("Most recent statement:\n{statement}\n\n"))
        .unwrap_or_default();
    let retry = retry
        .map(|(failed, reason)| {
            let reason: String = reason.chars().take(RETRY_REASON_CHARS).collect();
            // A reply that was cut off or is not JSON has no Lean to send back, so the retry asks
            // for a short answer instead.
            let (failed, short) = if failed.trim().is_empty() {
                (
                    "(no usable answer)",
                    " Keep the Lean as short as the block allows.",
                )
            } else {
                (failed, "")
            };
            format!(
                "\n\nYour previous formalization of this block was not accepted:\n{failed}\n\nThe reason:\n{reason}\n\nReturn a corrected formalization of the same block. Fix what the reason shows and keep the block's meaning: the same objects, hypotheses and conclusion. For a proof, keep the statement's name, binders and type exactly, and never use sorry.{short}"
            )
        })
        .unwrap_or_default();
    let prompt = format!(
        "Document above:\n{}\n\nLabel: {}\n\n{statement}Block:\n{text}{retry}",
        above.map_or(Cow::Borrowed("(none)"), bounded_above),
        label.name(),
    );
    parts.push(json!({"type": "text", "text": prompt}));
    json!({
        "model": FORMALIZER_MODEL,
        "temperature": 0,
        "max_tokens": MAX_TOKENS,
        "reasoning": {"max_tokens": REASONING_TOKENS},
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
pub(crate) fn parse_split(
    response: &Value,
    text: &str,
    context: &SplitContext,
) -> Result<Vec<Segment>, Failure> {
    let json = reply_object(response, "split")?;
    segments(&json, text, context)
}

/// Shared validation for model replies, worker replies and saved memory. Relations that require
/// rebuilding intervening Lean environments are rejected explicitly rather than rebound.
pub(crate) fn segments(
    json: &Value,
    text: &str,
    context: &SplitContext,
) -> Result<Vec<Segment>, Failure> {
    let invalid = || {
        block_failure(
            "Invalid labeled segmentation: expected unique positive IDs, roles, text and proof targets.",
        )
    };
    let entries = json
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if json.as_object().is_none_or(|object| object.len() != 1) || entries.is_empty() {
        return Err(invalid());
    }
    let mut items: Vec<Segment> = Vec::new();
    for entry in entries {
        if entry.as_object().is_none_or(|object| object.len() != 5) {
            return Err(invalid());
        }
        let id = entry["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or_else(invalid)?;
        let label = entry["role"]
            .as_str()
            .and_then(Label::from_name)
            .ok_or_else(invalid)?;
        let text = entry["text"].as_str().ok_or_else(invalid)?.to_owned();
        let proves = match entry.get("proves") {
            Some(Value::Null) => None,
            Some(value) => Some(value.as_u64().filter(|id| *id > 0).ok_or_else(invalid)?),
            None => return Err(invalid()),
        };
        let proves_previous = entry["proves_previous"].as_bool().ok_or_else(invalid)?;
        if items.iter().any(|item| item.id == id) {
            return Err(invalid());
        }
        if label != Label::Proof && (proves.is_some() || proves_previous) {
            return Err(block_failure(
                "Invalid labeled segmentation: a non-proof has a proof target.",
            ));
        }
        if label == Label::Proof {
            if proves_previous && (proves.is_some() || context.statement.is_none())
                || proves.is_some_and(|target| {
                    !items
                        .iter()
                        .any(|item| item.id == target && item.label == Label::Statement)
                })
            {
                return Err(block_failure(
                    "Invalid labeled segmentation: a proof must target an earlier statement.",
                ));
            }
            let last = items.iter().rev().find(|item| item.label != Label::Skip);
            if proves.is_some_and(|target| last.is_none_or(|item| item.id != target))
                || (proves_previous
                    && (last.is_some()
                        || context.previous.as_ref().is_some_and(|(role, text)| {
                            *role == Label::Proof
                                && (text.trim_end().ends_with('∎') || !items.is_empty())
                        })))
            {
                return Err(block_failure(
                    "Unsupported proof target: the statement must directly precede its proof; move intervening items after the proof.",
                ));
            }
        }
        items.push(Segment {
            id,
            label,
            text,
            proves,
            proves_previous,
        });
    }
    let texts: Vec<String> = items.iter().map(|item| item.text.clone()).collect();
    let covered = parts(text, &texts).ok_or_else(|| {
        block_failure(
            "Invalid labeled segmentation: items must reproduce the whole paragraph in order.",
        )
    })?;
    for (item, text) in items.iter_mut().zip(covered) {
        item.text = text;
    }
    Ok(items)
}

pub(crate) fn segments_json(items: &[Segment]) -> Value {
    json!({"items": items.iter().map(|item| json!({
        "id": item.id, "role": item.label.name(), "text": item.text,
        "proves": item.proves, "proves_previous": item.proves_previous,
    })).collect::<Vec<_>>()})
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
