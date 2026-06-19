use super::{Conversation, Entry};

pub fn compare(left: &Conversation, right: &Conversation) -> Result<(), Vec<String>> {
    let mut differences = Vec::new();
    if left.metadata.cwd != right.metadata.cwd {
        differences.push("cwd differs".into());
    }
    if !optional_default_matches(
        left.metadata.git_branch.as_deref(),
        right.metadata.git_branch.as_deref(),
        "HEAD",
    ) {
        differences.push("git branch differs".into());
    }
    if left.entries.len() != right.entries.len() {
        differences.push(format!(
            "entry count differs: {} != {}",
            left.entries.len(),
            right.entries.len()
        ));
    } else {
        for (index, (left_entry, right_entry)) in
            left.entries.iter().zip(&right.entries).enumerate()
        {
            if !entries_match(left_entry, right_entry) {
                differences.push(format!("entry {} differs", index + 1));
            }
        }
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(differences)
    }
}

fn optional_default_matches(left: Option<&str>, right: Option<&str>, default: &str) -> bool {
    left.unwrap_or(default) == right.unwrap_or(default)
}

fn entries_match(left: &Entry, right: &Entry) -> bool {
    match (left, right) {
        (Entry::User(left), Entry::User(right))
        | (Entry::Assistant(left), Entry::Assistant(right))
        | (Entry::Developer(left), Entry::Developer(right))
        | (Entry::System(left), Entry::System(right)) => left.parts == right.parts,
        (Entry::Thought(left), Entry::Thought(right)) => {
            left.content == right.content || left.summary.as_deref() == right.summary.as_deref()
        }
        (Entry::ToolCall(left), Entry::ToolCall(right)) => {
            left.id == right.id && left.name == right.name && left.arguments == right.arguments
        }
        (Entry::ToolResult(left), Entry::ToolResult(right)) => {
            left.call_id == right.call_id
                && left.output == right.output
                && left.is_error == right.is_error
        }
        _ => false,
    }
}
