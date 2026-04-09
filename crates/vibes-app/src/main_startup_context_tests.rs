#[cfg(test)]
mod tests {
    use teloxide::{ApiError, Bot, RequestError, types::User};

    use crate::main_startup_context::{startup_context_from_get_me, startup_context_from_parts};

    #[test]
    fn startup_context_from_parts_preserves_missing_bot_username() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: User = serde_json::from_str(
            r#"{
                "id": 777,
                "is_bot": true,
                "first_name": "vibes-bot"
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace/custom".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(bot_username, None);
        assert_eq!(workspace_root, "/workspace/custom");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn startup_context_from_parts_normalizes_empty_bot_username() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: User = serde_json::from_str(
            r#"{
                "id": 777,
                "is_bot": true,
                "first_name": "vibes-bot",
                "username": ""
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace/custom".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(bot_username, None);
        assert_eq!(workspace_root, "/workspace/custom");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn startup_context_from_parts_preserves_bot_username_and_runtime_paths() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: teloxide::types::User = serde_json::from_str(
            r#"{
                "id": 408258968,
                "is_bot": true,
                "first_name": "VibesBot",
                "username": "vibes_bot"
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace".to_owned()),
            Some("/tmp/vibes.sqlite3".to_owned()),
        );

        assert_eq!(bot_username.as_deref(), Some("vibes_bot"));
        assert_eq!(workspace_root, "/workspace");
        assert_eq!(db_path, "/tmp/vibes.sqlite3");
    }
}

}
