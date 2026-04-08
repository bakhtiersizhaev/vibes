#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use vibes_app::{
        AppAction, AppController, AppService, AppServiceError, RouteMessageInput, SessionRuntime,
        TopicManager,
    };
    use vibes_core::SessionHandle;
    use vibes_store::InMemoryBindingStore;
    use vibes_telegram::{TelegramChatKind, TelegramEnvelope};

    #[derive(Debug, Default)]
    struct FakeRuntime {
        new_calls: Mutex<usize>,
    }

    impl SessionRuntime for FakeRuntime {
        fn new_session(
            &self,
            label: Option<&str>,
            _workspace_root: &str,
        ) -> Result<SessionHandle, AppServiceError> {
            let mut calls = self.new_calls.lock().expect("runtime lock poisoned");
            let session_id = match *calls {
                0 => "019d6361-f755-7992-b08a".to_owned(),
                _ => format!("019d636{}-f755-7992-b08a", calls),
            };
            *calls += 1;
            Ok(SessionHandle {
                codex_session_id: session_id,
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
                display_name: format!("resumed-{target}"),
            })
        }
    }

    #[derive(Debug, Default)]
    struct FakeTopics;

    impl TopicManager for FakeTopics {
        fn create_topic(&self, _chat_id: i64, _title: &str) -> Result<i64, AppServiceError> {
            Ok(501)
        }
    }

    fn controller() -> AppController<InMemoryBindingStore, FakeRuntime, FakeTopics> {
        AppController::new(AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime::default(),
            FakeTopics,
        ))
    }

    fn general_forum() -> TelegramEnvelope {
        TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: true,
            message_thread_id: None,
        }
    }

    fn topic_envelope(topic_id: i64) -> TelegramEnvelope {
        TelegramEnvelope {
            chat_id: -1003562096175,
            chat_kind: TelegramChatKind::Supergroup,
            is_forum: true,
            message_thread_id: Some(topic_id),
        }
    }

    #[test]
    fn new_session_binds_created_topic_and_routes_follow_up_prompt() {
        let controller = controller();

        let created = controller
            .handle_message(RouteMessageInput {
                envelope: &general_forum(),
                text: "/new rust-rewrite",
                bot_username: Some("vibes_bot"),
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::Reply(reply) = created else {
            panic!("expected reply after /new");
        };
        assert!(reply.contains("Created topic"));
        assert!(reply.contains("019d6361"));

        let dispatched = controller
            .handle_message(RouteMessageInput {
                envelope: &topic_envelope(501),
                text: "ship the next rust step",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::DispatchPrompt { binding, prompt } = dispatched else {
            panic!("expected prompt dispatch in created topic");
        };
        assert_eq!(binding.session.codex_session_id, "019d6361-f755-7992-b08a");
        assert_eq!(binding.scope.scope_key(), "topic:-1003562096175:501");
        assert_eq!(prompt, "ship the next rust step");
    }

    #[test]
    fn resume_by_display_name_rebinds_topic_and_routes_prompt() {
        let controller = controller();

        controller
            .handle_message(RouteMessageInput {
                envelope: &TelegramEnvelope {
                    chat_id: 1668851955,
                    chat_kind: TelegramChatKind::Direct,
                    is_forum: false,
                    message_thread_id: None,
                },
                text: "/new rust-rewrite",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let resumed = controller
            .handle_message(RouteMessageInput {
                envelope: &topic_envelope(900),
                text: "/resume rust-rewrite",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::Reply(reply) = resumed else {
            panic!("expected resume reply");
        };
        assert!(reply.contains("019d6361"));

        let dispatched = controller
            .handle_message(RouteMessageInput {
                envelope: &topic_envelope(900),
                text: "continue from resumed topic",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::DispatchPrompt { binding, prompt } = dispatched else {
            panic!("expected dispatch after resume");
        };
        assert_eq!(binding.session.codex_session_id, "019d6361-f755-7992-b08a");
        assert_eq!(binding.scope.scope_key(), "topic:-1003562096175:900");
        assert_eq!(prompt, "continue from resumed topic");
    }

    #[test]
    fn unbound_topic_prompt_returns_helpful_reply() {
        let controller = controller();

        let action = controller
            .handle_message(RouteMessageInput {
                envelope: &topic_envelope(42),
                text: "just run it",
                bot_username: None,
                default_workspace_root: "/workspace",
            })
            .unwrap();

        let AppAction::Reply(reply) = action else {
            panic!("expected help reply");
        };
        assert!(reply.contains("/new"));
        assert!(reply.contains("/resume"));
    }
}
