use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::Bot;

use crate::main_runtime::handle_next_listener_event;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::NoopExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-next-listener-stream-end-tests-{pid}-{n}.sqlite3"))
    }

    #[tokio::test]
    async fn handle_next_listener_event_returns_false_for_stream_end() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;

        let keep_running =
            handle_next_listener_event(&controller, &bot, &executor, None, None, "/workspace")
                .await;

        assert!(!keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
