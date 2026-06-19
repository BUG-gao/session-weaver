use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::core::{
    ClientKind, Conversation, ConversationMeta, Entry, ImageData, Message, Part, Thought, ToolCall,
    ToolResult,
};

pub fn parse_file(path: &Path) -> Result<Conversation> {
    let reader = BufReader::new(
        File::open(path)
            .with_context(|| format!("cannot open Codex session {}", path.display()))?,
    );
    let mut conversation = Conversation {
        id: Uuid::now_v7().to_string(),
        source: ClientKind::Codex,
        metadata: ConversationMeta::default(),
        entries: Vec::new(),
    };
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("cannot read line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid Codex JSON at line {}", line_index + 1))?;
        if let Some(time) = timestamp(&value) {
            update_bounds(&mut conversation.metadata, time);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => parse_meta(&mut conversation, &value),
            Some("turn_context") => parse_context(&mut conversation.metadata, &value),
            Some("response_item") => parse_response(&mut conversation.entries, &value),
            _ => {}
        }
    }
    Ok(conversation)
}

fn parse_meta(conversation: &mut Conversation, value: &Value) {
    let Some(payload) = value.get("payload") else {
        return;
    };
    if let Some(id) = payload.get("id").and_then(Value::as_str) {
        conversation.id = id.to_string();
    }
    if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
        conversation.metadata.cwd = Some(cwd.to_string());
    }
    conversation.metadata.provider = payload
        .get("model_provider")
        .and_then(Value::as_str)
        .map(str::to_string);
    conversation.metadata.extras = payload.clone();
}

fn parse_context(metadata: &mut ConversationMeta, value: &Value) {
    let Some(payload) = value.get("payload") else {
        return;
    };
    if let Some(cwd) = payload.get("cwd").and_then(Value::as_str) {
        metadata.cwd = Some(cwd.to_string());
    }
    if let Some(model) = payload.get("model").and_then(Value::as_str) {
        metadata.model = Some(model.to_string());
    }
}

fn parse_response(entries: &mut Vec<Entry>, value: &Value) {
    let Some(payload) = value.get("payload") else {
        return;
    };
    let time = timestamp(value);
    match payload.get("type").and_then(Value::as_str) {
        Some("message") => {
            let role = payload
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("assistant");
            let parts = payload
                .get("content")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(parse_part).collect())
                .unwrap_or_default();
            let message = Message {
                id: payload
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                parts,
                timestamp: time,
                model: None,
            };
            entries.push(match role {
                "assistant" => Entry::Assistant(message),
                "developer" => Entry::Developer(message),
                "system" => Entry::System(message),
                _ => Entry::User(message),
            });
        }
        Some("reasoning") => {
            let content = payload
                .get("summary")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            entries.push(Entry::Thought(Thought {
                id: payload
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                summary: (!content.is_empty()).then(|| content.clone()),
                content,
                timestamp: time,
            }));
        }
        Some("function_call" | "custom_tool_call") => {
            let arguments = payload
                .get("arguments")
                .or_else(|| payload.get("input"))
                .cloned()
                .unwrap_or(Value::Null);
            let arguments = arguments
                .as_str()
                .and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or(arguments);
            entries.push(Entry::ToolCall(ToolCall {
                id: payload
                    .get("call_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                arguments,
                timestamp: time,
            }));
        }
        Some("function_call_output" | "custom_tool_call_output") => {
            entries.push(Entry::ToolResult(ToolResult {
                id: format!("result-{}", Uuid::new_v4()),
                call_id: payload
                    .get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                output: payload.get("output").cloned().unwrap_or(Value::Null),
                is_error: false,
                timestamp: time,
            }));
        }
        _ => {}
    }
}

fn parse_part(value: &Value) -> Part {
    match value.get("type").and_then(Value::as_str) {
        Some("input_text" | "output_text" | "text") => Part::Text(
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        Some("input_image") => Part::Image(ImageData {
            media_type: value
                .get("media_type")
                .and_then(Value::as_str)
                .unwrap_or("image/png")
                .to_string(),
            data: value
                .get("image_url")
                .and_then(Value::as_str)
                .and_then(|url| url.split_once(","))
                .map(|(_, data)| data)
                .unwrap_or_default()
                .to_string(),
            source_url: value
                .get("image_url")
                .and_then(Value::as_str)
                .filter(|url| !url.starts_with("data:"))
                .map(str::to_string),
        }),
        Some(kind) => Part::Opaque {
            kind: kind.into(),
            value: value.clone(),
        },
        None => Part::Opaque {
            kind: "unknown".into(),
            value: value.clone(),
        },
    }
}

pub fn render(conversation: &Conversation) -> Result<Vec<Value>> {
    let id = normalized_uuid(&conversation.id);
    let created = conversation.metadata.created_at.unwrap_or_else(Utc::now);
    let fallback = conversation.metadata.updated_at.unwrap_or(created);
    let cwd = conversation.metadata.cwd.as_deref().unwrap_or(".");
    let mut output = vec![json!({
        "timestamp": time(created),
        "type": "session_meta",
        "payload": {
            "id": id,
            "timestamp": time(created),
            "cwd": cwd,
            "originator": "session-weaver",
            "cli_version": env!("CARGO_PKG_VERSION"),
            "source": "import",
            "model_provider": conversation.metadata.provider.as_deref().unwrap_or("imported"),
            "base_instructions": {"text": "Imported by Session Weaver."}
        }
    })];
    let mut turn_id: Option<String> = None;
    let mut last_message = String::new();
    for entry in &conversation.entries {
        if matches!(entry, Entry::User(_)) {
            close_turn(&mut output, &mut turn_id, &last_message, fallback);
            start_turn(
                &mut output,
                &mut turn_id,
                cwd,
                entry.timestamp().unwrap_or(fallback),
            );
        } else if turn_id.is_none() && !matches!(entry, Entry::Developer(_) | Entry::System(_)) {
            start_turn(
                &mut output,
                &mut turn_id,
                cwd,
                entry.timestamp().unwrap_or(fallback),
            );
        }
        match entry {
            Entry::User(message)
            | Entry::Assistant(message)
            | Entry::Developer(message)
            | Entry::System(message) => {
                let (role, assistant) = match entry {
                    Entry::Assistant(_) => ("assistant", true),
                    Entry::Developer(_) => ("developer", false),
                    Entry::System(_) => ("system", false),
                    _ => ("user", false),
                };
                let blocks = encode_parts(&message.parts, assistant);
                output.push(json!({
                    "timestamp": time(message.timestamp.unwrap_or(fallback)),
                    "type": "response_item",
                    "payload": {"type": "message", "role": role, "content": blocks}
                }));
                if assistant {
                    last_message = message.plain_text();
                }
            }
            Entry::Thought(thought) => output.push(json!({
                "timestamp": time(thought.timestamp.unwrap_or(fallback)),
                "type": "response_item",
                "payload": {"type": "reasoning", "summary": [{
                    "type": "summary_text",
                    "text": thought.summary.as_deref().unwrap_or(&thought.content)
                }]}
            })),
            Entry::ToolCall(call) => output.push(json!({
                "timestamp": time(call.timestamp.unwrap_or(fallback)),
                "type": "response_item",
                "payload": {
                    "type": "function_call",
                    "id": Uuid::now_v7().to_string(),
                    "name": call.name,
                    "call_id": call.id,
                    "arguments": value_as_string(&call.arguments)
                }
            })),
            Entry::ToolResult(result) => output.push(json!({
                "timestamp": time(result.timestamp.unwrap_or(fallback)),
                "type": "response_item",
                "payload": {
                    "type": "function_call_output",
                    "call_id": result.call_id,
                    "output": value_as_string(&result.output)
                }
            })),
        }
    }
    close_turn(&mut output, &mut turn_id, &last_message, fallback);
    Ok(output)
}

fn start_turn(
    output: &mut Vec<Value>,
    active: &mut Option<String>,
    cwd: &str,
    timestamp: DateTime<Utc>,
) {
    let id = Uuid::now_v7().to_string();
    output.push(json!({
        "timestamp": time(timestamp),
        "type": "event_msg",
        "payload": {
            "type": "task_started",
            "turn_id": id,
            "model_context_window": 950000,
            "collaboration_mode_kind": "default"
        }
    }));
    output.push(json!({
        "timestamp": time(timestamp),
        "type": "turn_context",
        "payload": {
            "turn_id": id,
            "cwd": cwd,
            "approval_policy": "on-request",
            "sandbox_policy": {"type": "workspace-write"},
            "collaboration_mode": {"mode": "default"}
        }
    }));
    *active = Some(id);
}

fn close_turn(
    output: &mut Vec<Value>,
    active: &mut Option<String>,
    last_message: &str,
    timestamp: DateTime<Utc>,
) {
    if let Some(id) = active.take() {
        output.push(json!({
            "timestamp": time(timestamp),
            "type": "event_msg",
            "payload": {
                "type": "task_complete",
                "turn_id": id,
                "last_agent_message": last_message
            }
        }));
    }
}

fn encode_parts(parts: &[Part], assistant: bool) -> Vec<Value> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => json!({
                "type": if assistant {"output_text"} else {"input_text"},
                "text": text
            }),
            Part::Image(image) => json!({
                "type": "input_image",
                "image_url": image.source_url.clone().unwrap_or_else(|| {
                    format!("data:{};base64,{}", image.media_type, image.data)
                })
            }),
            Part::Json(value) => json!({
                "type": if assistant {"output_text"} else {"input_text"},
                "text": value.to_string()
            }),
            Part::Opaque { value, .. } => value.clone(),
        })
        .collect()
}

fn normalized_uuid(candidate: &str) -> String {
    Uuid::parse_str(candidate)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| Uuid::now_v7().to_string())
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

fn time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn value_as_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}
