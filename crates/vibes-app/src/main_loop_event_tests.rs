use teloxide::{Bot, types::Update};
use tokio_stream::iter;

use crate::main_runtime::run_polling_loop;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{NoopExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_polling_loop_processes_multiple_events_before_stream_end() {
        let db_path = unique_db_path("vibes-loop-event-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update1: Update = serde_json::from_str(
            r#"{
                "update_id": 100,
                "message": {
                    "message_id": 200,
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
        let update2: Update = serde_json::from_str(
            r#"{
                "update_id": 101,
                "callback_query": {
                    "id": "cb-1",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-1",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let update3: Update = serde_json::from_str(
            r#"{
                "update_id": 102,
                "message": {
                    "message_id": 201,
                    "date": 1710000001,
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
                    "caption": "hi caption"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![Ok(update1), Ok(update2), Ok(update3)]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
