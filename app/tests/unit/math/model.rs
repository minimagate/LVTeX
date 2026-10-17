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
