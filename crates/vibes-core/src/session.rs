use serde::{Deserialize, Serialize};

use crate::scope::ChatScope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHandle {
    pub codex_session_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBinding {
    pub scope: ChatScope,
    pub session: SessionHandle,
    pub workspace_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartSessionRequest {
    pub scope: ChatScope,
    pub label: Option<String>,
    pub workspace_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeSessionRequest {
    pub scope: ChatScope,
    pub target: String,
    pub workspace_root: String,
}

impl SessionHandle {
    pub fn short_id(&self) -> &str {
        short_session_id(&self.codex_session_id)
    }
}

pub fn short_session_id(session_id: &str) -> &str {
    session_id.split('-').next().unwrap_or(session_id)
}

#[cfg(test)]
mod tests {
    use super::{SessionHandle, short_session_id};

    #[test]
    fn keeps_short_codex_session_prefix() {
        assert_eq!(short_session_id("019d6361-f755-7992-b08a"), "019d6361");
        assert_eq!(short_session_id("plain-name"), "plain");
    }

    #[test]
    fn session_handle_uses_short_id_helper() {
        let handle = SessionHandle {
            codex_session_id: "019d6361-f755-7992-b08a".to_owned(),
            display_name: "codex-rust".to_owned(),
        };

        assert_eq!(handle.short_id(), "019d6361");
    }
}
