use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, RequestError, types::Update};
use tokio_stream::{StreamExt, iter, pending};

use crate::main_runtime::run_polling_loop_with_shutdown;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{PanicExecutor, SharedWriter};

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-loop-ctrlc-tests-{pid}-{n}.sqlite3"))
    }

    #[tokio::test]
    async fn run_polling_loop_processes_event_before_shutdown_signal() {
        let db_path = unique_db_path();
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update = serde_json::from_str::<Update>(
            r#"{
                "update_id": 778,
                "message": {
                    "message_id": 701,
                    "date": 1710000778,
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
                    "text": "hello before shutdown"
                }
            }"#,
        )
        .unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let mut tx = Some(tx);
        let stream = iter(vec![Ok(update)]).map(move |item| {
            if let Some(tx) = tx.take() {
                let _ = tx.send(());
            }
            item
        });

        run_polling_loop_with_shutdown(
            &controller,
            &bot,
            &executor,
            stream,
            async move {
                let _ = rx.await;
            },
            None,
            "/workspace",
        )
        .await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    }
