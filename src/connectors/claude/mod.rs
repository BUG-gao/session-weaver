use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::core::{
    ClientKind, Conversation, ConversationMeta, Entry, Finding, ImageData, Message, Part, Severity,
    Stage, Thought, ToolCall, ToolResult,
};

pub mod desktop;

pub fn parse_file(path: &Path) -> Result<Conversation> {
    let reader = BufReader::new(
        File::open(path)
            .with_context(|| format!("cannot open Claude session {}", path.display()))?,
    );
    let mut conversation = Conversation {
        id: Uuid::new_v4().to_string(),
        source: ClientKind::Claude,
        metadata: ConversationMeta::default(),
        entries: Vec::new(),
    };
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("cannot read line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid Claude JSON at line {}", line_index + 1))?;
        absorb_metadata(&mut conversation, &value);
        match value.get("type").and_then(Value::as_str) {
            Some("user") => parse_user(&mut conversation.entries, &value),
            Some("assistant") => parse_assistant(&mut conversation.entries, &value),
            _ => {}
        }
    }
    if conversation.metadata.title.is_none() {
        conversation.metadata.title = first_user_text(&conversation);
    }
    Ok(conversation)
}

fn absorb_metadata(conversation: &mut Conversation, value: &Value) {
    if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
        conversation.id = id.to_string();
    }
    if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
        conversation.metadata.cwd = Some(cwd.to_string());
    }
    if let Some(branch) = value.get("gitBranch").and_then(Value::as_str) {
        conversation.metadata.git_branch = Some(branch.to_string());
    }
    if let Some(timestamp) = timestamp(value) {
        update_bounds(&mut conversation.metadata, timestamp);
    }
}

fn parse_user(entries: &mut Vec<Entry>, value: &Value) {
    let Some(content) = value.pointer("/message/content") else {
        return;
    };
    let id = value
        .get("uuid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let time = timestamp(value);
    if let Some(text) = content.as_str() {
        if !text.trim().is_empty() {
            entries.push(Entry::User(Message {
                id,
                parts: vec![Part::Text(text.to_string())],
                timestamp: time,
                model: None,
            }));
        }
        return;
    }
    let Some(blocks) = content.as_array() else {
        return;
    };
    let mut parts = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            flush_user(entries, &id, time, &mut parts);
            entries.push(Entry::ToolResult(ToolResult {
                id: format!("{id}:result:{index}"),
                call_id: block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                output: block.get("content").cloned().unwrap_or(Value::Null),
                is_error: block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                timestamp: time,
            }));
        } else {
            parts.push(parse_part(block));
        }
    }
    flush_user(entries, &id, time, &mut parts);
}

fn flush_user(
    entries: &mut Vec<Entry>,
    id: &str,
    timestamp: Option<DateTime<Utc>>,
    parts: &mut Vec<Part>,
) {
    if !parts.is_empty() {
        entries.push(Entry::User(Message {
            id: id.to_string(),
            parts: std::mem::take(parts),
            timestamp,
            model: None,
        }));
    }
}

fn parse_assistant(entries: &mut Vec<Entry>, value: &Value) {
    let Some(blocks) = value.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    let base_id = value
        .get("uuid")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let model = value
        .pointer("/message/model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let time = timestamp(value);
    let mut parts = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        match block.get("type").and_then(Value::as_str) {
            Some("thinking") => {
                flush_assistant(entries, base_id, time, &model, &mut parts);
                entries.push(Entry::Thought(Thought {
                    id: format!("{base_id}:thought:{index}"),
                    summary: None,
                    content: block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    timestamp: time,
                }));
            }
            Some("tool_use") => {
                flush_assistant(entries, base_id, time, &model, &mut parts);
                entries.push(Entry::ToolCall(ToolCall {
                    id: block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    arguments: block.get("input").cloned().unwrap_or(Value::Null),
                    timestamp: time,
                }));
            }
            _ => parts.push(parse_part(block)),
        }
    }
    flush_assistant(entries, base_id, time, &model, &mut parts);
}

fn flush_assistant(
    entries: &mut Vec<Entry>,
    id: &str,
    timestamp: Option<DateTime<Utc>>,
    model: &Option<String>,
    parts: &mut Vec<Part>,
) {
    if !parts.is_empty() {
        entries.push(Entry::Assistant(Message {
            id: id.to_string(),
            parts: std::mem::take(parts),
            timestamp,
            model: model.clone(),
        }));
    }
}

fn parse_part(block: &Value) -> Part {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => Part::Text(
            block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        Some("image") => {
            let source = block.get("source").unwrap_or(&Value::Null);
            ImageData {
                media_type: source
                    .get("media_type")
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream")
                    .to_string(),
                data: source
                    .get("data")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                source_url: source
                    .get("url")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }
            .into()
        }
        Some(kind) => Part::Opaque {
            kind: kind.to_string(),
            value: block.clone(),
        },
        None => Part::Opaque {
            kind: "unknown".into(),
            value: block.clone(),
        },
    }
}

impl From<ImageData> for Part {
    fn from(value: ImageData) -> Self {
        Part::Image(value)
    }
}

pub fn render(conversation: &Conversation, fallback_model: &str) -> Result<Vec<Value>> {
    if fallback_model.trim().is_empty() {
        bail!("Claude fallback model cannot be empty");
    }
    let session_id = normalized_uuid(&conversation.id);
    let cwd = conversation.metadata.cwd.as_deref().unwrap_or(".");
    let branch = conversation
        .metadata
        .git_branch
        .as_deref()
        .unwrap_or("HEAD");
    // The Claude desktop app indexes sessions by their declared `model`. A
    // session whose assistant records carry a non-Claude model id (e.g. a
    // Codex source's `gpt-5.5`) is silently dropped from the app's history
    // list, so any model coming from the source must be coerced to a valid
    // Claude model before it lands in the Claude transcript.
    let default_model = conversation
        .metadata
        .model
        .as_deref()
        .filter(|value| is_claude_model(value))
        .unwrap_or(fallback_model);
    let mut output = Vec::new();
    let mut parent: Option<String> = None;
    let mut call_uuids: HashMap<String, String> = HashMap::new();

    for entry in &conversation.entries {
        let uuid = Uuid::new_v4().to_string();
        let time = entry
            .timestamp()
            .unwrap_or_else(Utc::now)
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut line = json!({
            "parentUuid": parent,
            "isSidechain": false,
            "userType": "external",
            "cwd": cwd,
            "sessionId": session_id,
            "version": env!("CARGO_PKG_VERSION"),
            "gitBranch": branch,
            "uuid": uuid,
            "timestamp": time,
        });
        match entry {
            Entry::User(message) => {
                line["type"] = json!("user");
                line["message"] = json!({
                    "role": "user",
                    "content": encode_parts(&message.parts),
                });
                line["permissionMode"] = json!("default");
            }
            Entry::Assistant(message) => {
                line["type"] = json!("assistant");
                line["message"] = assistant_message(
                    encode_parts(&message.parts),
                    "end_turn",
                    message
                        .model
                        .as_deref()
                        .filter(|value| is_claude_model(value))
                        .unwrap_or(default_model),
                );
            }
            Entry::Developer(message) | Entry::System(message) => {
                let role = if matches!(entry, Entry::Developer(_)) {
                    "developer"
                } else {
                    "system"
                };
                let mut parts = message.parts.clone();
                let prefix = format!("[Session Weaver imported {role} message]");
                match parts.first_mut() {
                    Some(Part::Text(text)) => *text = format!("{prefix}\n{text}"),
                    _ => parts.insert(0, Part::Text(prefix)),
                }
                line["type"] = json!("user");
                line["message"] = json!({
                    "role": "user",
                    "content": encode_parts(&parts),
                });
                line["permissionMode"] = json!("default");
            }
            Entry::Thought(_) => {
                // Reasoning cannot survive the trip to a Claude session: the
                // Anthropic API requires every `thinking` block replayed in
                // history to carry the original cryptographic `signature`,
                // which a migrated transcript cannot forge. Emitting an
                // unsigned (or empty) thinking block makes the whole session
                // unusable — the API rejects the next turn with a 400. Drop
                // reasoning entirely instead, keeping the parent chain intact.
                continue;
            }
            Entry::ToolCall(call) => {
                line["type"] = json!("assistant");
                line["message"] = assistant_message(
                    json!([{
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": normalize_tool_input(&call.arguments),
                        "caller": {"type": "direct"}
                    }]),
                    "tool_use",
                    default_model,
                );
                call_uuids.insert(call.id.clone(), uuid.clone());
            }
            Entry::ToolResult(result) => {
                line["type"] = json!("user");
                line["message"] = json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": result.call_id,
                        "content": normalize_tool_result_content(&result.output),
                        "is_error": result.is_error,
                    }]
                });
                line["sourceToolAssistantUUID"] = call_uuids.get(&result.call_id).cloned().into();
            }
        }
        parent = Some(uuid);
        output.push(line);
    }
    Ok(output)
}

/// The Anthropic API requires `tool_use.input` to be a JSON object. Codex
/// stores some tool arguments as a raw string (e.g. an `apply_patch` body) or
/// as a JSON-encoded string, both of which trigger a 400 on replay. Parse a
/// JSON object when possible, otherwise wrap the value in an object.
fn normalize_tool_input(arguments: &Value) -> Value {
    match arguments {
        Value::Object(_) => arguments.clone(),
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => Value::Object(map),
            _ => json!({ "input": text }),
        },
        other => json!({ "input": other }),
    }
}

/// Normalize a tool result `content` to what the Anthropic API accepts: a
/// string, or an array of `text`/`image` blocks. Codex emits foreign block
/// types (notably `input_image` with a data URL) that must be rewritten to
/// Anthropic `image` blocks, or the next turn is rejected with a 400.
fn normalize_tool_result_content(output: &Value) -> Value {
    match output {
        Value::String(_) => output.clone(),
        Value::Array(items) => Value::Array(items.iter().map(normalize_result_block).collect()),
        other => Value::String(other.to_string()),
    }
}

fn normalize_result_block(block: &Value) -> Value {
    match block.get("type").and_then(Value::as_str) {
        Some("text") | Some("image") => block.clone(),
        Some("input_image") => block
            .get("image_url")
            .and_then(Value::as_str)
            .map(image_block_from_url)
            .unwrap_or_else(|| json!({"type": "text", "text": block.to_string()})),
        _ => json!({"type": "text", "text": block.to_string()}),
    }
}

/// Build an Anthropic `image` block from a URL, splitting a `data:` URL into
/// the base64 source the API expects.
fn image_block_from_url(url: &str) -> Value {
    if let Some((meta, data)) = url
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(','))
    {
        let media_type = meta.split(';').next().unwrap_or("image/png");
        return json!({
            "type": "image",
            "source": {"type": "base64", "media_type": media_type, "data": data},
        });
    }
    json!({"type": "image", "source": {"type": "url", "url": url}})
}

/// Whether a model id is a Claude model the Claude desktop app can resolve.
/// Source transcripts from other providers carry foreign ids (e.g. `gpt-5.5`)
/// that must not be written into a Claude session, or the app hides it.
fn is_claude_model(model: &str) -> bool {
    let model = model.trim();
    !model.is_empty() && model.to_ascii_lowercase().starts_with("claude")
}

fn assistant_message(content: Value, stop_reason: &str, model: &str) -> Value {
    json!({
        "id": format!("msg_{}", Uuid::new_v4().simple()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
    })
}

fn encode_parts(parts: &[Part]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|part| match part {
                Part::Text(text) => json!({"type": "text", "text": text}),
                Part::Image(image) if image.source_url.is_some() => {
                    json!({"type": "image", "source": {
                        "type": "url",
                        "url": image.source_url,
                    }})
                }
                Part::Image(image) => json!({"type": "image", "source": {
                    "type": "base64",
                    "media_type": image.media_type,
                    "data": image.data,
                }}),
                Part::Json(value) => json!({"type": "text", "text": value.to_string()}),
                Part::Opaque { kind, value } => {
                    let mut object = value.as_object().cloned().unwrap_or_default();
                    object.insert("type".into(), Value::String(kind.clone()));
                    Value::Object(object)
                }
            })
            .collect(),
    )
}

pub fn validate_native(values: &[Value]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let message = value.get("message").unwrap_or(&Value::Null);
        if message
            .get("model")
            .and_then(Value::as_str)
            .is_none_or(|model| model.trim().is_empty())
        {
            findings.push(compat_error(
                index,
                "claude_model_missing",
                "/message/model",
            ));
        }
        if !matches!(
            message.get("stop_reason").and_then(Value::as_str),
            Some("end_turn" | "tool_use")
        ) {
            findings.push(compat_error(
                index,
                "claude_stop_reason_invalid",
                "/message/stop_reason",
            ));
        }
        if let Some(blocks) = message.get("content").and_then(Value::as_array) {
            for (block_index, block) in blocks.iter().enumerate() {
                if block.get("type").and_then(Value::as_str) == Some("text")
                    && block.get("text").and_then(Value::as_str).is_none()
                {
                    findings.push(compat_error(
                        index,
                        "claude_text_missing",
                        &format!("/message/content/{block_index}/text"),
                    ));
                }
            }
        }
    }
    findings
}

fn compat_error(record: usize, code: &str, path: &str) -> Finding {
    Finding {
        severity: Severity::Error,
        stage: Stage::Compatibility,
        code: code.into(),
        record: Some(record + 1),
        path: Some(path.into()),
        message: code.replace('_', " "),
    }
}

fn normalized_uuid(candidate: &str) -> String {
    Uuid::parse_str(candidate)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| Uuid::new_v4().to_string())
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc))
}

fn update_bounds(metadata: &mut ConversationMeta, timestamp: DateTime<Utc>) {
    metadata.created_at = Some(
        metadata
            .created_at
            .map_or(timestamp, |old| old.min(timestamp)),
    );
    metadata.updated_at = Some(
        metadata
            .updated_at
            .map_or(timestamp, |old| old.max(timestamp)),
    );
}

fn first_user_text(conversation: &Conversation) -> Option<String> {
    // Skip imported boilerplate (the injected developer/system, permissions
    // and app-context preamble) so the derived title reflects the first real
    // user prompt instead of `[Session Weaver imported developer message]`.
    const BOILERPLATE_PREFIXES: [&str; 4] = [
        "[Session Weaver imported",
        "[transession imported",
        "<permissions",
        "<app-context",
    ];
    conversation.entries.iter().find_map(|entry| match entry {
        Entry::User(message) => {
            let text = message.plain_text();
            let first_line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
            let is_boilerplate = BOILERPLATE_PREFIXES
                .iter()
                .any(|prefix| first_line.starts_with(prefix));
            (!is_boilerplate).then(|| first_line.chars().take(80).collect())
        }
        _ => None,
    })
}
