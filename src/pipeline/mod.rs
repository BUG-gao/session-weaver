use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::connectors::{claude, codex};
use crate::core::{ClientKind, Conversation, Severity, validate_conversation};
use crate::portable::PortableEnvelope;

pub struct MoveOptions<'a> {
    pub source: &'a Path,
    pub source_kind: ClientKind,
    pub target_kind: ClientKind,
    pub target_root: &'a Path,
    pub overwrite: bool,
    pub claude_model: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveReport {
    pub source_id: String,
    pub target_id: String,
    pub output: PathBuf,
    pub event_count: usize,
    pub warnings: Vec<String>,
}

pub fn export_package(conversation: &Conversation) -> Result<PortableEnvelope> {
    PortableEnvelope::build(conversation.clone())
}

pub fn import_package(package: &PortableEnvelope) -> Result<Conversation> {
    package.clone().verify()
}

pub fn move_session(options: MoveOptions<'_>) -> Result<MoveReport> {
    let mut conversation = parse(options.source_kind, options.source)?;
    let source_id = conversation.id.clone();
    let findings = validate_conversation(&conversation);
    if findings.iter().any(|item| item.severity == Severity::Error) {
        bail!("source conversation failed semantic validation");
    }
    conversation.id = match options.target_kind {
        ClientKind::Claude => Uuid::new_v4().to_string(),
        ClientKind::Codex => Uuid::now_v7().to_string(),
    };
    let values = render(options.target_kind, &conversation, options.claude_model)?;
    if options.target_kind == ClientKind::Claude {
        let target_findings = claude::validate_native(&values);
        if !target_findings.is_empty() {
            bail!("rendered Claude session failed compatibility validation");
        }
    }
    let output = output_path(options.target_kind, options.target_root, &conversation);
    write_jsonl_atomic(&output, &values, options.overwrite)?;
    parse(options.target_kind, &output).context("target read-back validation failed")?;
    append_native_index(options.target_kind, options.target_root, &conversation)?;
    Ok(MoveReport {
        source_id,
        target_id: conversation.id,
        output,
        event_count: conversation.entries.len(),
        warnings: findings
            .into_iter()
            .filter(|item| item.severity == Severity::Warning)
            .map(|item| item.message)
            .collect(),
    })
}

pub fn append_native_index(
    kind: ClientKind,
    root: &Path,
    conversation: &Conversation,
) -> Result<()> {
    fs::create_dir_all(root)?;
    let path = root.join(match kind {
        ClientKind::Claude => "history.jsonl",
        ClientKind::Codex => "session_index.jsonl",
    });
    let value = match kind {
        ClientKind::Claude => json!({
            "display": conversation
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| "Imported session".into()),
            "pastedContents": {},
            "timestamp": conversation
                .metadata
                .created_at
                .unwrap_or_else(Utc::now)
                .timestamp_millis(),
            "project": conversation.metadata.cwd.as_deref().unwrap_or("."),
            "sessionId": conversation.id,
        }),
        ClientKind::Codex => json!({
            "id": conversation.id,
            "thread_name": conversation
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| conversation.id.clone()),
            "updated_at": conversation
                .metadata
                .updated_at
                .unwrap_or_else(Utc::now)
                .to_rfc3339(),
        }),
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub fn parse(kind: ClientKind, path: &Path) -> Result<Conversation> {
    match kind {
        ClientKind::Claude => claude::parse_file(path),
        ClientKind::Codex => codex::parse_file(path),
    }
}

pub fn render(
    kind: ClientKind,
    conversation: &Conversation,
    claude_model: &str,
) -> Result<Vec<Value>> {
    match kind {
        ClientKind::Claude => claude::render(conversation, claude_model),
        ClientKind::Codex => codex::render(conversation),
    }
}

pub fn output_path(kind: ClientKind, root: &Path, conversation: &Conversation) -> PathBuf {
    match kind {
        ClientKind::Claude => {
            let cwd = conversation.metadata.cwd.as_deref().unwrap_or(".");
            let slug = cwd
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() {
                        character
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            let slug = if slug.starts_with('-') {
                slug
            } else {
                format!("-{slug}")
            };
            root.join("projects")
                .join(slug)
                .join(format!("{}.jsonl", conversation.id))
        }
        ClientKind::Codex => {
            let local = conversation
                .metadata
                .created_at
                .unwrap_or_else(Utc::now)
                .with_timezone(&Local);
            root.join("sessions")
                .join(format!("{:04}", local.year()))
                .join(format!("{:02}", local.month()))
                .join(format!("{:02}", local.day()))
                .join(format!(
                    "rollout-{:04}-{:02}-{:02}T{:02}-{:02}-{:02}-{}.jsonl",
                    local.year(),
                    local.month(),
                    local.day(),
                    local.hour(),
                    local.minute(),
                    local.second(),
                    conversation.id
                ))
        }
    }
}

pub fn write_jsonl_atomic(path: &Path, values: &[Value], overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        bail!("target already exists: {}", path.display());
    }
    let parent = path.parent().context("target path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = File::create(&temporary)?;
        for value in values {
            serde_json::to_writer(&mut file, value)?;
            file.write_all(b"\n")?;
        }
        file.flush()?;
        file.sync_all()?;
        if path.exists() {
            let backup = path.with_extension(format!(
                "jsonl.backup-{}",
                Utc::now().format("%Y%m%d%H%M%S")
            ));
            fs::copy(path, backup)?;
            fs::remove_file(path)?;
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
