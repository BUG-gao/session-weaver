use chrono::Utc;
use serde_json::json;
use session_weaver::core::{
    ClientKind, Conversation, ConversationMeta, Entry, ImageData, Message, Part, Thought, ToolCall,
    ToolResult,
};

#[test]
fn model_roundtrip_preserves_supported_content() {
    let conversation = Conversation {
        id: "source-id".into(),
        source: ClientKind::Codex,
        metadata: ConversationMeta {
            cwd: Some("/tmp/project".into()),
            git_branch: Some("main".into()),
            title: Some("Example".into()),
            created_at: Some(Utc::now()),
            updated_at: None,
            model: Some("gpt-example".into()),
            provider: Some("openai".into()),
            extras: json!({"custom": true}),
        },
        entries: vec![
            Entry::User(Message::text("u1", "hello")),
            Entry::Thought(Thought {
                id: "r1".into(),
                summary: Some("summary".into()),
                content: "private reasoning".into(),
                timestamp: None,
            }),
            Entry::Assistant(Message {
                id: "a1".into(),
                parts: vec![
                    Part::Text("answer".into()),
                    Part::Image(ImageData {
                        media_type: "image/png".into(),
                        data: "aGVsbG8=".into(),
                        source_url: None,
                    }),
                ],
                timestamp: None,
                model: Some("gpt-example".into()),
            }),
            Entry::ToolCall(ToolCall {
                id: "t1".into(),
                name: "shell".into(),
                arguments: json!({"cmd": "pwd"}),
                timestamp: None,
            }),
            Entry::ToolResult(ToolResult {
                id: "tr1".into(),
                call_id: "t1".into(),
                output: json!("ok"),
                is_error: false,
                timestamp: None,
            }),
        ],
    };

    let encoded = serde_json::to_string(&conversation).unwrap();
    let decoded: Conversation = serde_json::from_str(&encoded).unwrap();
    assert!(conversation.semantically_matches(&decoded).is_ok());
}
