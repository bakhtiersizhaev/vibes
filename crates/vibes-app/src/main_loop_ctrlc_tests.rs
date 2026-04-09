use teloxide::{Bot, RequestError, types::Update};
use tokio_stream::pending;

use crate::main_runtime::run_polling_loop_with_shutdown;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::{PanicExecutor, SharedWriter, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_polling_loop_logs_shutdown_and_stop() {
        let db_path = unique_db_path("vibes-loop-ctrlc-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let writer = SharedWriter::default();
        let captured = writer.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        run_polling_loop_with_shutdown(
            &controller,
            &bot,
            &executor,
            pending::<Result<Update, RequestError>>(),
            std::future::ready(()),
            None,
            "/workspace",
        )
        .await;

        let rendered = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("ctrl-c received, stopping polling loop"));
        assert!(rendered.contains("vibes polling loop stopped"));

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_returns_when_shutdown_resolves_immediately() {
        let db_path = unique_db_path("vibes-loop-ctrlc-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let stream = tokio_stream::iter(vec![Ok(serde_json::from_str::<Update>(
            r#"{
                "update_id": 777,
                "message": {
                    "message_id": 700,
                    "date": 1710000777,
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
                    "text": "should not be consumed"
                }
            }"#,
        )
        .unwrap())]);

        run_polling_loop_with_shutdown(
            &controller,
            &bot,
            &executor,
            stream,
            std::future::ready(()),
            None,
            "/workspace",
        )
        .await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }


}
