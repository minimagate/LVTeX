use super::*;
use crate::{Failure, Formal, HAIKU_MODEL, JEV_MODEL, Label, Tag};
use serde_json::{Value, json};

const NOT_SET: &str =
    "OPENROUTER_API_KEY is not set. Export it and launch Mathstral from that terminal.";
const MALFORMED: &str = "OPENROUTER_API_KEY is malformed.";
const NO_LABEL: &str = "Jev gave no label.";
const NO_JOINS: &str = "Jev did not say whether the paragraph continues the one above.";
const NO_CONTENT: &str = "Haiku sent no content.";
const CUT_OFF: &str = "The formalization was cut off.";
const INVALID_JSON: &str = "Haiku did not return valid JSON.";
const NO_LEAN: &str = "Haiku's JSON has no `lean`.";
const NO_LEAN_NAME: &str = "Haiku's JSON has no `lean_name`.";
const NO_ASSUMPTIONS: &str = "Haiku's JSON has no `assumptions`.";

fn service(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        service: true,
    }
}

fn block(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        service: false,
    }
}

fn formal(lean: &str, lean_name: Option<&str>, assumptions: &[&str]) -> Formal {
    Formal {
        lean: lean.to_string(),
        lean_name: lean_name.map(str::to_string),
        assumptions: assumptions.iter().map(|text| text.to_string()).collect(),
    }
}

/// A chat completion whose first choice has `content` and finished normally.
fn reply(content: Value) -> Value {
    json!({"choices": [{"finish_reason": "stop",
        "message": {"role": "assistant", "content": content}}]})
}

fn check_formal(cases: Vec<(Value, Result<Formal, Failure>)>) {
    for (value, expected) in cases {
        assert_eq!(parse_formal(&value), expected, "{value}");
    }
}

#[test]
fn checked_key_rejects_missing_and_malformed_keys() {
    let cases: Vec<(Option<&str>, Result<&str, Failure>)> = vec![
        (None, Err(service(NOT_SET))),
        (Some(""), Err(service(NOT_SET))),
        (Some(" \t"), Err(service(NOT_SET))),
        (Some("sk\"or"), Err(service(MALFORMED))),
        (Some("sk\\or"), Err(service(MALFORMED))),
        (Some("sk or"), Err(service(MALFORMED))),
        (Some(" sk-or"), Err(service(MALFORMED))),
        (Some("sk-or\n"), Err(service(MALFORMED))),
        (Some("sk-or-v1-abc"), Ok("sk-or-v1-abc")),
    ];
    for (key, expected) in cases {
        assert_eq!(checked_key(key), expected, "{key:?}");
    }
}

#[test]
fn escape_quotes_a_value_for_a_curl_config_string() {
    let cases = [
        ("plain", "plain"),
        ("back\\slash", "back\\\\slash"),
        ("say \"hi\"", "say \\\"hi\\\""),
        ("line\nbreak\rand\ttab", "line\\nbreak\\rand\\ttab"),
        ("é ∀", "é ∀"),
    ];
    for (input, expected) in cases {
        assert_eq!(escape(input), expected, "{input:?}");
    }
}

#[test]
fn curl_config_keeps_each_setting_on_one_line_and_the_key_in_its_header() {
    let body = "{\"q\": \"say \\\"hi\\\"\\\\ now\"}\nsecond";
    let config = curl_config("https://example.test/api", "sk-or-test", body);
    let expected = concat!(
        "url = \"https://example.test/api\"\n",
        "header = \"Authorization: Bearer sk-or-test\"\n",
        "header = \"Content-Type: application/json\"\n",
        "header = \"X-Title: Mathstral\"\n",
        r#"data-binary = "{\"q\": \"say \\\"hi\\\"\\\\ now\"}\nsecond""#,
        "\n",
    );
    assert_eq!(config, expected);
    assert_eq!(config.matches("sk-or-test").count(), 1);
}

#[test]
fn response_accepts_only_a_200_with_json() {
    let cases: Vec<(&str, Result<Value, Failure>)> = vec![
        ("{\"choices\":[]}\n200", Ok(json!({"choices": []}))),
        // The status is on the last line; the body may span lines.
        ("{\n\"a\": 1\n}\n200", Ok(json!({"a": 1}))),
        (
            "{\"error\":{\"message\":\"No auth credentials found\"}}\n401",
            Err(service(
                "OpenRouter answered 401: No auth credentials found",
            )),
        ),
        (
            "upstream <b>broken</b>\n500",
            Err(service("OpenRouter answered 500: upstream <b>broken</b>")),
        ),
        (
            "{\"error\":\"bad\"}\n400",
            Err(service("OpenRouter answered 400: {\"error\":\"bad\"}")),
        ),
        (
            "not json\n200",
            Err(service("OpenRouter sent invalid JSON.")),
        ),
    ];
    for (stdout, expected) in cases {
        assert_eq!(response(stdout), expected, "{stdout:?}");
    }
}

#[test]
fn response_truncates_a_long_error_body_on_a_character_boundary() {
    let stdout = format!("{}\n503", "é".repeat(300));
    assert_eq!(
        response(&stdout),
        Err(service(&format!(
            "OpenRouter answered 503: {}",
            "é".repeat(200)
        )))
    );
}

#[test]
fn classify_request_names_the_model_and_echoes_the_state() {
    for (previous, text, proof_open) in [
        (None, "Let $G$ be a group.", false),
        (Some("Let $G$ be a group."), "Hence $x = 0$.", true),
    ] {
        let request = classify_request(previous, text, proof_open);
        assert_eq!(request["model"], JEV_MODEL);
        assert_eq!(request["state"]["previous_paragraph"], json!(previous));
        assert_eq!(request["state"]["paragraph"], text);
        assert_eq!(
            request["state"]["proof_in_progress"], proof_open,
            "{text:?}"
        );
    }
}

#[test]
fn classify_request_offers_exactly_the_label_names_as_choices() {
    let request = classify_request(None, "x", false);
    assert_eq!(request["questions"]["kind"]["type"], "choice");
    let criteria = request["questions"]["kind"]["criteria"]
        .as_object()
        .expect("criteria object");
    let mut keys: Vec<&str> = criteria.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut names: Vec<&str> = Label::ALL.iter().map(|label| label.name()).collect();
    names.sort_unstable();
    assert_eq!(keys, names);
}

#[test]
fn classify_request_asks_whether_the_paragraph_continues_the_one_above() {
    let request = classify_request(Some("Let $x$ be real."), "Then $x = x$.", false);
    assert_eq!(request["questions"]["joins"]["type"], "choice");
    let criteria = request["questions"]["joins"]["criteria"]
        .as_object()
        .expect("criteria object");
    let mut keys: Vec<&str> = criteria.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["continues", "new"]);
}

#[test]
fn parse_tag_reads_each_label_and_whether_it_joins() {
    for label in Label::ALL {
        for (choice, joins) in [("continues", true), ("new", false)] {
            let response = json!({"answers": {
                "kind": {"choice": label.name()},
                "joins": {"choice": choice},
            }});
            assert_eq!(
                parse_tag(&response),
                Ok(Tag { label, joins }),
                "{} {choice}",
                label.name()
            );
        }
    }
}

#[test]
fn parse_tag_rejects_an_unknown_or_missing_label() {
    let cases = [
        json!({"answers": {"kind": {"choice": "theorem"}, "joins": {"choice": "new"}}}),
        json!({"answers": {"kind": {"choice": "Proof"}, "joins": {"choice": "new"}}}),
        json!({"answers": {"kind": {"choice": null}, "joins": {"choice": "new"}}}),
        json!({"answers": {"kind": {}, "joins": {"choice": "continues"}}}),
        json!({"answers": {"joins": {"choice": "continues"}}}),
    ];
    for response in cases {
        assert_eq!(parse_tag(&response), Err(service(NO_LABEL)), "{response}");
    }
}

#[test]
fn parse_tag_rejects_a_missing_or_unknown_answer_about_joining() {
    let cases = [
        json!({"answers": {"kind": {"choice": "skip"}}}),
        json!({"answers": {"kind": {"choice": "skip"}, "joins": {"choice": null}}}),
        json!({"answers": {"kind": {"choice": "skip"}, "joins": {"choice": "maybe"}}}),
    ];
    for response in cases {
        assert_eq!(parse_tag(&response), Err(service(NO_JOINS)), "{response}");
    }
}

#[test]
fn formalize_request_caches_the_system_prompt_and_the_last_lean_segment() {
    let above = [
        (1, "def a := 1".to_string()),
        (2, "def b := 2".to_string()),
        (3, "def c := 3".to_string()),
    ];
    let request = formalize_request(Label::Definition, "Let $c$ be $a$.", None, &above, None);
    assert_eq!(request["messages"][0]["role"], "system");
    let system = &request["messages"][0]["content"][0];
    assert_eq!(system["text"], SYSTEM);
    assert_eq!(system["cache_control"], json!({"type": "ephemeral"}));

    let parts = request["messages"][1]["content"]
        .as_array()
        .expect("user content parts");
    assert_eq!(parts.len(), 4);
    for (index, part) in parts.iter().enumerate().take(2) {
        assert!(
            part.get("cache_control").is_none(),
            "part {index} has no cache_control"
        );
    }
    assert_eq!(parts[2]["cache_control"], json!({"type": "ephemeral"}));
    assert_eq!(parts[2]["text"], "-- mathstral:block 3\ndef c := 3\n");
    assert!(parts[3].get("cache_control").is_none());
}

#[test]
fn formalize_request_sends_one_part_when_nothing_is_above() {
    let request = formalize_request(Label::Skip, "Thanks.", None, &[], None);
    let parts = request["messages"][1]["content"]
        .as_array()
        .expect("user content parts");
    assert_eq!(parts.len(), 1);
    assert!(parts[0].get("cache_control").is_none());
}

#[test]
fn formalize_request_states_the_prose_label_statement_and_block() {
    let statement = "theorem t : True := by sorry";
    let request = formalize_request(
        Label::Proof,
        "by trivial",
        Some("Let $G$ be a group."),
        &[],
        Some(statement),
    );
    let parts = request["messages"][1]["content"]
        .as_array()
        .expect("user content parts");
    assert_eq!(
        parts.last().expect("final part")["text"],
        "Prose just above:\nLet $G$ be a group.\n\nLabel: proof\n\nMost recent statement:\ntheorem t : True := by sorry\n\nBlock:\nby trivial"
    );

    let request = formalize_request(Label::Skip, "Thanks.", None, &[], None);
    let parts = request["messages"][1]["content"]
        .as_array()
        .expect("user content parts");
    assert_eq!(
        parts.last().expect("final part")["text"],
        "Prose just above:\n(none)\n\nLabel: skip\n\nBlock:\nThanks."
    );
}

#[test]
fn formalize_request_uses_the_model_deterministically_with_a_strict_schema() {
    let request = formalize_request(Label::Statement, "x", None, &[], None);
    assert_eq!(request["model"], HAIKU_MODEL);
    assert_eq!(request["temperature"], 0);
    let format = &request["response_format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["json_schema"]["name"], "formalization");
    assert_eq!(format["json_schema"]["strict"], true);
    assert_eq!(
        format["json_schema"]["schema"]["required"],
        json!(["lean", "lean_name", "assumptions"])
    );
    assert_eq!(
        format["json_schema"]["schema"]["additionalProperties"],
        false
    );
}

#[test]
fn parse_formal_reads_string_and_part_content() {
    let text =
        r#"{"lean": "def f := 1", "lean_name": "f", "assumptions": ["f is a natural number"]}"#;
    let parts = json!([
        {"type": "text", "text": r#"{"lean": "def g"#},
        {"type": "text", "text": r#" := 2", "lean_name": null, "assumptions": []}"#},
    ]);
    check_formal(vec![
        (
            reply(json!(text)),
            Ok(formal("def f := 1", Some("f"), &["f is a natural number"])),
        ),
        (reply(parts), Ok(formal("def g := 2", None, &[]))),
    ]);
}

#[test]
fn parse_formal_strips_a_code_fence() {
    let body = r#"{"lean": "def h := 3", "lean_name": "h", "assumptions": []}"#;
    check_formal(vec![
        (
            reply(json!(format!("```json\n{body}\n```"))),
            Ok(formal("def h := 3", Some("h"), &[])),
        ),
        (
            reply(json!(format!("```\n{body}\n```\n"))),
            Ok(formal("def h := 3", Some("h"), &[])),
        ),
    ]);
}

#[test]
fn parse_formal_normalizes_the_name_the_lean_and_the_assumptions() {
    check_formal(vec![
        (
            reply(json!(
                r#"{"lean": "  def f := 1 \n\n", "lean_name": "  f ", "assumptions": ["  x is real  ", "", "   "]}"#
            )),
            Ok(formal("  def f := 1", Some("f"), &["x is real"])),
        ),
        (
            reply(json!(
                r#"{"lean": "def f := 1", "lean_name": null, "assumptions": [], "notes": "ignored"}"#
            )),
            Ok(formal("def f := 1", None, &[])),
        ),
        (
            reply(json!(
                r#"{"lean": "def f := 1", "lean_name": "  ", "assumptions": []}"#
            )),
            Ok(formal("def f := 1", None, &[])),
        ),
    ]);
}

#[test]
fn parse_formal_rejects_a_reply_with_a_bad_field() {
    check_formal(vec![
        (
            reply(json!(r#"{"lean_name": null, "assumptions": []}"#)),
            Err(block(NO_LEAN)),
        ),
        (
            reply(json!(
                r#"{"lean": " \n\t", "lean_name": null, "assumptions": []}"#
            )),
            Err(block(NO_LEAN)),
        ),
        (
            reply(json!(
                r#"{"lean": 3, "lean_name": null, "assumptions": []}"#
            )),
            Err(block(NO_LEAN)),
        ),
        (
            reply(json!(r#"{"lean": "def f := 1", "assumptions": []}"#)),
            Err(block(NO_LEAN_NAME)),
        ),
        (
            reply(json!(
                r#"{"lean": "def f := 1", "lean_name": 3, "assumptions": []}"#
            )),
            Err(block(NO_LEAN_NAME)),
        ),
        (
            reply(json!(
                r#"{"lean": "def f := 1", "lean_name": null, "assumptions": "x"}"#
            )),
            Err(block(NO_ASSUMPTIONS)),
        ),
        (
            reply(json!(
                r#"{"lean": "def f := 1", "lean_name": null, "assumptions": ["x", 3]}"#
            )),
            Err(block(NO_ASSUMPTIONS)),
        ),
        (reply(json!("[1, 2]")), Err(block(INVALID_JSON))),
        (reply(json!("not json")), Err(block(INVALID_JSON))),
    ]);
}

#[test]
fn parse_formal_rejects_truncated_and_empty_replies() {
    let truncated = json!({"choices": [{"finish_reason": "length",
        "message": {"role": "assistant", "content": "{\"lean\": \"def f"}}]});
    assert_eq!(parse_formal(&truncated), Err(block(CUT_OFF)));

    let cases = [
        json!({}),
        json!({"choices": []}),
        reply(Value::Null),
        reply(json!(3)),
    ];
    for value in cases {
        assert_eq!(parse_formal(&value), Err(service(NO_CONTENT)), "{value}");
    }
}
