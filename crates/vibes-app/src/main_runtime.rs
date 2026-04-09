use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
};
use tokio::pin;
use tokio_stream::StreamExt;
use tracing::{error, info};
use vibes_app::{
    AppController, AppServiceError, RuntimeOutcome, TelegramExecutionError, TelegramPromptExecutor,
    TelegramRequester, TopicManager, complete_runtime_outcome, run_telegram_update,
};
use vibes_codex::CodexExecRunner;

use crate::main_support::{codex_request_and_cwd, rendered_or_default};

pub(crate) struct BotTopicManager {
    pub(crate) bot: Bot,
}

impl TopicManager for BotTopicManager {
    fn create_topic(&self, chat_id: i64, title: &str) -> Result<i64, AppServiceError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.bot
                    .create_forum_topic(ChatId(chat_id), title.to_owned())
                    .send()
                    .await
                    .map(|topic| i64::from(topic.thread_id.0.0))
                    .map_err(|err| AppServiceError::Topic(err.to_string()))
            })
        })
    }
}

pub(crate) struct CodexPromptExecutor<'a> {
    pub(crate) runner: &'a CodexExecRunner,
}

impl TelegramPromptExecutor for CodexPromptExecutor<'_> {
    fn execute_prompt(
        &self,
        binding: &vibes_core::SessionBinding,
        prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        let (request, cwd) = codex_request_and_cwd(binding, prompt);
        let result = self.runner.run(&request, &cwd).map_err(|err| {
            TelegramExecutionError::new(format!("codex prompt execution failed: {err}"))
        })?;
        Ok(rendered_or_default(result.transcript.rendered()))
    }
}

pub(crate) async fn handle_prompt_ready<Q, E>(
    requester: &Q,
    executor: &E,
    target: vibes_telegram::ReplyTarget,
    binding: vibes_core::SessionBinding,
    prompt: String,
) where
    Q: TelegramRequester,
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

pub(crate) async fn run_polling_loop_with_shutdown<S, F, E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    stream: S,
    shutdown: F,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    S: tokio_stream::Stream<Item = Result<teloxide::types::Update, teloxide::RequestError>>,
    F: core::future::Future<Output = ()>,
    E: TelegramPromptExecutor,
{
    pin!(shutdown);
    pin!(stream);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                info!("ctrl-c received, stopping polling loop");
                break;
            }
            update = stream.next() => {
                if !handle_next_listener_event(controller, bot, executor, update, bot_username, workspace_root).await {
                    break;
                }
            }
        }
    }
    info!("vibes polling loop stopped");
}

pub(crate) async fn run_polling_loop<S, E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    stream: S,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    S: tokio_stream::Stream<Item = Result<teloxide::types::Update, teloxide::RequestError>>,
    E: TelegramPromptExecutor,
{
    run_polling_loop_with_shutdown(
        controller,
        bot,
        executor,
        stream,
        async {
            let _ = tokio::signal::ctrl_c().await;
        },
        bot_username,
        workspace_root,
    )
    .await;
}
