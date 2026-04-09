use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use teloxide::types::User;
use teloxide::{ApiError, Bot, RequestError, types::Update};
use vibes_app::RuntimeOutcome;

use crate::main_runtime::{
    build_runtime_components, handle_listener_item, handle_next_listener_event,
    handle_prompt_ready, handle_runtime_outcome, handle_update, startup_context_from_get_me,
    startup_context_from_parts,
};
use crate::main_test_support::{
    NoopExecutor, PanicExecutor, RecordingExecutor, RecordingRequester, SharedWriter,
};

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-build-runtime-{pid}-{n}.sqlite3"))
    }

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_ignored_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;

        handle_runtime_outcome(&bot, &executor, RuntimeOutcome::Ignored).await;
    }

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_replied_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;
        let outcome = RuntimeOutcome::Replied {
            target: vibes_telegram::ReplyTarget {
                chat_id: 408258968,
                message_thread_id: None,
            },
            text: "already replied".to_owned(),
        };

        handle_runtime_outcome(&bot, &executor, outcome).await;
    }

    #[tokio::test]
    async fn handle_update_processes_new_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 996,
                "message": {
                    "message_id": 774,
                    "date": 1710001108,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_prompt_ready_passes_binding_and_prompt_to_executor() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        };
        let executor = RecordingExecutor {
            seen: Mutex::new(Vec::new()),
            response: "recorded".to_owned(),
        };
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target.clone(),
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let seen = executor.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "/workspace");
        assert_eq!(seen[0].1, "codex-1");
        assert_eq!(seen[0].2, "continue parser work");
        drop(seen);

        let sent = requester.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, target);
        assert_eq!(sent[0].1, "recorded");
    }

    #[tokio::test]
    async fn handle_prompt_ready_sends_reply_on_success() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        };
        let executor = NoopExecutor;
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target.clone(),
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let sent = requester.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, target);
        assert_eq!(sent[0].1, "noop");
    }

    #[tokio::test]
    async fn handle_prompt_ready_ignores_completion_error_without_panicking() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(Some("send boom".to_owned())),
        };
        let executor = NoopExecutor;
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target,
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        assert!(requester.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle_prompt_ready_logs_completion_error() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(Some("send boom".to_owned())),
        };
        let executor = NoopExecutor;
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };
        let writer = SharedWriter::default();
        let captured = writer.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        handle_prompt_ready(
            &requester,
            &executor,
            target,
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let rendered = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("failed to complete codex execution outcome"));
        assert!(requester.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle_update_processes_new_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "date": 1710001109,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_new_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "message_thread_id": null,
                    "date": 1710001109,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_new_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 998,
                "message": {
                    "message_id": 776,
                    "message_thread_id": null,
                    "date": 1710001110,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_new_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1001,
                "message": {
                    "message_id": 779,
                    "message_thread_id": 900,
                    "date": 1710001113,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_new_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1002,
                "message": {
                    "message_id": 780,
                    "message_thread_id": 900,
                    "date": 1710001114,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_resume_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 999,
                "message": {
                    "message_id": 777,
                    "message_thread_id": null,
                    "date": 1710001111,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_resume_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1000,
                "message": {
                    "message_id": 778,
                    "message_thread_id": null,
                    "date": 1710001112,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_resume_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1003,
                "message": {
                    "message_id": 781,
                    "message_thread_id": 900,
                    "date": 1710001115,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_resume_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1004,
                "message": {
                    "message_id": 782,
                    "message_thread_id": 900,
                    "date": 1710001116,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_resume_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 995,
                "message": {
                    "message_id": 773,
                    "date": 1710001107,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_resume_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 994,
                "message": {
                    "message_id": 772,
                    "date": 1710001106,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_ignores_non_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1001,
                "callback_query": {
                    "id": "cb-main-2",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-2",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1005,
                "message": {
                    "message_id": 783,
                    "message_thread_id": 900,
                    "date": 1710001117,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser work"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 998,
                "message": {
                    "message_id": 776,
                    "date": 1710001110,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1006,
                "message": {
                    "message_id": 784,
                    "message_thread_id": 900,
                    "date": 1710001118,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from topic caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "date": 1710001109,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "caption only"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_logs_runtime_error() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 998,
                "message": {
                    "message_id": 776,
                    "date": 1710001110,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "message_thread_id": 900,
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello without binding"
                }
            }"#,
        )
        .unwrap();
        let writer = SharedWriter::default();
        let captured = writer.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        let rendered = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("failed to handle telegram update"));

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_ignores_runtime_error_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 999,
                "message": {
                    "message_id": 777,
                    "date": 1710001111,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "message_thread_id": 900,
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello without binding"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, &update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_ignores_non_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1000,
                "callback_query": {
                    "id": "cb-main-1",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-1",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_new_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1016,
                "message": {
                    "message_id": 794,
                    "message_thread_id": 900,
                    "date": 1710001128,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq3",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_resume_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1018,
                "message": {
                    "message_id": 796,
                    "message_thread_id": 900,
                    "date": 1710001130,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq4",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_resume_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1017,
                "message": {
                    "message_id": 795,
                    "message_thread_id": 900,
                    "date": 1710001129,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_new_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1015,
                "message": {
                    "message_id": 793,
                    "message_thread_id": 900,
                    "date": 1710001127,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1007,
                "message": {
                    "message_id": 785,
                    "message_thread_id": 900,
                    "date": 1710001119,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser from topic"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_resume_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1014,
                "message": {
                    "message_id": 792,
                    "date": 1710001126,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq2",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_resume_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1013,
                "message": {
                    "message_id": 791,
                    "date": 1710001125,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_new_command_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1011,
                "message": {
                    "message_id": 789,
                    "date": 1710001123,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1009,
                "message": {
                    "message_id": 787,
                    "date": 1710001121,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser from forum root"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_logs_request_error() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let writer = SharedWriter::default();
        let captured = writer.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        handle_listener_item(
            &controller,
            &bot,
            &executor,
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            None,
            "/workspace",
        )
        .await;

        let rendered = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("polling listener error"));

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_message_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1002,
                "message": {
                    "message_id": 778,
                    "date": 1710002222,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1008,
                "message": {
                    "message_id": 786,
                    "message_thread_id": 900,
                    "date": 1710001120,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from topic caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_new_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1012,
                "message": {
                    "message_id": 790,
                    "date": 1710001124,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1010,
                "message": {
                    "message_id": 788,
                    "date": 1710001122,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from forum root caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_caption_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1003,
                "message": {
                    "message_id": 779,
                    "date": 1710003333,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "caption only"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_ignores_request_error_without_executor_use() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;

        handle_listener_item(
            &controller,
            &bot,
            &executor,
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            None,
            "/workspace",
        )
        .await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
