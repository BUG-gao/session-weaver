use serde_json::json;
use session_weaver::connectors::claude;

#[test]
fn rejects_text_block_without_text() {
    let values = vec![json!({
        "type": "assistant",
        "message": {
            "model": "claude-test",
            "stop_reason": "end_turn",
            "content": [{"type": "text", "image_url": "data:image/png;base64,aA=="}]
        }
    })];
    let findings = claude::validate_native(&values);
    assert!(
        findings
            .iter()
            .any(|item| item.code == "claude_text_missing")
    );
}

#[test]
fn rejects_missing_model_and_null_stop_reason() {
    let values = vec![json!({
        "type": "assistant",
        "message": {
            "stop_reason": null,
            "content": [{"type": "text", "text": "answer"}]
        }
    })];
    let findings = claude::validate_native(&values);
    assert!(
        findings
            .iter()
            .any(|item| item.code == "claude_model_missing")
    );
    assert!(
        findings
            .iter()
            .any(|item| item.code == "claude_stop_reason_invalid")
    );
}
