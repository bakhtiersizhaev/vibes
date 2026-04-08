use std::sync::Mutex;

use async_trait::async_trait;
use teloxide::types::Update;
use vibes_app::{
    AppController, AppService, AppServiceError, RuntimeOutcome, SessionRuntime,
    TelegramRequestError, TelegramRequester, TopicManager, run_telegram_update,
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
}

#[async_trait(?Send)]
impl TelegramRequester for FakeRequester {
    async fn send_text(
        &self,
        target: &ReplyTarget,
        text: &str,
    ) -> Result<(), TelegramRequestError> {
        self.sent
            .lock()
            .expect("fake requester lock poisoned")
            .push((target.chat_id, target.message_thread_id, text.to_owned()));
        Ok(())
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
