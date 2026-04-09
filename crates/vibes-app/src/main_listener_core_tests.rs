use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{ApiError, Bot, RequestError, types::Update};

use crate::main_runtime::handle_listener_item;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{PanicExecutor, SharedWriter};

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-listener-tests-{pid}-{n}.sqlite3"))
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
}
