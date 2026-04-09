use teloxide::{Bot, RequestError, types::Update};
use tokio_stream::iter;

use crate::main_runtime_loop::run_polling_loop;
use crate::main_runtime_components::build_runtime_components;
use crate::main_test_support::{NoopExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_polling_loop_returns_immediately_on_empty_stream() {
        let db_path = unique_db_path("vibes-loop-stream-end-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let stream = iter(Vec::<Result<Update, RequestError>>::new());

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_processes_event_and_returns_on_stream_end() {
        let db_path = unique_db_path("vibes-loop-stream-end-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 19,
                "message": {
                    "message_id": 28,
                    "date": 1710000000,
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

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    }
