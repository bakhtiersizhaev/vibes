use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, types::Update};

use crate::main_runtime::handle_listener_item;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::PanicExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-listener-topic-command-tests-{pid}-{n}.sqlite3"))
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

}
