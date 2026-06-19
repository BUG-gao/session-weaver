use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::Conversation;

pub const PACKAGE_SCHEMA: &str = "session-weaver/conversation";
pub const PACKAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableEnvelope {
    pub schema: String,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub digest: String,
    pub conversation: Conversation,
}

impl PortableEnvelope {
    pub fn build(conversation: Conversation) -> Result<Self> {
        let digest = digest(&conversation)?;
        Ok(Self {
            schema: PACKAGE_SCHEMA.into(),
            version: PACKAGE_VERSION,
            created_at: Utc::now(),
            digest,
            conversation,
        })
    }

    pub fn verify(self) -> Result<Conversation> {
        if self.schema != PACKAGE_SCHEMA || self.version != PACKAGE_VERSION {
            bail!(
                "unsupported portable package {} v{}",
                self.schema,
                self.version
            );
        }
        if self.digest != digest(&self.conversation)? {
            bail!("portable package checksum mismatch");
        }
        Ok(self.conversation)
    }
}

fn digest(conversation: &Conversation) -> Result<String> {
    let bytes = serde_json::to_vec(conversation)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
