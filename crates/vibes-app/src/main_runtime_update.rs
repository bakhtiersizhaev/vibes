use teloxide::prelude::Bot;
use tracing::error;
use vibes_app::{TelegramPromptExecutor, run_telegram_update};

use crate::main_runtime_components::RuntimeController;
use crate::main_runtime_outcome::handle_runtime_outcome;

pub(crate) async fn handle_update<E>(
    controller: &RuntimeController,
    bot: &Bot,
    executor: &E,
    update: &teloxide::types::Update,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    E: TelegramPromptExecutor,
{
    match run_telegram_update(controller, bot, update, bot_username, workspace_root).await {
        Ok(outcome) => handle_runtime_outcome(bot, executor, outcome).await,
        Err(err) => error!(error = ?err, "failed to handle telegram update"),
    }
}
