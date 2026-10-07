use gizai_agents::models::{ModelOption, fetch_models, parse_models};

const FIXTURE: &str = include_str!("fixtures/models-init.jsonl");

#[test]
fn reads_claude_codes_model_list_from_the_initialize_answer() {
    let models = parse_models(FIXTURE.trim(), "gizai-models").expect("a model list");
    let names: Vec<&str> = models.iter().map(|m| m.value.as_str()).collect();
    assert_eq!(names, ["default", "opus", "fable", "sonnet", "haiku"]);
    let opus = &models[1];
    assert_eq!(opus.resolved_model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(opus.display_name, "Opus");
    assert!(opus.description.starts_with("Opus 5.5"));
    assert!(opus.supports_effort);
    assert_eq!(opus.effort_levels, ["low", "medium", "high", "xhigh", "max"]);
    let haiku = &models[4];
    assert!(!haiku.supports_effort && haiku.effort_levels.is_empty());
}

#[test]
fn other_lines_and_other_requests_are_not_a_model_list() {
    assert!(parse_models(FIXTURE.trim(), "other-id").is_none());
    assert!(parse_models(r#"{"type":"system","subtype":"init"}"#, "r1").is_none());
    assert!(parse_models("not json", "r1").is_none());
    assert!(parse_models(r#"{"type":"control_response","response":{"subtype":"error","request_id":"r1","error":"nope"}}"#, "r1").is_none());
}

#[tokio::test]
async fn asks_a_running_claude_code_and_lets_it_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude.sh");
    let models: Vec<ModelOption> = fetch_models(bin.as_ref(), tmp.path()).await.unwrap();
    assert_eq!(models.len(), 5);
    assert_eq!(models[2].value, "fable");
}

#[tokio::test]
async fn a_claude_code_that_gives_no_list_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let e = fetch_models("/bin/false".as_ref(), tmp.path()).await.unwrap_err();
    assert!(e.to_string().contains("model list"), "{e}");
}
