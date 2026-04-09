#[cfg(test)]
mod tests {
    use teloxide::{ApiError, Bot, RequestError, types::User};

    use crate::main_startup::{startup_context_from_get_me, startup_context_from_parts};
    use crate::main_support::{bot_username, runtime_paths};

    #[test]
    fn runtime_paths_use_defaults_when_env_is_missing() {
        let (workspace_root, db_path) = runtime_paths(None, None);

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_env_overrides() {
        let (workspace_root, db_path) = runtime_paths(
            Some("/tmp/custom-workspace".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_workspace_override_with_default_db() {
        let (workspace_root, db_path) =
            runtime_paths(Some("/tmp/custom-workspace".to_owned()), None);

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_db_override_with_default_workspace() {
        let (workspace_root, db_path) = runtime_paths(None, Some("/tmp/custom.sqlite3".to_owned()));

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_empty_string_overrides() {
        let (workspace_root, db_path) = runtime_paths(Some(String::new()), Some(String::new()));

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_empty_workspace_override_with_default_db() {
        let (workspace_root, db_path) = runtime_paths(Some(String::new()), None);

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_empty_db_override_with_default_workspace() {
        let (workspace_root, db_path) = runtime_paths(None, Some(String::new()));

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_workspace_override_with_empty_db() {
        let (workspace_root, db_path) = runtime_paths(
            Some("/tmp/custom-workspace".to_owned()),
            Some(String::new()),
        );

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_empty_workspace_with_db_override() {
        let (workspace_root, db_path) =
            runtime_paths(Some(String::new()), Some("/tmp/custom.sqlite3".to_owned()));

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn bot_username_returns_some_when_present() {
        let user: teloxide::types::User = serde_json::from_str(
            r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":"vibes_bot"}"#,
        )
        .unwrap();

        assert_eq!(bot_username(&user).as_deref(), Some("vibes_bot"));
    }

    #[test]
    fn bot_username_returns_none_when_missing() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes"}"#).unwrap();

        assert_eq!(bot_username(&user), None);
    }

    #[test]
    fn bot_username_returns_none_for_empty_string() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":""}"#)
                .unwrap();

        assert_eq!(bot_username(&user), None);
    }

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

    #[tokio::test]
    async fn startup_context_from_get_me_preserves_username_and_runtime_paths() {
        let bot = Bot::new("123456:TESTTOKEN");
        let me: teloxide::types::Me = serde_json::from_str(
            r#"{
                "id": 1,
                "is_bot": true,
                "first_name": "VibesBot",
                "username": "vibes_bot",
                "can_join_groups": true,
                "can_read_all_group_messages": false,
                "supports_inline_queries": false,
                "can_connect_to_business": false,
                "has_main_web_app": false
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_get_me(
            bot,
            Ok(me),
            Some("/workspace".to_owned()),
            Some("/tmp/vibes.sqlite3".to_owned()),
        )
        .await
        .unwrap();

        assert_eq!(bot_username.as_deref(), Some("vibes_bot"));
        assert_eq!(workspace_root, "/workspace");
        assert_eq!(db_path, "/tmp/vibes.sqlite3");
    }

    #[tokio::test]
    async fn startup_context_from_get_me_wraps_error_with_context() {
        let bot = Bot::new("123456:TESTTOKEN");
        let error = startup_context_from_get_me(
            bot,
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            None,
            None,
        )
        .await
        .unwrap_err();

        let rendered = format!("{error:#}");
        assert!(rendered.contains("get_me failed"));
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
