use std::fs;

use rusqlite::Connection;
use session_weaver::catalog::{discover_sessions, register_codex_thread};
use session_weaver::core::{ClientKind, Conversation, ConversationMeta};
use tempfile::tempdir;

#[test]
fn discovers_nested_native_sessions() {
    let directory = tempdir().unwrap();
    let nested = directory.path().join("sessions/2026/06/19");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("rollout-demo-abc.jsonl"),
        r#"{"type":"session_meta","payload":{"id":"abc"}}"#,
    )
    .unwrap();
    let sessions = discover_sessions(ClientKind::Codex, directory.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "abc");
}

#[test]
fn ignores_claude_subagent_logs_when_discovering_sessions() {
    let directory = tempdir().unwrap();
    let project = directory.path().join("projects/project-a");
    let session_id = "51420484-0e21-4ae8-ae6a-983ae2afd51d";
    fs::create_dir_all(project.join(session_id).join("subagents")).unwrap();
    fs::write(
        project.join(format!("{session_id}.jsonl")),
        format!(r#"{{"type":"user","sessionId":"{session_id}"}}"#),
    )
    .unwrap();
    fs::write(
        project
            .join(session_id)
            .join("subagents")
            .join("agent-a.jsonl"),
        format!(r#"{{"type":"user","sessionId":"{session_id}","isSidechain":true}}"#),
    )
    .unwrap();

    let sessions = discover_sessions(ClientKind::Claude, directory.path()).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session_id);
    assert_eq!(
        sessions[0].path,
        project.join(format!("{session_id}.jsonl"))
    );
}

#[test]
fn registers_thread_using_available_columns() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("state_5.sqlite");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL
            );",
        )
        .unwrap();
    drop(connection);

    let conversation = Conversation {
        id: "id-1".into(),
        source: ClientKind::Claude,
        metadata: ConversationMeta {
            cwd: Some("/tmp/project".into()),
            title: Some("Demo".into()),
            ..Default::default()
        },
        entries: vec![],
    };
    register_codex_thread(
        directory.path(),
        &conversation,
        &directory.path().join("x.jsonl"),
    )
    .unwrap();

    let connection = Connection::open(database).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}
