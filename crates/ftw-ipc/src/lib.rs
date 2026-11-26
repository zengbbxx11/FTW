pub mod agent;
pub mod credentials;
pub mod crm;
pub mod data;
pub mod diagnostics;
pub mod fields;
pub mod inbox;
pub mod local_agent;
pub mod mail;
mod profile;
pub mod receipt;
pub mod views;
mod windows;

pub use profile::{display_path, Profile, StorageMode};
pub use windows::{current_user_sid, token_user_sid, PipeEndpoint, PipeOpenError};

use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL_VERSION: u16 = 1;
pub const CORE_ALREADY_RUNNING: i32 = 3;

const FRAME_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request<T = Operation> {
    pub version: u16,
    pub operation: T,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Health,
    Changes,
    Crm(Box<crm::Command>),
    Mail(Box<mail::Command>),
    DesktopMail {
        owner: isize,
        command: Box<mail::Command>,
    },
    Agent(Box<mail::AgentCommand>),
    RegisteredAgentMail {
        token: mail::Secret,
        command: Box<mail::AgentCommand>,
    },
    Inbox(Box<inbox::Command>),
    AgentInbox(Box<inbox::AgentCommand>),
    RegisteredAgentInbox {
        token: mail::Secret,
        command: Box<inbox::AgentCommand>,
    },
    AgentCrm(Box<agent::Command>),
    RegisteredAgentCrm {
        token: mail::Secret,
        command: Box<agent::Command>,
    },
    AgentCollaboration {
        token: mail::Secret,
        command: Box<agent::Collaboration>,
    },
    Data(Box<data::Command>),
}

impl Operation {
    pub fn diagnostic_group(&self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Changes => "changes",
            Self::Crm(_) => "crm",
            Self::Mail(_) | Self::DesktopMail { .. } => "mail",
            Self::Inbox(_) => "inbox",
            Self::Agent(_) | Self::RegisteredAgentMail { .. } => "agent_mail",
            Self::AgentInbox(_) | Self::RegisteredAgentInbox { .. } => "agent_inbox",
            Self::AgentCrm(_) | Self::RegisteredAgentCrm { .. } => "agent_crm",
            Self::AgentCollaboration { .. } => "agent_collaboration",
            Self::Data(_) => "data",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub core_version: String,
    pub schema_version: i64,
    pub storage_mode: StorageMode,
    pub data_root: String,
    pub process_id: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChangeVersions {
    pub epoch: String,
    pub domains: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Ready {
        health: Health,
    },
    Changes {
        versions: ChangeVersions,
    },
    Crm {
        data: Box<crm::Result>,
    },
    Mail {
        data: Box<mail::Result>,
    },
    Inbox {
        data: Box<inbox::Result>,
    },
    Data {
        data: Box<data::Result>,
    },
    AgentData {
        data: serde_json::Value,
    },
    Error {
        code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

pub async fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
) -> io::Result<T> {
    let len = stream.read_u32_le().await? as usize;
    if len > FRAME_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame is too large",
        ));
    }

    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub async fn write_frame(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > FRAME_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame is too large",
        ));
    }

    stream.write_u32_le(bytes.len() as u32).await?;
    stream.write_all(&bytes).await
}
