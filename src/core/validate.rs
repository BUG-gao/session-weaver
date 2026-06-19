use std::collections::HashSet;

use super::{Conversation, Entry, Finding, Severity, Stage};

pub fn validate_conversation(conversation: &Conversation) -> Vec<Finding> {
    let calls: HashSet<&str> = conversation
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::ToolCall(call) => Some(call.id.as_str()),
            _ => None,
        })
        .collect();
    let mut ids = HashSet::new();
    let mut findings = Vec::new();

    for (index, entry) in conversation.entries.iter().enumerate() {
        let id = match entry {
            Entry::User(value)
            | Entry::Assistant(value)
            | Entry::Developer(value)
            | Entry::System(value) => value.id.as_str(),
            Entry::Thought(value) => value.id.as_str(),
            Entry::ToolCall(value) => value.id.as_str(),
            Entry::ToolResult(value) => value.id.as_str(),
        };
        if !id.is_empty() && !ids.insert(id) {
            findings.push(error(index, "duplicate_entry_id", "entry id is duplicated"));
        }
        match entry {
            Entry::Assistant(message)
                if message
                    .model
                    .as_ref()
                    .is_some_and(|model| model.trim().is_empty()) =>
            {
                findings.push(error(
                    index,
                    "assistant_model_empty",
                    "assistant model must not be empty",
                ));
            }
            Entry::ToolCall(call) if call.name.trim().is_empty() => {
                findings.push(error(
                    index,
                    "tool_name_empty",
                    "tool name must not be empty",
                ));
            }
            Entry::ToolResult(result) if !calls.contains(result.call_id.as_str()) => {
                findings.push(error(
                    index,
                    "orphan_tool_result",
                    "tool result has no matching call",
                ));
            }
            _ => {}
        }
    }
    findings
}

fn error(record: usize, code: &str, message: &str) -> Finding {
    Finding {
        severity: Severity::Error,
        stage: Stage::Semantic,
        code: code.into(),
        record: Some(record + 1),
        path: None,
        message: message.into(),
    }
}
