use std::fs;

use serde_json::json;
use session_weaver::connectors::claude;
use session_weaver::connectors::codex;
use session_weaver::core::{ClientKind, Conversation, ConversationMeta, Entry, Message};
use session_weaver::pipeline::{MoveOptions, export_package, import_package, move_session};
use tempfile::tempdir;

fn conversation() -> Conversation {
    Conversation {
        id: "00000000-0000-4000-8000-000000000009".into(),
        source: ClientKind::Claude,
        metadata: ConversationMeta {
            cwd: Some("/tmp/project".into()),
            model: Some("claude-test".into()),
            extras: json!({}),
            ..Default::default()
        },
        entries: vec![
            Entry::User(Message::text("u", "hello")),
            Entry::Assistant(Message {
                id: "a".into(),
                parts: vec![session_weaver::core::Part::Text("world".into())],
                timestamp: None,
                model: Some("claude-test".into()),
            }),
        ],
    }
}

#[test]
fn package_roundtrip_verifies_digest() {
    let package = export_package(&conversation()).unwrap();
    let restored = import_package(&package).unwrap();
    assert!(conversation().semantically_matches(&restored).is_ok());

    let mut corrupted = package;
    corrupted.digest = "bad".into();
    assert!(import_package(&corrupted).is_err());
}

#[test]
fn move_session_writes_parseable_target() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.jsonl");
    let source_lines = claude::render(&conversation(), "claude-test").unwrap();
    fs::write(
        &source,
        source_lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();

    let target_root = directory.path().join("codex-home");
    let report = move_session(MoveOptions {
        source: &source,
        source_kind: ClientKind::Claude,
        target_kind: ClientKind::Codex,
        target_root: &target_root,
        overwrite: false,
        claude_model: "claude-test",
    })
    .unwrap();

    assert!(report.output.exists());
    assert!(
        report
            .output
            .components()
            .any(|component| component.as_os_str() == "sessions")
    );
    assert!(target_root.join("session_index.jsonl").exists());
}

#[test]
fn move_session_updates_claude_history() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("source.jsonl");
    let source_lines = codex::render(&conversation()).unwrap();
    fs::write(
        &source,
        source_lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let target_root = directory.path().join("claude-home");
    move_session(MoveOptions {
        source: &source,
        source_kind: ClientKind::Codex,
        target_kind: ClientKind::Claude,
        target_root: &target_root,
        overwrite: false,
        claude_model: "claude-test",
    })
    .unwrap();
    assert!(target_root.join("history.jsonl").exists());
}
