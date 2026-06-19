use serde_json::json;
use session_weaver::core::{
    ClientKind, Conversation, ConversationMeta, Entry, Message, ToolResult, validate_conversation,
};

#[test]
fn validation_reports_empty_model_and_orphan_tool_result() {
    let conversation = Conversation {
        id: "x".into(),
        source: ClientKind::Claude,
        metadata: ConversationMeta::default(),
        entries: vec![
            Entry::Assistant(Message {
                id: "a".into(),
                parts: vec![],
                timestamp: None,
                model: Some(String::new()),
            }),
            Entry::ToolResult(ToolResult {
                id: "r".into(),
                call_id: "missing".into(),
                output: json!("x"),
                is_error: false,
                timestamp: None,
            }),
        ],
    };

    let findings = validate_conversation(&conversation);
    assert!(
        findings
            .iter()
            .any(|item| item.code == "assistant_model_empty")
    );
    assert!(
        findings
            .iter()
            .any(|item| item.code == "orphan_tool_result")
    );
}

#[test]
fn validation_allows_multiple_entries_without_native_ids() {
    let conversation = Conversation {
        id: "x".into(),
        source: ClientKind::Codex,
        metadata: ConversationMeta::default(),
        entries: vec![
            Entry::User(Message::text("", "first")),
            Entry::User(Message::text("", "second")),
        ],
    };
    let findings = validate_conversation(&conversation);
    assert!(
        !findings
            .iter()
            .any(|item| item.code == "duplicate_entry_id")
    );
}
