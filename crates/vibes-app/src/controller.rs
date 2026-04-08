use vibes_core::SessionBinding;
use vibes_store::SessionBindingStore;

use crate::presenter::{render_missing_binding_hint, render_resume_success, render_start_success};
use crate::router::{MessageRoute, MessageRouteError, RouteMessageInput, route_message};
use crate::service::{AppService, AppServiceError, SessionRuntime, TopicManager};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    Reply(String),
    DispatchPrompt {
        binding: SessionBinding,
        prompt: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum HandleMessageError {
    #[error(transparent)]
    Route(#[from] MessageRouteError),
    #[error(transparent)]
    Service(#[from] AppServiceError),
}

#[derive(Debug)]
pub struct AppController<S, R, T> {
    service: AppService<S, R, T>,
}

impl<S, R, T> AppController<S, R, T>
where
    S: SessionBindingStore,
    R: SessionRuntime,
    T: TopicManager,
{
    pub fn new(service: AppService<S, R, T>) -> Self {
        Self { service }
    }

    pub fn handle_message(
        &self,
        input: RouteMessageInput<'_>,
    ) -> Result<AppAction, HandleMessageError> {
        match route_message(self.service.store(), input) {
            Ok(MessageRoute::StartNew(start)) => {
                let outcome = self.service.start_new(start)?;
                Ok(AppAction::Reply(render_start_success(&outcome)))
            }
            Ok(MessageRoute::Resume(resume)) => {
                let outcome = self.service.resume(resume)?;
                Ok(AppAction::Reply(render_resume_success(&outcome)))
            }
            Ok(MessageRoute::ContinueBound { binding, prompt }) => {
                Ok(AppAction::DispatchPrompt { binding, prompt })
            }
            Err(MessageRouteError::MissingBinding(scope_key)) => {
                Ok(AppAction::Reply(render_missing_binding_hint(&scope_key)))
            }
            Err(error) => Err(HandleMessageError::Route(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use vibes_core::{ChatScope, SessionBindingWriter, SessionHandle};
    use vibes_store::InMemoryBindingStore;
    use vibes_telegram::{TelegramChatKind, TelegramEnvelope};

    use crate::{
        AppAction, AppController, AppService, AppServiceError, RouteMessageInput, SessionRuntime,
        TopicManager,
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
                .expect("controller topic lock poisoned")
                .push((chat_id, title.to_owned()));
            Ok(777)
        }
    }

    fn general_forum() -> TelegramEnvelope {
        TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: true,
            message_thread_id: None,
        }
    }

    #[test]
    fn controller_replies_after_new_session_creation() {
        let service = AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime,
            FakeTopics::default(),
        );
        let controller = AppController::new(service);

        let action = controller
            .handle_message(RouteMessageInput {
                envelope: &general_forum(),
                text: "/new rust-rewrite",
                bot_username: Some("PillPhant_bot"),
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::Reply(text) = action else {
            panic!("expected reply action");
        };
        assert!(text.contains("Created topic"));
        assert!(text.contains("sess"));
    }

    #[test]
    fn controller_dispatches_prompt_for_bound_scope() {
        let store = InMemoryBindingStore::default();
        store.upsert_binding(vibes_core::SessionBinding {
            scope: ChatScope::Direct(1668851955),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        });
        let service = AppService::new(store, FakeRuntime, FakeTopics::default());
        let controller = AppController::new(service);

        let action = controller
            .handle_message(RouteMessageInput {
                envelope: &TelegramEnvelope {
                    chat_id: 1668851955,
                    chat_kind: TelegramChatKind::Direct,
                    is_forum: false,
                    message_thread_id: None,
                },
                text: "continue parser work",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::DispatchPrompt { binding, prompt } = action else {
            panic!("expected dispatch action");
        };
        assert_eq!(binding.session.codex_session_id, "sess-1");
        assert_eq!(prompt, "continue parser work");
    }

    #[test]
    fn controller_turns_missing_binding_into_helpful_reply() {
        let service = AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime,
            FakeTopics::default(),
        );
        let controller = AppController::new(service);

        let action = controller
            .handle_message(RouteMessageInput {
                envelope: &TelegramEnvelope {
                    chat_id: 1668851955,
                    chat_kind: TelegramChatKind::Direct,
                    is_forum: false,
                    message_thread_id: None,
                },
                text: "hello codex",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::Reply(text) = action else {
            panic!("expected reply action");
        };
        assert!(text.contains("/new"));
        assert!(text.contains("/resume"));
    }
}
