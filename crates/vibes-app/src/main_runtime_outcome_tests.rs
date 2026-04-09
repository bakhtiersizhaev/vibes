use teloxide::Bot;
use vibes_app::RuntimeOutcome;

use crate::main_runtime_outcome::handle_runtime_outcome;
use crate::main_test_support::PanicExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_ignored_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;

        handle_runtime_outcome(&bot, &executor, RuntimeOutcome::Ignored).await;
    }

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_replied_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;
        let outcome = RuntimeOutcome::Replied {
            target: vibes_telegram::ReplyTarget {
                chat_id: 408258968,
                message_thread_id: None,
            },
            text: "already replied".to_owned(),
        };

        handle_runtime_outcome(&bot, &executor, outcome).await;
    }
}
