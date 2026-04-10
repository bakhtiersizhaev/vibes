use teloxide::{Bot, RequestError, types::Update};
use tokio_stream::pending;

use crate::main_runtime_builder::build_runtime_components;
use crate::main_runtime_loop::run_polling_loop_with_shutdown;
use crate::main_test_support::{PanicExecutor, unique_db_path};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn run_polling_loop_logs_shutdown_and_stop() {
        let db_path = unique_db_path("vibes-loop-ctrlc-tests");
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (_store, _runtime, _topics, controller) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;

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

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }
}
