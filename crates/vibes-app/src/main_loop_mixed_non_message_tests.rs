use teloxide::{ApiError, Bot, RequestError, types::Update};
use tokio_stream::iter;

use crate::main_runtime::run_polling_loop;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{NoopExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_non_message_then_request_error_until_stream_end()
     {
        let db_path = unique_db_path("vibes-loop-mixed-non-message-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let callback: Update = serde_json::from_str(
            r#"{
                "update_id": 1009,
                "callback_query": {
                    "id": "cb-main-9",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-9",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let stream = tokio_stream::iter(vec![
            Ok(callback),
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_non_message_then_message_until_stream_end() {
        let db_path = unique_db_path("vibes-loop-mixed-non-message-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let non_message: Update = serde_json::from_str(
            r#"{
                "update_id": 102,
                "callback_query": {
                    "id": "abc123",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let message: Update = serde_json::from_str(
            r#"{
                "update_id": 103,
                "message": {
                    "message_id": 11,
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
        let stream = iter(vec![Ok(non_message), Ok(message)]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    }
