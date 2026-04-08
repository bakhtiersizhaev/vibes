use std::collections::HashMap;
use std::sync::RwLock;

use vibes_core::{ChatScope, SessionBinding, SessionBindingLookup, SessionBindingWriter};

use crate::binding_store::{SessionBindingStore, StoreError};

#[derive(Debug, Default)]
pub struct InMemoryBindingStore {
    bindings: RwLock<HashMap<ChatScope, SessionBinding>>,
}

impl SessionBindingWriter for InMemoryBindingStore {
    fn upsert_binding(&self, binding: SessionBinding) {
        self.bindings
            .write()
            .expect("in-memory store write lock poisoned")
            .insert(binding.scope.clone(), binding);
    }
}

impl SessionBindingLookup for InMemoryBindingStore {
    fn find_by_scope(&self, scope: &ChatScope) -> Option<SessionBinding> {
        self.bindings
            .read()
            .expect("in-memory store read lock poisoned")
            .get(scope)
            .cloned()
    }

    fn find_by_session_id(&self, session_id: &str) -> Option<SessionBinding> {
        self.bindings
            .read()
            .expect("in-memory store read lock poisoned")
            .values()
            .find(|binding| binding.session.codex_session_id == session_id)
            .cloned()
    }

    fn find_by_display_name(&self, display_name: &str) -> Option<SessionBinding> {
        self.bindings
            .read()
            .expect("in-memory store read lock poisoned")
            .values()
            .find(|binding| binding.session.display_name == display_name)
            .cloned()
    }

    fn find_by_topic_id(&self, topic_id: i64) -> Option<SessionBinding> {
        self.bindings
            .read()
            .expect("in-memory store read lock poisoned")
            .values()
            .find(|binding| binding.scope.topic_id() == Some(topic_id))
            .cloned()
    }
}

impl SessionBindingStore for InMemoryBindingStore {
    fn get_by_scope(&self, scope: &ChatScope) -> Result<Option<SessionBinding>, StoreError> {
        Ok(self.find_by_scope(scope))
    }

    fn get_by_session_id(&self, session_id: &str) -> Result<Option<SessionBinding>, StoreError> {
        Ok(self.find_by_session_id(session_id))
    }

    fn get_by_display_name(
        &self,
        display_name: &str,
    ) -> Result<Option<SessionBinding>, StoreError> {
        Ok(self.find_by_display_name(display_name))
    }

    fn get_by_topic_id(&self, topic_id: i64) -> Result<Option<SessionBinding>, StoreError> {
        Ok(self.find_by_topic_id(topic_id))
    }

    fn list(&self) -> Result<Vec<SessionBinding>, StoreError> {
        Ok(self
            .bindings
            .read()
            .expect("in-memory store read lock poisoned")
            .values()
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use vibes_core::{ChatScope, SessionBinding, SessionBindingWriter, SessionHandle};

    use crate::{InMemoryBindingStore, SessionBindingStore};

    fn binding(scope: ChatScope, session_id: &str) -> SessionBinding {
        SessionBinding {
            scope,
            session: SessionHandle {
                codex_session_id: session_id.to_owned(),
                display_name: "codex-rust".to_owned(),
            },
            workspace_root: "/tmp/project".to_owned(),
        }
    }

    #[test]
    fn replaces_binding_for_same_scope() {
        let scope = ChatScope::Group(-100);
        let store = InMemoryBindingStore::default();
        store.upsert_binding(binding(scope.clone(), "first"));
        store.upsert_binding(binding(scope.clone(), "second"));

        let loaded = store.get_by_scope(&scope).unwrap().unwrap();
        assert_eq!(loaded.session.codex_session_id, "second");
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn resolves_binding_by_session_id() {
        let store = InMemoryBindingStore::default();
        store.upsert_binding(binding(
            ChatScope::Topic {
                chat_id: -100,
                topic_id: 7,
            },
            "019d6361-f755-7992-b08a",
        ));

        let loaded = store
            .get_by_session_id("019d6361-f755-7992-b08a")
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded.scope,
            ChatScope::Topic {
                chat_id: -100,
                topic_id: 7,
            }
        );
    }

    #[test]
    fn resolves_binding_by_display_name_and_topic_id() {
        let store = InMemoryBindingStore::default();
        store.upsert_binding(binding(
            ChatScope::Topic {
                chat_id: -100,
                topic_id: 11,
            },
            "session-11",
        ));

        let named = store.get_by_display_name("codex-rust").unwrap().unwrap();
        let topic = store.get_by_topic_id(11).unwrap().unwrap();

        assert_eq!(named.session.codex_session_id, "session-11");
        assert_eq!(topic.session.codex_session_id, "session-11");
    }
}
