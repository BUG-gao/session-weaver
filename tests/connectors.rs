use std::fs;

use serde_json::{Value, json};
use session_weaver::connectors::{claude, codex};
use session_weaver::core::{
    ClientKind, Conversation, ConversationMeta, Entry, ImageData, Message, Part, ToolCall,
    ToolResult,
};
use tempfile::tempdir;

fn sample() -> Conversation {
    Conversation {
        id: "00000000-0000-4000-8000-000000000001".into(),
        source: ClientKind::Codex,
        metadata: ConversationMeta {
            cwd: Some("/tmp/demo".into()),
            git_branch: Some("main".into()),
            model: Some("claude-sonnet-4-20250514".into()),
            ..Default::default()
        },
        entries: vec![
            Entry::User(Message::text("u1", "hello")),
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
                model: Some("claude-sonnet-4-20250514".into()),
            }),
            Entry::ToolCall(ToolCall {
                id: "call-1".into(),
                name: "shell".into(),
                arguments: json!({"cmd": "pwd"}),
                timestamp: None,
            }),
            Entry::ToolResult(ToolResult {
                id: "result-1".into(),
                call_id: "call-1".into(),
                output: json!("ok"),
                is_error: false,
                timestamp: None,
            }),
        ],
    }
}

#[test]
fn claude_renderer_emits_resume_safe_fields() {
    let values = claude::render(&sample(), "fallback-model").unwrap();
    let assistants: Vec<&Value> = values
        .iter()
        .filter(|value| value["type"] == "assistant")
        .collect();

    assert!(assistants.iter().all(|value| {
        value["message"]["model"]
            .as_str()
            .is_some_and(|model| !model.is_empty())
    }));
    assert!(assistants.iter().any(|value| {
        value["message"]["stop_reason"] == "end_turn"
            && value["message"]["content"]
                .as_array()
                .unwrap()
                .iter()
                .any(|block| block["type"] == "image" && block["source"]["type"] == "base64")
    }));
    assert!(
        assistants
            .iter()
            .any(|value| value["message"]["stop_reason"] == "tool_use")
    );
    assert!(claude::validate_native(&values).is_empty());
}

#[test]
fn claude_renderer_coerces_foreign_model_ids() {
    // A Codex source carries a non-Claude model id; the Claude desktop app
    // hides sessions whose `model` it cannot resolve, so every assistant
    // record must fall back to the requested Claude model instead.
    let mut conversation = sample();
    conversation.metadata.model = Some("gpt-5.5".into());
    if let Some(Entry::Assistant(message)) = conversation
        .entries
        .iter_mut()
        .find(|entry| matches!(entry, Entry::Assistant(_)))
    {
        message.model = Some("gpt-5.5".into());
    }

    let values = claude::render(&conversation, "claude-opus-4-8").unwrap();
    assert!(values.iter().filter(|v| v["type"] == "assistant").all(|v| {
        v["message"]["model"]
            .as_str()
            .is_some_and(|model| model.starts_with("claude"))
    }));
}

#[test]
fn claude_renderer_preserves_native_claude_model() {
    // claude->claude migrations must keep the original Claude model verbatim.
    let values = claude::render(&sample(), "fallback-model").unwrap();
    assert!(values.iter().filter(|v| v["type"] == "assistant").all(|v| {
        v["message"]["model"] == "claude-sonnet-4-20250514"
    }));
}

#[test]
fn claude_parser_reads_messages_and_tools() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("claude.jsonl");
    let lines = claude::render(&sample(), "fallback-model").unwrap();
    fs::write(
        &path,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let parsed = claude::parse_file(&path).unwrap();
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::ToolCall(_)))
    );
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::ToolResult(_)))
    );
}

#[test]
fn codex_parser_and_renderer_preserve_core_events() {
    let lines = codex::render(&sample()).unwrap();
    assert_eq!(lines[0]["type"], "session_meta");
    assert!(lines.iter().any(|value| {
        value["type"] == "response_item" && value["payload"]["type"] == "function_call"
    }));

    let directory = tempdir().unwrap();
    let path = directory.path().join("codex.jsonl");
    fs::write(
        &path,
        lines
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let parsed = codex::parse_file(&path).unwrap();
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::User(_)))
    );
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::Assistant(_)))
    );
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::ToolCall(_)))
    );
    assert!(
        parsed
            .entries
            .iter()
            .any(|item| matches!(item, Entry::ToolResult(_)))
    );
}

#[test]
fn developer_messages_are_preserved_and_projected_for_claude() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("codex.jsonl");
    fs::write(
        &path,
        [
            json!({"timestamp":"2026-06-19T00:00:00Z","type":"session_meta","payload":{"id":"00000000-0000-7000-8000-000000000001","cwd":"."}}),
            json!({"timestamp":"2026-06-19T00:00:01Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"Follow repository rules."}]}}),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n"),
    )
    .unwrap();
    let parsed = codex::parse_file(&path).unwrap();
    assert!(matches!(parsed.entries.first(), Some(Entry::Developer(_))));
    let claude_lines = claude::render(&parsed, "claude-test").unwrap();
    assert!(claude_lines.iter().any(|line| {
        line["type"] == "user"
            && line["message"]["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("developer"))
    }));
}
