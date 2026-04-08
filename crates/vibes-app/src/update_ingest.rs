use vibes_core::SessionBinding;
use vibes_store::SessionBindingStore;
use vibes_telegram::{ReplyTarget, extract_text_message, reply_target};

use crate::{
    AppAction, AppController, HandleMessageError, RouteMessageInput, SessionRuntime, TopicManager,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramUpdateAction {
    Reply {
        target: ReplyTarget,
        text: String,
    },
    DispatchPrompt {
        target: ReplyTarget,
        binding: SessionBinding,
        prompt: String,
    },
}

pub fn handle_telegram_update<S, R, T>(
    controller: &AppController<S, R, T>,
    update: &teloxide::types::Update,
    bot_username: Option<&str>,
    default_workspace_root: &str,
) -> Result<Option<TelegramUpdateAction>, HandleMessageError>
where
    S: SessionBindingStore,
    R: SessionRuntime,
    T: TopicManager,
{
    let incoming = match extract_text_message(update) {
        Some(incoming) => incoming,
        None => return Ok(None),
    };
    let target = reply_target(&incoming.envelope);
    let action = controller.handle_message(RouteMessageInput {
        envelope: &incoming.envelope,
        text: &incoming.text,
        bot_username,
        default_workspace_root,
    })?;

    Ok(Some(match action {
        AppAction::Reply(text) => TelegramUpdateAction::Reply { target, text },
        AppAction::DispatchPrompt { binding, prompt } => TelegramUpdateAction::DispatchPrompt {
            target,
            binding,
            prompt,
        },
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use teloxide::types::Update;
    use vibes_core::{SessionBindingWriter, SessionHandle};
    use vibes_store::InMemoryBindingStore;

    use crate::{
        AppController, AppService, AppServiceError, SessionRuntime, TelegramUpdateAction,
        TopicManager, handle_telegram_update,
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
                codex_session_id: "019d6361-f755-7992-b08a".to_owned(),
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
                .expect("ingest topic lock poisoned")
                .push((chat_id, title.to_owned()));
            Ok(501)
        }
    }

    fn controller() -> AppController<InMemoryBindingStore, FakeRuntime, FakeTopics> {
        AppController::new(AppService::new(
            InMemoryBindingStore::default(),
            FakeRuntime,
            FakeTopics::default(),
        ))
    }

    fn parse_update(text: &str) -> Update {
        serde_json::from_str(text).expect("valid telegram update")
    }

    #[test]
    fn turns_forum_new_command_into_reply_action() {
        let controller = controller();
        let update = parse_update(
            r#"{
                "message": {
                    "chat": {
                        "id": -1001293752024,
                        "title": "CryptoInside Chat",
                        "type": "supergroup",
                        "username": "cryptoinside_talk",
                        "is_forum": true
                    },
                    "date": 1721592580,
                    "entities": [
                        {
                            "length": 4,
                            "offset": 0,
                            "type": "bot_command"
                        }
                    ],
                    "from": {
                        "first_name": "the Cable Guy",
                        "id": 5964236329,
                        "is_bot": false,
                        "language_code":"en",
                        "username": "spacewhaleblues"
                    },
                    "message_id": 134546,
                    "text": "/new rust-rewrite"
                },
                "update_id": 439432600
            }"#,
        );

        let action = handle_telegram_update(&controller, &update, None, "/workspace")
            .expect("ingest ok")
            .expect("message action");

        let TelegramUpdateAction::Reply { target, text } = action else {
            panic!("expected reply action");
        };
        assert_eq!(target.chat_id, -1001293752024);
        assert_eq!(target.message_thread_id, None);
        assert!(text.contains("Created topic"));
        assert!(text.contains("019d6361"));
    }

    #[test]
    fn turns_bound_dm_prompt_into_dispatch_action() {
        let store = InMemoryBindingStore::default();
        store.upsert_binding(vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        });
        let controller =
            AppController::new(AppService::new(store, FakeRuntime, FakeTopics::default()));
        let update = parse_update(
            r#"
            {
              "message": {
                "chat": {
                  "first_name": "Hirrolot",
                  "id": 408258968,
                  "type": "private",
                  "username": "hirrolot"
                },
                "date": 1581448857,
                "from": {
                  "first_name": "Hirrolot",
                  "id": 408258968,
                  "is_bot": false,
                  "language_code": "en",
                  "username": "hirrolot"
                },
                "message_id": 154,
                "text": "continue parser work"
              },
              "update_id": 306197398
            }
            "#,
        );

        let action = handle_telegram_update(&controller, &update, None, "/workspace")
            .expect("ingest ok")
            .expect("message action");

        let TelegramUpdateAction::DispatchPrompt {
            target,
            binding,
            prompt,
        } = action
        else {
            panic!("expected dispatch action");
        };
        assert_eq!(target.chat_id, 408258968);
        assert_eq!(target.message_thread_id, None);
        assert_eq!(binding.session.codex_session_id, "sess-1");
        assert_eq!(prompt, "continue parser work");
    }

    #[test]
    fn ignores_non_message_updates() {
        let controller = controller();
        let update = parse_update(
            r#"{
                "update_id": 3,
                "callback_query": {
                    "id": "abc",
                    "from": {
                        "id": 1668851955,
                        "is_bot": false,
                        "first_name": "Baha"
                    },
                    "chat_instance": "inst",
                    "data": "noop"
                }
            }"#,
        );

        let action =
            handle_telegram_update(&controller, &update, None, "/workspace").expect("ingest ok");
        assert!(action.is_none());
    }
}
