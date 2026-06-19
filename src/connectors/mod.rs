pub mod claude;
pub mod codex;

use std::path::Path;

use anyhow::Result;

use crate::core::Conversation;

pub trait Connector {
    fn parse(&self, path: &Path) -> Result<Conversation>;
    fn render(&self, conversation: &Conversation) -> Result<Vec<serde_json::Value>>;
}
