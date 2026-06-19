use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Claude,
    Codex,
}

impl std::fmt::Display for ClientKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        })
    }
}

impl std::str::FromStr for ClientKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err(format!("unsupported client: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub source: ClientKind,
    #[serde(default)]
    pub metadata: ConversationMeta,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl Conversation {
    pub fn semantically_matches(&self, other: &Self) -> Result<(), Vec<String>> {
        super::equivalence::compare(self, other)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub title: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub provider: Option<String>,
    #[serde(default)]
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Entry {
    User(Message),
    Assistant(Message),
    Developer(Message),
    System(Message),
    Thought(Thought),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

impl Entry {
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::User(value)
            | Self::Assistant(value)
            | Self::Developer(value)
            | Self::System(value) => value.timestamp,
            Self::Thought(value) => value.timestamp,
            Self::ToolCall(value) => value.timestamp,
            Self::ToolResult(value) => value.timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    pub timestamp: Option<DateTime<Utc>>,
    pub model: Option<String>,
}

impl Message {
    pub fn text(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            parts: vec![Part::Text(text.into())],
            timestamp: None,
            model: None,
        }
    }

    pub fn plain_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Part {
    Text(String),
    Image(ImageData),
    Json(Value),
    Opaque { kind: String, value: Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageData {
    pub media_type: String,
    pub data: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thought {
    pub id: String,
    pub summary: Option<String>,
    pub content: String,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: String,
    pub call_id: String,
    pub output: Value,
    pub is_error: bool,
    pub timestamp: Option<DateTime<Utc>>,
}
