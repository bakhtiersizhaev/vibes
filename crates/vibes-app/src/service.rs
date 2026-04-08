use std::path::Path;

use thiserror::Error;
use vibes_codex::CodexExecRunner;
use vibes_core::{
    ChatScope, ResumeSessionRequest, SessionBinding, SessionHandle, StartSessionRequest,
};
use vibes_store::{SessionBindingStore, StoreError};
use vibes_telegram::build_topic_title;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartNewSessionInput {
    pub request: StartSessionRequest,
    pub create_topic_for_group: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindOutcome {
    pub binding: SessionBinding,
    pub created_topic_id: Option<i64>,
    pub created_topic_title: Option<String>,
}

pub trait SessionRuntime: Send + Sync {
    fn new_session(
        &self,
        label: Option<&str>,
        workspace_root: &str,
    ) -> Result<SessionHandle, AppServiceError>;
    fn resume_session(
        &self,
        target: &str,
        workspace_root: &str,
    ) -> Result<SessionHandle, AppServiceError>;
}

pub trait TopicManager: Send + Sync {
    fn create_topic(&self, chat_id: i64, title: &str) -> Result<i64, AppServiceError>;
}

impl SessionRuntime for CodexExecRunner {
    fn new_session(
        &self,
        label: Option<&str>,
        workspace_root: &str,
    ) -> Result<SessionHandle, AppServiceError> {
        self.start_new(label, Path::new(workspace_root))
            .map_err(|err| AppServiceError::Runtime(err.to_string()))
    }

    fn resume_session(
        &self,
        target: &str,
        workspace_root: &str,
    ) -> Result<SessionHandle, AppServiceError> {
        self.resume(target, Path::new(workspace_root))
            .map_err(|err| AppServiceError::Runtime(err.to_string()))
    }
}

#[derive(Debug)]
pub struct AppService<S, R, T> {
    store: S,
    runtime: R,
    topics: T,
}

#[derive(Debug, Error)]
pub enum AppServiceError {
    #[error("binding store error: {0}")]
    Store(#[from] StoreError),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("topic manager error: {0}")]
    Topic(String),
}

impl<S, R, T> AppService<S, R, T>
where
    S: SessionBindingStore,
    R: SessionRuntime,
    T: TopicManager,
{
    pub fn new(store: S, runtime: R, topics: T) -> Self {
        Self {
            store,
            runtime,
            topics,
        }
    }

    pub fn start_new(&self, input: StartNewSessionInput) -> Result<BindOutcome, AppServiceError> {
        let StartNewSessionInput {
            request,
            create_topic_for_group,
        } = input;

        let session = self
            .runtime
            .new_session(request.label.as_deref(), &request.workspace_root)?;
        let mut scope = request.scope;
        let mut topic_id = None;
        let mut topic_title = None;

        if create_topic_for_group && let ChatScope::Group(chat_id) = scope {
            let title = build_topic_title(request.label.as_deref(), &session.codex_session_id);
            let new_topic_id = self.topics.create_topic(chat_id, &title)?;
            topic_id = Some(new_topic_id);
            topic_title = Some(title);
            scope = ChatScope::Topic {
                chat_id,
                topic_id: new_topic_id,
            };
        }

        let binding = SessionBinding {
            scope,
            session,
            workspace_root: request.workspace_root,
        };
        self.store.upsert_binding(binding.clone());

        Ok(BindOutcome {
            binding,
            created_topic_id: topic_id,
            created_topic_title: topic_title,
        })
    }

    pub fn resume(&self, request: ResumeSessionRequest) -> Result<BindOutcome, AppServiceError> {
        let session = self
            .runtime
            .resume_session(&request.target, &request.workspace_root)?;
        let existing = self.store.get_by_session_id(&session.codex_session_id)?;
        let workspace_root = if request.workspace_root.trim().is_empty() {
            existing
                .as_ref()
                .map(|binding| binding.workspace_root.clone())
                .unwrap_or_default()
        } else {
            request.workspace_root
        };

        let binding = SessionBinding {
            scope: request.scope,
            session,
            workspace_root,
        };
        self.store.upsert_binding(binding.clone());

        Ok(BindOutcome {
            binding,
            created_topic_id: None,
            created_topic_title: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use vibes_core::{ChatScope, ResumeSessionRequest, SessionHandle, StartSessionRequest};
    use vibes_store::InMemoryBindingStore;

    use crate::service::{
        AppService, AppServiceError, SessionRuntime, StartNewSessionInput, TopicManager,
    };

    #[derive(Debug, Default)]
    struct FakeRuntime;

    impl SessionRuntime for FakeRuntime {
        fn new_session(
            &self,
            label: Option<&str>,
            _workspace_root: &str,
        ) -> Result<SessionHandle, AppServiceError> {
            Ok(SessionHandle {
                codex_session_id: "sess-new-1".to_owned(),
                display_name: label.unwrap_or("codex-session").to_owned(),
            })
        }

        fn resume_session(
            &self,
            target: &str,
            _workspace_root: &str,
        ) -> Result<SessionHandle, AppServiceError> {
            Ok(SessionHandle {
                codex_session_id: target.to_owned(),
                display_name: "resumed".to_owned(),
            })
        }
    }

    #[derive(Debug, Default)]
    struct FakeTopics {
        calls: Mutex<Vec<(i64, String)>>,
    }

    impl TopicManager for FakeTopics {
        fn create_topic(&self, chat_id: i64, title: &str) -> Result<i64, AppServiceError> {
            self.calls
                .lock()
                .expect("topic test mutex poisoned")
                .push((chat_id, title.to_owned()));
            Ok(777)
        }
    }

    #[test]
    fn new_creates_topic_and_persists_topic_binding() {
        let service = AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime,
            FakeTopics::default(),
        );

        let outcome = service
            .start_new(StartNewSessionInput {
                request: StartSessionRequest {
                    scope: ChatScope::Group(-100),
                    label: Some("rust-rewrite".to_owned()),
                    workspace_root: "/tmp/work".to_owned(),
                },
                create_topic_for_group: true,
            })
            .unwrap();

        assert_eq!(outcome.created_topic_id, Some(777));
        assert_eq!(outcome.binding.scope.scope_key(), "topic:-100:777");
        assert_eq!(outcome.binding.session.display_name, "rust-rewrite");
    }

    #[test]
    fn resume_rebinds_scope_for_existing_session() {
        let service = AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime,
            FakeTopics::default(),
        );

        service
            .start_new(StartNewSessionInput {
                request: StartSessionRequest {
                    scope: ChatScope::Direct(1),
                    label: None,
                    workspace_root: "/tmp/original".to_owned(),
                },
                create_topic_for_group: false,
            })
            .unwrap();

        let rebound = service
            .resume(ResumeSessionRequest {
                scope: ChatScope::Topic {
                    chat_id: -100,
                    topic_id: 99,
                },
                target: "sess-new-1".to_owned(),
                workspace_root: "".to_owned(),
            })
            .unwrap();

        assert_eq!(rebound.binding.scope.scope_key(), "topic:-100:99");
        assert_eq!(rebound.binding.workspace_root, "/tmp/original");
    }
}
