#[cfg(test)]
mod tests {
    use teloxide::{Bot, types::Update};

    use crate::main_runtime_listener::handle_listener_item;
    use crate::main_runtime_builder::build_runtime_components;
    use crate::main_test_support::{PanicExecutor, unique_db_path};

    #[tokio::test]
    async fn handle_listener_item_processes_topic_resume_caption_without_executor_use() {
        let db_path = unique_db_path("vibes-listener-topic-resume-tests");
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
        let db_path = unique_db_path("vibes-listener-topic-resume-tests");
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
}
