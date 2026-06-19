use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::core::{ClientKind, Conversation};

#[derive(Debug, Clone, Serialize)]
pub struct SessionRecord {
    pub id: String,
    pub client: ClientKind,
    pub path: PathBuf,
}

pub fn discover_sessions(client: ClientKind, root: &Path) -> Result<Vec<SessionRecord>> {
    let search_root = match client {
        ClientKind::Claude => root.join("projects"),
        ClientKind::Codex => root.join("sessions"),
    };
    if !search_root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    for entry in WalkDir::new(&search_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .filter(|entry| !is_auxiliary_session_log(client, entry.path()))
    {
        if let Some(id) = native_id(client, entry.path())? {
            if !seen.insert(id.clone()) {
                bail!("duplicate {client} session id found: {id}");
            }
            records.push(SessionRecord {
                id,
                client,
                path: entry.path().to_path_buf(),
            });
        }
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(records)
}

fn is_auxiliary_session_log(client: ClientKind, path: &Path) -> bool {
    client == ClientKind::Claude
        && path
            .components()
            .any(|component| component.as_os_str() == "subagents")
}

fn native_id(client: ClientKind, path: &Path) -> Result<Option<String>> {
    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid JSONL while scanning {}", path.display()))?;
        return Ok(match client {
            ClientKind::Claude => value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    path.file_stem()
                        .and_then(|name| name.to_str())
                        .map(str::to_string)
                }),
            ClientKind::Codex => value
                .pointer("/payload/id")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    Ok(None)
}

pub fn register_codex_thread(
    codex_root: &Path,
    conversation: &Conversation,
    rollout_path: &Path,
) -> Result<()> {
    let database = codex_root.join("state_5.sqlite");
    if !database.exists() {
        return Ok(());
    }
    let mut connection = Connection::open(&database)?;
    let columns = table_columns(&connection, "threads")?;
    let now = Utc::now().timestamp();
    let candidates = [
        ("id", conversation.id.clone()),
        ("rollout_path", rollout_path.display().to_string()),
        ("created_at", now.to_string()),
        ("updated_at", now.to_string()),
        (
            "cwd",
            conversation
                .metadata
                .cwd
                .clone()
                .unwrap_or_else(|| ".".into()),
        ),
        (
            "title",
            conversation
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| conversation.id.clone()),
        ),
        ("source", "cli".into()),
        ("model_provider", "imported".into()),
        ("sandbox_policy", r#"{"type":"workspace-write"}"#.into()),
        ("approval_mode", "on-request".into()),
        ("cli_version", env!("CARGO_PKG_VERSION").into()),
        ("first_user_message", first_user_text(conversation)),
        ("memory_mode", "enabled".into()),
    ];
    let selected: Vec<(&str, String)> = candidates
        .into_iter()
        .filter(|(name, _)| columns.contains(*name))
        .collect();
    for required in [
        "id",
        "rollout_path",
        "created_at",
        "updated_at",
        "cwd",
        "title",
    ] {
        if !selected.iter().any(|(name, _)| *name == required) {
            bail!("Codex threads schema is missing required column {required}");
        }
    }
    let names = selected.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    let placeholders = (1..=selected.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>();
    let updates = names
        .iter()
        .filter(|name| **name != "id")
        .map(|name| format!("{name}=excluded.{name}"))
        .collect::<Vec<_>>();
    let sql = format!(
        "INSERT INTO threads ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {}",
        names.join(","),
        placeholders.join(","),
        updates.join(",")
    );
    let transaction = connection.transaction()?;
    transaction.execute(
        &sql,
        params_from_iter(selected.iter().map(|(_, value)| value)),
    )?;
    transaction.commit()?;
    Ok(())
}

fn table_columns(connection: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn first_user_text(conversation: &Conversation) -> String {
    conversation
        .entries
        .iter()
        .find_map(|entry| match entry {
            crate::core::Entry::User(message) => Some(message.plain_text()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| conversation.metadata.title.clone().unwrap_or_default())
}
