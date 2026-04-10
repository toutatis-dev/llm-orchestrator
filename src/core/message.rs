use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Local>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenCount>,
    #[serde(default)]
    pub is_streaming: bool,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            timestamp: Local::now(),
            tokens: None,
            is_streaming: false,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    pub fn orchestrator(content: impl Into<String>) -> Self {
        Self::new(Role::Orchestrator, content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    User,
    Orchestrator,
    System,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "User"),
            Role::Orchestrator => write!(f, "Orchestrator"),
            Role::System => write!(f, "System"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCount {
    pub input: usize,
    pub output: usize,
}
