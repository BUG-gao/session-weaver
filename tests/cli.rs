use assert_cmd::Command;
use predicates::prelude::*;
use session_weaver::core::{ClientKind, Conversation, ConversationMeta};
use session_weaver::pipeline::export_package;
use std::fs;
use tempfile::tempdir;

#[test]
fn prints_product_name() {
    Command::cargo_bin("session-weaver")
        .expect("binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Session Weaver"));
}

#[test]
fn short_binary_lists_easy_commands() {
    Command::cargo_bin("sw")
        .expect("short binary should build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("to-claude"))
        .stdout(predicate::str::contains("tc"))
        .stdout(predicate::str::contains("ok"));
}

#[test]
fn tc_moves_codex_session_without_from_to_flags() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("codex.jsonl");
    let target = directory.path().join("claude-home");
    fs::write(
        &source,
        r#"{"timestamp":"2026-06-19T00:00:00.000Z","type":"session_meta","payload":{"id":"codex-short","cwd":"."}}
{"timestamp":"2026-06-19T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","id":"u1","content":[{"type":"input_text","text":"hello"}]}}"#,
    )
    .unwrap();
    Command::cargo_bin("sw")
        .unwrap()
        .args([
            "tc",
            source.to_str().unwrap(),
            "-o",
            target.to_str().unwrap(),
            "--claude-model",
            "claude-test",
        ])
        .env_remove("HOME")
        .env("USERPROFILE", directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("迁移完成: codex -> claude"));
    assert!(target.join("history.jsonl").exists());
}

#[test]
fn pack_exports_with_client_first_syntax() {
    let directory = tempdir().unwrap();
    let source = directory.path().join("claude.jsonl");
    let output = directory.path().join("session.sw.json");
    fs::write(
        &source,
        r#"{"parentUuid":null,"cwd":".","sessionId":"00000000-0000-4000-8000-000000000002","type":"assistant","uuid":"a","timestamp":"2026-06-19T00:00:00.000Z","message":{"id":"msg_x","type":"message","role":"assistant","model":"claude-test","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null}}"#,
    )
    .unwrap();
    Command::cargo_bin("sw")
        .unwrap()
        .args([
            "pack",
            "claude",
            source.to_str().unwrap(),
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(output.exists());
}

#[test]
fn doctor_supports_json_output() {
    let directory = tempdir().unwrap();
    Command::cargo_bin("session-weaver")
        .unwrap()
        .args(["doctor", "--json"])
        .env_remove("HOME")
        .env("USERPROFILE", directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rust_version\""));
}

#[test]
fn check_validates_a_claude_session() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("session.jsonl");
    fs::write(
        &path,
        r#"{"parentUuid":null,"cwd":".","sessionId":"00000000-0000-4000-8000-000000000001","type":"assistant","uuid":"a","timestamp":"2026-06-19T00:00:00.000Z","message":{"id":"msg_x","type":"message","role":"assistant","model":"claude-test","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null}}"#,
    )
    .unwrap();
    Command::cargo_bin("session-weaver")
        .unwrap()
        .args(["check", path.to_str().unwrap(), "--from", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("兼容性检查通过"));
}

#[test]
fn inspect_resolves_native_session_id() {
    let directory = tempdir().unwrap();
    let nested = directory.path().join("sessions/2026/06/19");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("rollout-test-native-id.jsonl"),
        r#"{"timestamp":"2026-06-19T00:00:00.000Z","type":"session_meta","payload":{"id":"native-id","cwd":"."}}"#,
    )
    .unwrap();
    Command::cargo_bin("session-weaver")
        .unwrap()
        .args(["inspect", "native-id", "--from", "codex", "--json"])
        .env("SESSION_WEAVER_CODEX_HOME", directory.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"session_id\": \"native-id\""));
}

#[test]
fn import_updates_native_discovery_index() {
    let directory = tempdir().unwrap();
    let package_path = directory.path().join("session.sw.json");
    let target = directory.path().join("claude-home");
    let package = export_package(&Conversation {
        id: "source".into(),
        source: ClientKind::Codex,
        metadata: ConversationMeta {
            cwd: Some("/tmp/demo".into()),
            model: Some("claude-test".into()),
            ..Default::default()
        },
        entries: vec![],
    })
    .unwrap();
    fs::write(&package_path, serde_json::to_vec(&package).unwrap()).unwrap();
    Command::cargo_bin("session-weaver")
        .unwrap()
        .args([
            "import",
            package_path.to_str().unwrap(),
            "--to",
            "claude",
            "--output",
            target.to_str().unwrap(),
            "--claude-model",
            "claude-test",
        ])
        .env_remove("HOME")
        .env("USERPROFILE", directory.path())
        .assert()
        .success();
    assert!(target.join("history.jsonl").exists());
}
