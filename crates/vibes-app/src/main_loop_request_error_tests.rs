use teloxide::{ApiError, Bot, RequestError, types::Update};
use tokio_stream::iter;

use crate::main_runtime_loop::run_polling_loop;
use crate::main_runtime_builder::build_runtime_components;
use crate::main_test_support::{NoopExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_request_error_until_stream_end() {
        let db_path = unique_db_path("vibes-loop-request-error-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let request_error = RequestError::Api(ApiError::Unknown("bad request".to_owned()));
        let stream = iter(vec![Err(request_error)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_request_error_then_non_message_until_stream_end()
     {
        let db_path = unique_db_path("vibes-loop-request-error-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
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
        let stream = iter(vec![
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            Ok(update),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
