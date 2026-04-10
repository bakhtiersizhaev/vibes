use teloxide::{Bot, types::Update};

use crate::main_runtime_builder::build_runtime_components;
use crate::main_runtime_listener::handle_next_listener_event;
use crate::main_test_support::{NoopExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_resume_command() {
        let db_path = unique_db_path("vibes-next-listener-direct-resume-command-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 11,
                "message": {
                    "message_id": 20,
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
                    "entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_resume_caption() {
        let db_path = unique_db_path("vibes-next-listener-direct-resume-command-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 12,
                "message": {
                    "message_id": 21,
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
                    "caption_entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
