use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use vibes_core::{
    ChatScope, SessionBinding, SessionBindingLookup, SessionBindingWriter, SessionHandle,
};

use crate::binding_store::{SessionBindingStore, StoreError};

#[derive(Debug)]
pub struct SqliteBindingStore {
    connection: Mutex<Connection>,
}

impl SqliteBindingStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        Self::from_connection(connection)
    }

    fn from_connection(connection: Connection) -> Result<Self, StoreError> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_bindings (
                scope_key TEXT PRIMARY KEY,
                scope_json TEXT NOT NULL,
                session_id TEXT NOT NULL,
                display_name TEXT NOT NULL,
                workspace_root TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );
            CREATE INDEX IF NOT EXISTS idx_session_bindings_session_id
                ON session_bindings(session_id);",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

impl SessionBindingStore for SqliteBindingStore {
    fn get_by_scope(&self, scope: &ChatScope) -> Result<Option<SessionBinding>, StoreError> {
        self.connection
            .lock()
            .expect("sqlite mutex poisoned")
            .query_row(
                "SELECT scope_json, session_id, display_name, workspace_root
                 FROM session_bindings WHERE scope_key=?1",
                params![scope.scope_key()],
                row_to_binding,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn get_by_session_id(&self, session_id: &str) -> Result<Option<SessionBinding>, StoreError> {
        self.connection
            .lock()
            .expect("sqlite mutex poisoned")
            .query_row(
                "SELECT scope_json, session_id, display_name, workspace_root
                 FROM session_bindings
                 WHERE session_id=?1
                 ORDER BY updated_at DESC
                 LIMIT 1",
                params![session_id],
                row_to_binding,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn get_by_display_name(
        &self,
        display_name: &str,
    ) -> Result<Option<SessionBinding>, StoreError> {
        self.connection
            .lock()
            .expect("sqlite mutex poisoned")
            .query_row(
                "SELECT scope_json, session_id, display_name, workspace_root
                 FROM session_bindings
                 WHERE display_name=?1
                 ORDER BY updated_at DESC
                 LIMIT 1",
                params![display_name],
                row_to_binding,
            )
            .optional()
            .map_err(StoreError::from)
    }

    fn get_by_topic_id(&self, topic_id: i64) -> Result<Option<SessionBinding>, StoreError> {
        let bindings = self.list()?;
        Ok(bindings
            .into_iter()
            .find(|binding| binding.scope.topic_id() == Some(topic_id)))
    }

    fn list(&self) -> Result<Vec<SessionBinding>, StoreError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let mut statement = conn.prepare(
            "SELECT scope_json, session_id, display_name, workspace_root
             FROM session_bindings
             ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map([], row_to_binding)?;
        let mut bindings = Vec::new();
        for row in rows {
            bindings.push(row?);
        }
        Ok(bindings)
    }
}

impl SessionBindingWriter for SqliteBindingStore {
    fn upsert_binding(&self, binding: SessionBinding) {
        self.upsert_internal(binding)
            .expect("sqlite binding upsert should succeed");
    }
}

impl SessionBindingLookup for SqliteBindingStore {
    fn find_by_scope(&self, scope: &ChatScope) -> Option<SessionBinding> {
        self.get_by_scope(scope).ok().flatten()
    }

    fn find_by_session_id(&self, session_id: &str) -> Option<SessionBinding> {
        self.get_by_session_id(session_id).ok().flatten()
    }

    fn find_by_display_name(&self, display_name: &str) -> Option<SessionBinding> {
        self.get_by_display_name(display_name).ok().flatten()
    }

    fn find_by_topic_id(&self, topic_id: i64) -> Option<SessionBinding> {
        self.get_by_topic_id(topic_id).ok().flatten()
    }
}

impl SqliteBindingStore {
    fn upsert_internal(&self, binding: SessionBinding) -> Result<(), StoreError> {
        let scope_key = binding.scope.scope_key();
        let scope_json = serde_json::to_string(&binding.scope)?;
        self.connection
            .lock()
            .expect("sqlite mutex poisoned")
            .execute(
                "INSERT INTO session_bindings (
                    scope_key,
                    scope_json,
                    session_id,
                    display_name,
                    workspace_root,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%s','now'))
                ON CONFLICT(scope_key) DO UPDATE SET
                    scope_json=excluded.scope_json,
                    session_id=excluded.session_id,
                    display_name=excluded.display_name,
                    workspace_root=excluded.workspace_root,
                    updated_at=strftime('%s','now')",
                params![
                    scope_key,
                    scope_json,
                    binding.session.codex_session_id,
                    binding.session.display_name,
                    binding.workspace_root,
                ],
            )?;
        Ok(())
    }
}

fn row_to_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionBinding> {
    let scope_json: String = row.get(0)?;
    let scope: ChatScope = serde_json::from_str(&scope_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let session_id: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let workspace_root: String = row.get(3)?;

    Ok(SessionBinding {
        scope,
        session: SessionHandle {
            codex_session_id: session_id,
            display_name,
        },
        workspace_root,
    })
}

#[cfg(test)]
mod tests {
    use vibes_core::{ChatScope, SessionBinding, SessionBindingWriter, SessionHandle};

    use crate::{SessionBindingStore, SqliteBindingStore};

    #[test]
    fn persists_bindings_between_instances() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path();

        let store = SqliteBindingStore::open(path).unwrap();
        store.upsert_binding(SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -100,
                topic_id: 22,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "codex-a".to_owned(),
            },
            workspace_root: "/tmp/work-a".to_owned(),
        });
        drop(store);

        let reloaded = SqliteBindingStore::open(path).unwrap();
        let binding = reloaded
            .get_by_session_id("sess-1")
            .unwrap()
            .expect("binding should exist");

        assert_eq!(binding.workspace_root, "/tmp/work-a");
        assert_eq!(binding.scope.scope_key(), "topic:-100:22");
    }

    #[test]
    fn resolves_display_name_and_topic_id() {
        let store = SqliteBindingStore::open_in_memory().unwrap();
        store.upsert_binding(SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -100,
                topic_id: 31,
            },
            session: SessionHandle {
                codex_session_id: "sess-31".to_owned(),
                display_name: "codex-31".to_owned(),
            },
            workspace_root: "/tmp/work-31".to_owned(),
        });

        assert_eq!(
            store
                .get_by_display_name("codex-31")
                .unwrap()
                .unwrap()
                .session
                .codex_session_id,
            "sess-31"
        );
        assert_eq!(
            store
                .get_by_topic_id(31)
                .unwrap()
                .unwrap()
                .scope
                .scope_key(),
            "topic:-100:31"
        );
    }
}
