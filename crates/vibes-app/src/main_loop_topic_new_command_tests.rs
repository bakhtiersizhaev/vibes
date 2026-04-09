use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, types::Update};
use tokio_stream::iter;

use crate::main_runtime::run_polling_loop;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::NoopExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-loop-topic-command-tests-{pid}-{n}.sqlite3"))
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_new_command_until_stream_end() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 35,
                "message": {
                    "message_id": 43,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_new_caption_until_stream_end() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 36,
                "message": {
                    "message_id": 44,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

}
