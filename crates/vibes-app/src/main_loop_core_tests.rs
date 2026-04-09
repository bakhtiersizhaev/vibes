use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{ApiError, Bot, RequestError, types::Update};
use tokio_stream::{StreamExt, iter, pending};

use crate::main_runtime::{
    handle_next_listener_event, run_polling_loop, run_polling_loop_with_shutdown,
};
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{NoopExecutor, PanicExecutor, SharedWriter};

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-loop-core-tests-{pid}-{n}.sqlite3"))
    }

    #[tokio::test]
    async fn run_polling_loop_returns_on_stream_end_without_events() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let stream = tokio_stream::iter(vec![]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }


}

