use thiserror::Error;
use vibes_core::{
    ChatScope, SessionBinding, SessionBindingLookup, SessionCommand, SessionCommandInput,
    parse_user_input,
};
use vibes_telegram::{TelegramEnvelope, resolve_scope};

use crate::service::StartNewSessionInput;
use vibes_core::{ResumeSessionRequest, StartSessionRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteMessageInput<'a> {
    pub envelope: &'a TelegramEnvelope,
    pub text: &'a str,
    pub bot_username: Option<&'a str>,
    pub default_workspace_root: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRoute {
    StartNew(StartNewSessionInput),
    Resume(ResumeSessionRequest),
    ContinueBound {
        binding: SessionBinding,
        prompt: String,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MessageRouteError {
    #[error(transparent)]
    Command(#[from] vibes_core::CommandParseError),
    #[error("no bound session for scope {0}")]
    MissingBinding(String),
}

pub fn route_message<Store>(
    store: &Store,
    input: RouteMessageInput<'_>,
) -> Result<MessageRoute, MessageRouteError>
where
    Store: SessionBindingLookup,
{
    let scope = resolve_scope(input.envelope);
    match parse_user_input(input.text, input.bot_username)? {
        SessionCommandInput::Command(SessionCommand::New { label }) => {
            Ok(MessageRoute::StartNew(StartNewSessionInput {
                request: StartSessionRequest {
                    scope,
                    label,
                    workspace_root: input.default_workspace_root.to_owned(),
                },
                create_topic_for_group: should_create_topic(input.envelope),
            }))
        }
        SessionCommandInput::Command(SessionCommand::Resume { target }) => {
            Ok(MessageRoute::Resume(ResumeSessionRequest {
                scope,
                target,
                workspace_root: String::new(),
            }))
        }
        SessionCommandInput::Prompt(prompt) => {
            let binding = store
                .find_by_scope(&scope)
                .ok_or_else(|| MessageRouteError::MissingBinding(scope.scope_key()))?;
            Ok(MessageRoute::ContinueBound { binding, prompt })
        }
        SessionCommandInput::Command(SessionCommand::Status)
        | SessionCommandInput::Command(SessionCommand::Sessions)
        | SessionCommandInput::Command(SessionCommand::Help) => {
            Err(MessageRouteError::MissingBinding(help_scope(&scope)))
        }
    }
}

fn should_create_topic(envelope: &TelegramEnvelope) -> bool {
    envelope.is_forum && matches!(resolve_scope(envelope), ChatScope::Group(_))
}

fn help_scope(scope: &ChatScope) -> String {
    format!("command-not-yet-routed:{}", scope.scope_key())
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use vibes_core::{
        ChatScope, SessionBinding, SessionBindingLookup, SessionBindingWriter, SessionHandle,
    };
    use vibes_telegram::{TelegramChatKind, TelegramEnvelope};

    use crate::{MessageRoute, MessageRouteError, RouteMessageInput, route_message};

    #[derive(Default)]
    struct MemoryStore {
        bindings: RwLock<Vec<SessionBinding>>,
    }

    impl SessionBindingWriter for MemoryStore {
        fn upsert_binding(&self, binding: SessionBinding) {
            let mut bindings = self.bindings.write().expect("router test lock poisoned");
            bindings.retain(|current| current.scope != binding.scope);
            bindings.push(binding);
        }
    }

    impl SessionBindingLookup for MemoryStore {
        fn find_by_scope(&self, scope: &ChatScope) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("router test lock poisoned")
                .iter()
                .find(|binding| &binding.scope == scope)
                .cloned()
        }

        fn find_by_session_id(&self, session_id: &str) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("router test lock poisoned")
                .iter()
                .find(|binding| binding.session.codex_session_id == session_id)
                .cloned()
        }

        fn find_by_display_name(&self, display_name: &str) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("router test lock poisoned")
                .iter()
                .find(|binding| binding.session.display_name == display_name)
                .cloned()
        }

        fn find_by_topic_id(&self, topic_id: i64) -> Option<SessionBinding> {
            self.bindings
                .read()
                .expect("router test lock poisoned")
                .iter()
                .find(|binding| binding.scope.topic_id() == Some(topic_id))
                .cloned()
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
    fn routes_new_session_from_general_forum_chat() {
        let store = MemoryStore::default();
        let route = route_message(
            &store,
            RouteMessageInput {
                envelope: &general_forum(),
                text: "/new rust-rewrite",
                bot_username: Some("PillPhant_bot"),
                default_workspace_root: "/workspace",
            },
        )
        .unwrap();

        let MessageRoute::StartNew(start) = route else {
            panic!("expected new-session route");
        };
        assert!(start.create_topic_for_group);
        assert_eq!(start.request.scope, ChatScope::Group(-1003562096175));
        assert_eq!(start.request.label.as_deref(), Some("rust-rewrite"));
    }

    #[test]
    fn routes_resume_inside_topic_scope() {
        let store = MemoryStore::default();
        let envelope = TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: true,
            message_thread_id: Some(114),
        };
        let route = route_message(
            &store,
            RouteMessageInput {
                envelope: &envelope,
                text: "/resume sess-123",
                bot_username: None,
                default_workspace_root: "/workspace",
            },
        )
        .unwrap();

        let MessageRoute::Resume(resume) = route else {
            panic!("expected resume route");
        };
        assert_eq!(
            resume.scope,
            ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            }
        );
        assert_eq!(resume.target, "sess-123");
    }

    #[test]
    fn routes_plain_prompt_to_existing_binding() {
        let store = MemoryStore::default();
        store.upsert_binding(SessionBinding {
            scope: ChatScope::Direct(1668851955),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "vibes-rust".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        });

        let route = route_message(
            &store,
            RouteMessageInput {
                envelope: &TelegramEnvelope {
                    chat_id: 1668851955,
                    chat_kind: TelegramChatKind::Direct,
                    is_forum: false,
                    message_thread_id: None,
                },
                text: "continue building telegram adapter",
                bot_username: None,
                default_workspace_root: "/workspace",
            },
        )
        .unwrap();

        let MessageRoute::ContinueBound { binding, prompt } = route else {
            panic!("expected bound prompt route");
        };
        assert_eq!(binding.session.codex_session_id, "sess-1");
        assert_eq!(prompt, "continue building telegram adapter");
    }

    #[test]
    fn rejects_unbound_plain_prompt() {
        let store = MemoryStore::default();
        let error = route_message(
            &store,
            RouteMessageInput {
                envelope: &TelegramEnvelope {
                    chat_id: 1668851955,
                    chat_kind: TelegramChatKind::Direct,
                    is_forum: false,
                    message_thread_id: None,
                },
                text: "hello codex",
                bot_username: None,
                default_workspace_root: "/workspace",
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            MessageRouteError::MissingBinding("dm:1668851955".to_owned())
        );
    }
}
