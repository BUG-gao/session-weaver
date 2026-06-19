use std::fs;

use proptest::prelude::*;
use serde_json::json;
use session_weaver::connectors::{claude, codex};
use session_weaver::core::{
    ClientKind, Conversation, ConversationMeta, Entry, Message, Part, ToolCall, ToolResult,
};
use tempfile::tempdir;

fn write_values(path: &std::path::Path, values: &[serde_json::Value]) {
    fs::write(
        path,
        values
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
}

fn sample(text: String) -> Conversation {
    Conversation {
        id: "00000000-0000-4000-8000-000000000001".into(),
        source: ClientKind::Claude,
        metadata: ConversationMeta {
            cwd: Some("/tmp/demo".into()),
            model: Some("claude-test".into()),
            ..Default::default()
        },
        entries: vec![
            Entry::User(Message::text("u", text)),
            Entry::Assistant(Message {
                id: "a".into(),
                parts: vec![Part::Text("answer".into())],
                timestamp: None,
                model: Some("claude-test".into()),
            }),
            Entry::ToolCall(ToolCall {
                id: "call".into(),
                name: "shell".into(),
                arguments: json!({"cmd": "pwd"}),
                timestamp: None,
            }),
            Entry::ToolResult(ToolResult {
                id: "result".into(),
                call_id: "call".into(),
                output: json!("ok"),
                is_error: false,
                timestamp: None,
            }),
        ],
    }
}

#[test]
fn both_native_formats_roundtrip_semantically() {
    let directory = tempdir().unwrap();
    let original = sample("hello".into());

    let claude_path = directory.path().join("claude.jsonl");
    write_values(
        &claude_path,
        &claude::render(&original, "claude-test").unwrap(),
    );
    let from_claude = claude::parse_file(&claude_path).unwrap();
    assert!(
        original.semantically_matches(&from_claude).is_ok(),
        "{:?}",
        original.semantically_matches(&from_claude)
    );

    let codex_path = directory.path().join("codex.jsonl");
    write_values(&codex_path, &codex::render(&original).unwrap());
    let from_codex = codex::parse_file(&codex_path).unwrap();
    assert!(
        original.semantically_matches(&from_codex).is_ok(),
        "{:?}",
        original.semantically_matches(&from_codex)
    );
}

proptest! {
    #[test]
    fn text_survives_both_renderers(text in ".{1,200}") {
        let directory = tempdir().unwrap();
        let original = sample(text);
        let path = directory.path().join("session.jsonl");
        write_values(&path, &codex::render(&original).unwrap());
        let parsed = codex::parse_file(&path).unwrap();
        prop_assert_eq!(
            original.entries.first().and_then(|entry| match entry {
                Entry::User(message) => Some(message.plain_text()),
                _ => None,
            }),
            parsed.entries.first().and_then(|entry| match entry {
                Entry::User(message) => Some(message.plain_text()),
                _ => None,
            })
        );
    }
}
