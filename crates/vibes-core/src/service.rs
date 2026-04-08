use thiserror::Error;

use crate::{ChatScope, SessionBinding, SessionHandle};

pub trait SessionBindingLookup {
    fn find_by_scope(&self, scope: &ChatScope) -> Option<SessionBinding>;
    fn find_by_session_id(&self, session_id: &str) -> Option<SessionBinding>;
    fn find_by_display_name(&self, display_name: &str) -> Option<SessionBinding>;
    fn find_by_topic_id(&self, topic_id: i64) -> Option<SessionBinding>;
}

pub trait SessionBindingWriter {
    fn upsert_binding(&self, binding: SessionBinding);
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResumeLookupError {
    #[error("session not found: {0}")]
    NotFound(String),
}

pub struct SessionService<Store> {
    store: Store,
}

impl<Store> SessionService<Store> {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }
}

impl<Store> SessionService<Store>
where
    Store: SessionBindingWriter,
{
    pub fn bind_session(
        &self,
        scope: ChatScope,
        session: SessionHandle,
        workspace_root: impl Into<String>,
    ) -> SessionBinding {
        let binding = SessionBinding {
            scope,
            session,
            workspace_root: workspace_root.into(),
        };
        self.store.upsert_binding(binding.clone());
        binding
    }
}

impl<Store> SessionService<Store>
where
    Store: SessionBindingLookup,
{
    pub fn resolve_resume_target(&self, target: &str) -> Result<SessionBinding, ResumeLookupError> {
        let trimmed = target.trim();
        if let Some(binding) = self.store.find_by_display_name(trimmed) {
            return Ok(binding);
        }
        if let Some(binding) = self.store.find_by_session_id(trimmed) {
            return Ok(binding);
        }
        if let Ok(topic_id) = trimmed.parse::<i64>()
            && let Some(binding) = self.store.find_by_topic_id(topic_id)
        {
            return Ok(binding);
        }

        Err(ResumeLookupError::NotFound(trimmed.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use crate::{
        ChatScope, ResumeLookupError, SessionBinding, SessionBindingLookup, SessionBindingWriter,
        SessionHandle, SessionService,
    };

    #[derive(Default)]
    struct MemoryStore {
        bindings: RwLock<Vec<SessionBinding>>,
    }

    impl SessionBindingWriter for MemoryStore {
        fn upsert_binding(&self, binding: SessionBinding) {
            let mut bindings = self
                .bindings
                .write()
                .expect("memory store write lock poisoned");
            bindings.retain(|current| current.scope != binding.scope);
            bindings.push(binding);
        }
    }

    impl SessionBindingLookup for MemoryStore {
        fn find_by_scope(&self, scope: &ChatScope) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("memory store read lock poisoned")
                .iter()
                .find(|binding| &binding.scope == scope)
                .cloned()
        }

        fn find_by_session_id(&self, session_id: &str) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("memory store read lock poisoned")
                .iter()
                .find(|binding| binding.session.codex_session_id == session_id)
                .cloned()
        }

        fn find_by_display_name(&self, display_name: &str) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("memory store read lock poisoned")
                .iter()
                .find(|binding| binding.session.display_name == display_name)
                .cloned()
        }

        fn find_by_topic_id(&self, topic_id: i64) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("memory store read lock poisoned")
                .iter()
                .find(|binding| matches!(binding.scope, ChatScope::Topic { topic_id: binding_topic_id, .. } if binding_topic_id == topic_id))
                .cloned()
        }
    }

    fn handle(session_id: &str, display_name: &str) -> SessionHandle {
        SessionHandle {
            codex_session_id: session_id.to_owned(),
            display_name: display_name.to_owned(),
        }
    }

    #[test]
    fn binds_session_into_store() {
        let service = SessionService::new(MemoryStore::default());
        let binding = service.bind_session(
            ChatScope::Direct(1668851955),
            handle("019d6361-f755-7992-b08a", "vibes-rust"),
            "/tmp/vibes",
        );

        assert_eq!(
            service
                .store()
                .find_by_scope(&ChatScope::Direct(1668851955)),
            Some(binding)
        );
    }

    #[test]
    fn resolves_resume_by_name_session_or_topic_id() {
        let service = SessionService::new(MemoryStore::default());
        service.bind_session(
            ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            },
            handle("019d6361-f755-7992-b08a", "vibes-rust"),
            "/tmp/vibes",
        );

        assert_eq!(
            service
                .resolve_resume_target("vibes-rust")
                .unwrap()
                .session
                .display_name,
            "vibes-rust"
        );
        assert_eq!(
            service
                .resolve_resume_target("019d6361-f755-7992-b08a")
                .unwrap()
                .session
                .codex_session_id,
            "019d6361-f755-7992-b08a"
        );
        assert_eq!(
            service.resolve_resume_target("114").unwrap().scope,
            ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            }
        );
    }

    #[test]
    fn errors_for_unknown_resume_target() {
        let service = SessionService::new(MemoryStore::default());
        let error = service.resolve_resume_target("missing").unwrap_err();
        assert_eq!(error, ResumeLookupError::NotFound("missing".to_owned()));
    }
}
