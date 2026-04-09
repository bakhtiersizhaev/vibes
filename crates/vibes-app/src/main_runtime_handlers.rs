use teloxide::prelude::Bot;
use tracing::{error, info};
use vibes_app::{
    AppController, RuntimeOutcome, TelegramPromptExecutor, complete_runtime_outcome,
    run_telegram_update,
};

use crate::main_runtime::BotTopicManager;

pub(crate) async fn handle_prompt_ready<Q, E>(
    requester: &Q,
    executor: &E,
    target: vibes_telegram::ReplyTarget,
    binding: vibes_core::SessionBinding,
    prompt: String,
) where
    Q: vibes_app::TelegramRequester,
    E: TelegramPromptExecutor,
{
    info!(
        chat_id = target.chat_id,
        thread_id = target.message_thread_id,
        scope = %binding.scope.scope_key(),
        session_id = %binding.session.codex_session_id,
        prompt_len = prompt.len(),
        "prompt ready for codex execution"
    );

    match complete_runtime_outcome(
        requester,
        executor,
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        },
    )
    .await
    {
        Ok(RuntimeOutcome::Replied { target, text }) => {
            info!(
                chat_id = target.chat_id,
                thread_id = target.message_thread_id,
                text_len = text.len(),
                "codex execution reply sent"
            );
        }
        Ok(other) => {
            error!(outcome = ?other, "unexpected runtime completion outcome");
        }
        Err(err) => {
            error!(error = %err, "failed to complete codex execution outcome");
        }
    }
}

pub(crate) async fn handle_runtime_outcome<E>(bot: &Bot, executor: &E, outcome: RuntimeOutcome)
where
    E: TelegramPromptExecutor,
{
    match outcome {
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => handle_prompt_ready(bot, executor, target, binding, prompt).await,
        other => {
            let _ = complete_runtime_outcome(bot, executor, other).await;
        }
    }
}

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
