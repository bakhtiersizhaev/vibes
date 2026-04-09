use std::sync::Mutex;

use async_trait::async_trait;
use teloxide::types::Update;
use vibes_app::{
    AppController, AppService, AppServiceError, RuntimeOutcome, SessionRuntime,
    TelegramExecutionError, TelegramPromptExecutor, TelegramRequestError, TelegramRequester,
    TopicManager, complete_runtime_outcome, run_telegram_update,
};
use vibes_core::{SessionBindingWriter, SessionHandle};
use vibes_store::InMemoryBindingStore;
use vibes_telegram::ReplyTarget;

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
struct FakeTopics;

impl TopicManager for FakeTopics {
    fn create_topic(&self, _chat_id: i64, _title: &str) -> Result<i64, AppServiceError> {
        Ok(501)
    }
}

#[derive(Debug, Default)]
struct FakeRequester {
    sent: Mutex<Vec<(i64, Option<i64>, String)>>,
    fail: Mutex<Option<String>>,
}

#[derive(Debug)]
struct FakeExecutor {
    response: Mutex<Result<String, String>>,
}

#[async_trait(?Send)]
impl TelegramRequester for FakeRequester {
    async fn send_text(
        &self,
        target: &ReplyTarget,
        text: &str,
    ) -> Result<(), TelegramRequestError> {
        if let Some(message) = self
            .fail
            .lock()
            .expect("fake requester fail lock poisoned")
            .clone()
        {
            return Err(TelegramRequestError::new(message));
        }
        self.sent
            .lock()
            .expect("fake requester lock poisoned")
            .push((target.chat_id, target.message_thread_id, text.to_owned()));
        Ok(())
    }
}

impl TelegramPromptExecutor for FakeExecutor {
    fn execute_prompt(
        &self,
        _binding: &vibes_core::SessionBinding,
        _prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        self.response
            .lock()
            .expect("fake executor lock poisoned")
            .clone()
            .map_err(TelegramExecutionError::new)
    }
}

fn parse_update(text: &str) -> Update {
    serde_json::from_str(text).expect("valid update")
}

fn controller() -> AppController<InMemoryBindingStore, FakeRuntime, FakeTopics> {
    AppController::new(AppService::new(
        InMemoryBindingStore::default(),
        FakeRuntime,
        FakeTopics,
    ))
}

fn run_ready<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[test]
fn returns_request_error_when_new_session_reply_send_fails() {
    let controller = controller();
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("send boom".to_owned())),
    };
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
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134546,
                "text": "/new rust-rewrite"
            },
            "update_id": 439432600
        }"#,
    );

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn sends_reply_for_new_session_update_in_direct_chat() {
    let controller = controller();
    let requester = FakeRequester::default();
    let update = parse_update(
        r#"{
            "message": {
                "chat": {
                    "id": 408258968,
                    "first_name": "Baha",
                    "type": "private",
                    "username": "spacewhaleblues"
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
                    "first_name": "Baha",
                    "id": 408258968,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134545,
                "text": "/new rust-rewrite"
            },
            "update_id": 439432599
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::Replied { target, text } = outcome else {
        panic!("expected reply outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert!(text.contains("rust-rewrite") || text.contains("019d6361"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert!(sent[0].2.contains("rust-rewrite") || sent[0].2.contains("019d6361"));
}

#[test]
fn returns_request_error_when_direct_new_session_reply_send_fails() {
    let controller = controller();
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("send boom".to_owned())),
    };
    let update = parse_update(
        r#"{
            "message": {
                "chat": {
                    "id": 408258968,
                    "first_name": "Baha",
                    "type": "private",
                    "username": "spacewhaleblues"
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
                    "first_name": "Baha",
                    "id": 408258968,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134546,
                "text": "/new rust-rewrite"
            },
            "update_id": 439432600
        }"#,
    );

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn sends_reply_for_new_session_update() {
    let controller = controller();
    let requester = FakeRequester::default();
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

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::Replied { target, text } = outcome else {
        panic!("expected reply outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, None);
    assert!(text.contains("Created topic"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert!(sent[0].2.contains("Created topic"));
}

#[test]
fn returns_prompt_ready_without_sending_message() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
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

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady {
        target,
        binding,
        prompt,
    } = outcome
    else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(binding.session.codex_session_id, "sess-1");
    assert_eq!(prompt, "continue parser work");
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn prompt_executes_and_replies_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("direct transcript".to_owned())),
    };
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
            "message_id": 155,
            "text": "continue parser work"
          },
          "update_id": 306197399
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady { target, prompt, .. } = &outcome else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert_eq!(prompt, "continue parser work");

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert_eq!(text, "direct transcript");
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(
        sent.as_slice(),
        &[(408258968, None, "direct transcript".to_owned())]
    );
}

#[test]
fn returns_prompt_ready_from_caption_without_text() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
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
            "message_id": 155,
            "caption": "continue parser from caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197399
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady {
        target,
        binding,
        prompt,
    } = outcome
    else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(binding.session.codex_session_id, "sess-1");
    assert_eq!(prompt, "continue parser from caption");
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn caption_prompt_executes_and_replies_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption direct transcript".to_owned())),
    };
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
            "message_id": 157,
            "caption": "continue parser from caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197401
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady { target, prompt, .. } = &outcome else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert_eq!(prompt, "continue parser from caption");

    let completion =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("completion ok");

    let RuntimeOutcome::Replied { target, text } = completion else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert_eq!(text, "caption direct transcript");

    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert_eq!(sent[0].2, "caption direct transcript");
}

#[test]
fn caption_prompt_execution_failure_replies_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
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
            "message_id": 158,
            "caption": "continue parser from caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197402
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let completion =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("completion ok");

    let RuntimeOutcome::Replied { target, text } = completion else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));

    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn caption_prompt_result_propagates_request_send_failure_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption direct transcript".to_owned())),
    };
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
            "message_id": 159,
            "caption": "continue parser from caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197405
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn caption_prompt_execution_failure_propagates_request_send_failure_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
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
            "message_id": 160,
            "caption": "continue parser from caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197406
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn topic_caption_prompt_result_falls_back_to_chat_reply_when_thread_send_fails() {
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption topic transcript after fallback".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser from topic caption".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "caption topic transcript after fallback");

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, None);
    assert_eq!(sent[0].2, "caption topic transcript after fallback");
}

#[test]
fn topic_caption_prompt_result_returns_request_error_when_thread_fallback_also_fails() {
    let requester = ThreadThenChatFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption topic transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser from topic caption".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("thread then chat fallback should fail");
    assert!(
        err.to_string()
            .contains("telegram request failed: chat send boom")
    );
}

#[test]
fn topic_caption_prompt_result_propagates_request_send_failure() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption topic transcript".to_owned())),
    };
    let update = parse_update(
        r#"
        {
          "message": {
            "chat": {
              "id": -1001293752024,
              "title": "CryptoInside Chat",
              "type": "supergroup",
              "username": "cryptoinside_talk",
              "is_forum": true
            },
            "date": 1721592580,
            "from": {
              "first_name": "the Cable Guy",
              "id": 5964236329,
              "is_bot": false,
              "language_code": "en",
              "username": "tg"
            },
            "message_id": 3129,
            "message_thread_id": 900,
            "caption": "continue parser from topic caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197407
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn topic_caption_execution_failure_propagates_request_send_failure() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let update = parse_update(
        r#"
        {
          "message": {
            "chat": {
              "id": -1001293752024,
              "title": "CryptoInside Chat",
              "type": "supergroup",
              "username": "cryptoinside_talk",
              "is_forum": true
            },
            "date": 1721592580,
            "from": {
              "first_name": "the Cable Guy",
              "id": 5964236329,
              "is_bot": false,
              "language_code": "en",
              "username": "tg"
            },
            "message_id": 3130,
            "message_thread_id": 900,
            "caption": "continue parser from topic caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197408
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn trims_caption_before_routing_prompt() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
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
            "message_id": 156,
            "caption": "   continue parser from caption   ",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197400
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady { prompt, .. } = outcome else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(prompt, "continue parser from caption");
}

#[test]
fn returns_topic_prompt_ready_from_caption_without_text() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let update = parse_update(
        r#"
        {
          "message": {
            "chat": {
              "id": -1001293752024,
              "title": "CryptoInside Chat",
              "type": "supergroup",
              "username": "cryptoinside_talk",
              "is_forum": true
            },
            "date": 1721592580,
            "from": {
              "first_name": "the Cable Guy",
              "id": 5964236329,
              "is_bot": false,
              "language_code": "en",
              "username": "tg"
            },
            "message_id": 3126,
            "message_thread_id": 900,
            "caption": "continue parser from topic caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197401
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady {
        target,
        binding,
        prompt,
    } = outcome
    else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(binding.session.codex_session_id, "sess-1");
    assert_eq!(prompt, "continue parser from topic caption");
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn topic_caption_prompt_executes_and_replies_back_into_same_thread() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("caption topic transcript".to_owned())),
    };
    let update = parse_update(
        r#"
        {
          "message": {
            "chat": {
              "id": -1001293752024,
              "title": "CryptoInside Chat",
              "type": "supergroup",
              "username": "cryptoinside_talk",
              "is_forum": true
            },
            "date": 1721592580,
            "from": {
              "first_name": "the Cable Guy",
              "id": 5964236329,
              "is_bot": false,
              "language_code": "en",
              "username": "tg"
            },
            "message_id": 3127,
            "message_thread_id": 900,
            "caption": "continue parser from topic caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197403
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady { target, prompt, .. } = &outcome else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(prompt, "continue parser from topic caption");

    let completion =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("completion ok");

    let RuntimeOutcome::Replied { target, text } = completion else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "caption topic transcript");

    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, Some(900));
    assert_eq!(sent[0].2, "caption topic transcript");
}

#[test]
fn topic_caption_execution_failure_replies_back_into_same_thread() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser from topic caption".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, Some(900));
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn topic_caption_prompt_execution_failure_falls_back_to_chat_reply() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let update = parse_update(
        r#"
        {
          "message": {
            "chat": {
              "id": -1001293752024,
              "title": "CryptoInside Chat",
              "type": "supergroup",
              "username": "cryptoinside_talk",
              "is_forum": true
            },
            "date": 1721592580,
            "from": {
              "first_name": "the Cable Guy",
              "id": 5964236329,
              "is_bot": false,
              "language_code": "en",
              "username": "tg"
            },
            "message_id": 3128,
            "message_thread_id": 900,
            "caption": "continue parser from topic caption",
            "photo": [
              {
                "file_id": "id",
                "file_unique_id": "uq",
                "width": 1,
                "height": 1
              }
            ]
          },
          "update_id": 306197404
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let completion =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completion else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, None);
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn sends_resume_reply_in_direct_chat() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let update = parse_update(
        r#"{
            "message": {
                "chat": {
                    "id": 408258968,
                    "first_name": "Baha",
                    "type": "private",
                    "username": "spacewhaleblues"
                },
                "date": 1721592580,
                "entities": [
                    {
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "Baha",
                    "id": 408258968,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134547,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432601
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::Replied { target, text } = outcome else {
        panic!("expected reply outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert!(text.contains("resumed") || text.contains("019d6361"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert!(sent[0].2.contains("resumed") || sent[0].2.contains("019d6361"));
}

#[test]
fn returns_request_error_when_direct_resume_reply_send_fails() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Direct(408258968),
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("send boom".to_owned())),
    };
    let update = parse_update(
        r#"{
            "message": {
                "chat": {
                    "id": 408258968,
                    "first_name": "Baha",
                    "type": "private",
                    "username": "spacewhaleblues"
                },
                "date": 1721592580,
                "entities": [
                    {
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "Baha",
                    "id": 408258968,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134548,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432602
        }"#,
    );

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn sends_resume_reply_into_existing_topic_thread() {
    let controller = controller();
    let requester = FakeRequester::default();
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
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "the Cable Guy",
                    "id": 5964236329,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134547,
                "message_thread_id": 900,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432601
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::Replied { target, text } = outcome else {
        panic!("expected reply outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("resumed") || text.contains("019d6361"));
    assert!(text.contains("topic 900"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, Some(900));
    assert!(sent[0].2.contains("resumed") || sent[0].2.contains("019d6361"));
    assert!(sent[0].2.contains("topic 900"));
}

#[test]
fn returns_request_error_when_resume_reply_send_fails() {
    let controller = controller();
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("send boom".to_owned())),
    };
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
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "the Cable Guy",
                    "id": 5964236329,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134548,
                "message_thread_id": 900,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432602
        }"#,
    );

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn returns_request_error_when_resume_reply_thread_fallback_also_fails() {
    let controller = controller();
    let requester = ThreadThenChatFailRequester;
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
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "the Cable Guy",
                    "id": 5964236329,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134549,
                "message_thread_id": 900,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432603
        }"#,
    );

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("fallback chain should fail");
    assert!(
        err.to_string()
            .contains("telegram request failed: chat send boom")
    );
}

#[test]
fn falls_back_to_chat_reply_for_resume_reply_when_thread_send_fails() {
    let controller = controller();
    let requester = ThreadFailRequester::default();
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
                        "length": 8,
                        "offset": 0,
                        "type": "bot_command"
                    }
                ],
                "from": {
                    "first_name": "the Cable Guy",
                    "id": 5964236329,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134547,
                "message_thread_id": 900,
                "text": "/resume rust-rewrite"
            },
            "update_id": 439432601
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("fallback send expected");

    let RuntimeOutcome::Replied { target, text } = outcome else {
        panic!("expected reply outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("resumed") || text.contains("019d6361"));

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, None);
    assert!(sent[0].2.contains("resumed") || sent[0].2.contains("019d6361"));
}

#[test]
fn complete_runtime_outcome_does_not_resend_existing_reply() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("unused".to_owned())),
    };
    let outcome = RuntimeOutcome::Replied {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        text: "already sent".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("completion ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "already sent");
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn prompt_ready_executes_and_sends_transcript_reply() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("done transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(text, "done transcript");
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(
        sent.as_slice(),
        &[(408258968, None, "done transcript".to_owned())]
    );
}

#[test]
fn direct_prompt_execution_failure_is_sent_as_reply_text() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, None);
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.as_slice(), &[(408258968, None, text)]);
}

#[test]
fn topic_prompt_update_executes_and_replies_back_into_same_thread() {
    let store = InMemoryBindingStore::default();
    store.upsert_binding(vibes_core::SessionBinding {
        scope: vibes_core::ChatScope::Topic {
            chat_id: -1001293752024,
            topic_id: 900,
        },
        session: SessionHandle {
            codex_session_id: "sess-1".to_owned(),
            display_name: "rust-rewrite".to_owned(),
        },
        workspace_root: "/workspace".to_owned(),
    });
    let controller = AppController::new(AppService::new(store, FakeRuntime, FakeTopics));
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("topic transcript".to_owned())),
    };
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
                "from": {
                    "first_name": "the Cable Guy",
                    "id": 5964236329,
                    "is_bot": false,
                    "language_code": "en",
                    "username": "spacewhaleblues"
                },
                "message_id": 134548,
                "message_thread_id": 900,
                "text": "continue parser work"
            },
            "update_id": 439432602
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    let RuntimeOutcome::PromptReady { target, prompt, .. } = &outcome else {
        panic!("expected prompt-ready outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(prompt, "continue parser work");

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "topic transcript");
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(
        sent.as_slice(),
        &[(-1001293752024, Some(900), "topic transcript".to_owned())]
    );
}

#[test]
fn topic_prompt_result_falls_back_to_chat_reply_when_thread_send_fails() {
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("topic transcript after fallback".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "topic transcript after fallback");
    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, None);
    assert_eq!(sent[0].2, "topic transcript after fallback");
}

#[test]
fn topic_prompt_result_propagates_request_send_failure() {
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Ok("topic transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn topic_prompt_execution_failure_falls_back_to_chat_reply_when_thread_send_fails() {
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, None);
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn topic_prompt_execution_failure_replies_back_into_same_thread() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: -1001293752024,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, -1001293752024);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, -1001293752024);
    assert_eq!(sent[0].1, Some(900));
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn prompt_execution_failure_is_sent_as_reply_text() {
    let requester = FakeRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("execution ok");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));
    let sent = requester.sent.lock().expect("fake requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, Some(900));
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn direct_prompt_execution_failure_propagates_request_send_failure() {
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn direct_prompt_result_propagates_request_send_failure() {
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Ok("done transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn prompt_execution_result_propagates_request_send_failure() {
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("reply send boom".to_owned())),
    };
    let executor = FakeExecutor {
        response: Mutex::new(Ok("done transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("request send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: reply send boom")
    );
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn returns_request_error_when_reply_send_fails() {
    let controller = controller();
    let requester = FakeRequester {
        sent: Mutex::new(Vec::new()),
        fail: Mutex::new(Some("send boom".to_owned())),
    };
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

    let err = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect_err("send failure expected");
    assert!(
        err.to_string()
            .contains("telegram request failed: send boom")
    );
}

#[derive(Debug, Default)]
struct ThreadFailRequester {
    sent: Mutex<Vec<(i64, Option<i64>, String)>>,
}

#[derive(Debug, Default)]
struct ThreadThenChatFailRequester;

#[async_trait(?Send)]
impl TelegramRequester for ThreadFailRequester {
    async fn send_text(
        &self,
        target: &ReplyTarget,
        text: &str,
    ) -> Result<(), TelegramRequestError> {
        if target.message_thread_id.is_some() {
            return Err(TelegramRequestError::new("thread send boom"));
        }
        self.sent
            .lock()
            .expect("thread fail requester lock poisoned")
            .push((target.chat_id, target.message_thread_id, text.to_owned()));
        Ok(())
    }
}

#[async_trait(?Send)]
impl TelegramRequester for ThreadThenChatFailRequester {
    async fn send_text(
        &self,
        target: &ReplyTarget,
        _text: &str,
    ) -> Result<(), TelegramRequestError> {
        if target.message_thread_id.is_some() {
            Err(TelegramRequestError::new("thread send boom"))
        } else {
            Err(TelegramRequestError::new("chat send boom"))
        }
    }
}

#[test]
fn returns_request_error_when_thread_fallback_also_fails() {
    let requester = ThreadThenChatFailRequester;
    let executor = FakeExecutor {
        response: Mutex::new(Ok("done transcript".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let err = run_ready(complete_runtime_outcome(&requester, &executor, outcome))
        .expect_err("fallback chain should fail");
    assert!(
        err.to_string()
            .contains("telegram request failed: chat send boom")
    );
}

#[test]
fn falls_back_to_chat_reply_when_execution_failure_text_hits_thread_send_failure() {
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Err("boom".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, Some(900));
    assert!(text.contains("Codex execution failed: telegram execution failed: boom"));

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert!(
        sent[0]
            .2
            .contains("Codex execution failed: telegram execution failed: boom")
    );
}

#[test]
fn falls_back_to_chat_reply_when_thread_send_fails() {
    let requester = ThreadFailRequester::default();
    let executor = FakeExecutor {
        response: Mutex::new(Ok("transcript after fallback".to_owned())),
    };
    let outcome = RuntimeOutcome::PromptReady {
        target: ReplyTarget {
            chat_id: 408258968,
            message_thread_id: Some(900),
        },
        binding: vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            session: SessionHandle {
                codex_session_id: "sess-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        },
        prompt: "continue parser work".to_owned(),
    };

    let completed =
        run_ready(complete_runtime_outcome(&requester, &executor, outcome)).expect("fallback send");

    let RuntimeOutcome::Replied { target, text } = completed else {
        panic!("expected replied outcome");
    };
    assert_eq!(target.chat_id, 408258968);
    assert_eq!(target.message_thread_id, Some(900));
    assert_eq!(text, "transcript after fallback");

    let sent = requester
        .sent
        .lock()
        .expect("thread fail requester lock poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, 408258968);
    assert_eq!(sent[0].1, None);
    assert_eq!(sent[0].2, "transcript after fallback");
}

#[test]
fn ignores_non_message_updates() {
    let controller = controller();
    let requester = FakeRequester::default();
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

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");
    assert_eq!(outcome, RuntimeOutcome::Ignored);
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn ignores_plain_text_update_when_text_is_whitespace() {
    let controller = AppController::new(AppService::new(
        InMemoryBindingStore::default(),
        FakeRuntime,
        FakeTopics,
    ));
    let requester = FakeRequester::default();
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
            "message_id": 158,
            "text": "   "
          },
          "update_id": 306197402
        }
        "#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    assert_eq!(outcome, RuntimeOutcome::Ignored);
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}

#[test]
fn ignores_caption_only_media_when_caption_is_whitespace() {
    let controller = controller();
    let requester = FakeRequester::default();
    let update = parse_update(
        r#"{
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
                "message_id": 156,
                "caption": "   ",
                "photo": [
                    {
                        "file_id": "id",
                        "file_unique_id": "uq",
                        "width": 1,
                        "height": 1
                    }
                ]
            },
            "update_id": 306197400
        }"#,
    );

    let outcome = run_ready(run_telegram_update(
        &controller,
        &requester,
        &update,
        None,
        "/workspace",
    ))
    .expect("runtime ok");

    assert_eq!(outcome, RuntimeOutcome::Ignored);
    assert!(
        requester
            .sent
            .lock()
            .expect("fake requester lock poisoned")
            .is_empty()
    );
}
