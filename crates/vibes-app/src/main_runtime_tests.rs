use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{ApiError, Bot, RequestError, types::User};

use crate::main_runtime::{
    build_runtime_components, startup_context_from_get_me, startup_context_from_parts,
};

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_db_path() -> PathBuf {
    let pid = std::process::id();
    let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("vibes-build-runtime-{pid}-{n}.sqlite3"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_runtime_components_creates_sqlite_store_at_path() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_runtime_components_reopens_existing_sqlite_store() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
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

    #[test]
    fn build_runtime_components_returns_error_for_invalid_db_path() {
        let db_path = unique_db_path()
            .with_extension("")
            .join("nested")
            .join("vibes.sqlite3");

        let bot = Bot::new("123456:TESTTOKEN");
        let path_string = db_path.to_str().unwrap().to_owned();
        let rendered = match build_runtime_components(&bot, &path_string) {
            Ok(_) => panic!("expected sqlite open failure"),
            Err(err) => err.to_string(),
        };

        assert!(rendered.contains("failed to open sqlite store"));
        assert!(rendered.contains(&path_string));
    }


}
