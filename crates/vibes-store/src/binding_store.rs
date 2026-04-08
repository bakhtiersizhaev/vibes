use thiserror::Error;
use vibes_core::{ChatScope, SessionBinding, SessionBindingLookup, SessionBindingWriter};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub trait SessionBindingStore: Send + Sync + SessionBindingLookup + SessionBindingWriter {
    fn get_by_scope(&self, scope: &ChatScope) -> Result<Option<SessionBinding>, StoreError>;
    fn get_by_session_id(&self, session_id: &str) -> Result<Option<SessionBinding>, StoreError>;
    fn get_by_display_name(&self, display_name: &str)
    -> Result<Option<SessionBinding>, StoreError>;
    fn get_by_topic_id(&self, topic_id: i64) -> Result<Option<SessionBinding>, StoreError>;
    fn list(&self) -> Result<Vec<SessionBinding>, StoreError>;
}
