use teloxide::prelude::Bot;
use tracing::{error, info};
use vibes_app::{AppController, TelegramPromptExecutor, run_telegram_update};

use crate::main_runtime::BotTopicManager;
use crate::main_runtime_outcome::handle_runtime_outcome;

pub(crate) async fn handle_update<E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
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

pub(crate) async fn handle_listener_item<E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    item: Result<teloxide::types::Update, teloxide::RequestError>,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    E: TelegramPromptExecutor,
{
    match item {
        Ok(update) => {
            handle_update(
                controller,
                bot,
                executor,
                &update,
                bot_username,
                workspace_root,
            )
            .await
        }
        Err(err) => error!(error = ?err, "polling listener error"),
    }
}

pub(crate) async fn handle_next_listener_event<E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    update: Option<Result<teloxide::types::Update, teloxide::RequestError>>,
    bot_username: Option<&str>,
    workspace_root: &str,
) -> bool
where
    E: TelegramPromptExecutor,
{
    let Some(update) = update else {
        info!("polling listener stream ended");
        return false;
    };

    handle_listener_item(
        controller,
        bot,
        executor,
        update,
        bot_username,
        workspace_root,
    )
    .await;
    true
}
