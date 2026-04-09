use std::env;

use anyhow::Context;
use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
    update_listeners::{self, AsUpdateStream},
};
use tokio::pin;
use tokio_stream::StreamExt;
use tracing::{error, info};
use vibes_app::{
    AppController, AppService, AppServiceError, RuntimeOutcome, TelegramExecutionError,
    TelegramPromptExecutor, TopicManager, complete_runtime_outcome, run_telegram_update,
};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

struct BotTopicManager {
    bot: Bot,
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

struct CodexPromptExecutor<'a> {
    runner: &'a CodexExecRunner,
}

mod main_support;

use main_support::{bot_username, codex_request_and_cwd, rendered_or_default, runtime_paths};

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

async fn handle_prompt_ready(
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    target: vibes_telegram::ReplyTarget,
    binding: vibes_core::SessionBinding,
    prompt: String,
) {
    info!(
        chat_id = target.chat_id,
        thread_id = target.message_thread_id,
        scope = %binding.scope.scope_key(),
        session_id = %binding.session.codex_session_id,
        prompt_len = prompt.len(),
        "prompt ready for codex execution"
    );

    match complete_runtime_outcome(
        bot,
        executor,
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        },
    )
    .await
    {
        Ok(RuntimeOutcome::Replied { target, .. }) => {
            info!(
                chat_id = target.chat_id,
                thread_id = target.message_thread_id,
                "codex execution reply sent"
            );
        }
        Ok(other) => {
            info!(outcome = ?other, "unexpected runtime completion outcome");
        }
        Err(err) => {
            error!(error = %err, "failed to complete codex execution outcome");
        }
    }
}

async fn handle_runtime_outcome(
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    outcome: RuntimeOutcome,
) {
    match outcome {
        RuntimeOutcome::Ignored => {}
        RuntimeOutcome::Replied { target, .. } => {
            info!(
                chat_id = target.chat_id,
                thread_id = target.message_thread_id,
                "reply sent"
            );
        }
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => {
            handle_prompt_ready(bot, executor, target, binding, prompt).await;
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,vibes_app=debug".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let bot = Bot::from_env();
    let me = bot.get_me().send().await.context("get_me failed")?;
    let bot_username = bot_username(&me.user);
    let (workspace_root, db_path) = runtime_paths(
        env::var("VIBES_WORKSPACE_ROOT").ok(),
        env::var("VIBES_DB_PATH").ok(),
    );

    let store = SqliteBindingStore::open(&db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager { bot: bot.clone() };
    let controller = AppController::new(AppService::new(store, runtime.clone(), topics));
    let executor = CodexPromptExecutor { runner: &runtime };

    let mut listener = update_listeners::polling_default(bot.clone()).await;
    let stream = listener.as_stream();
    pin!(stream);

    info!(bot_username = ?bot_username, db_path, workspace_root, "vibes polling loop started");

    let shutdown = tokio::signal::ctrl_c();
    pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("ctrl-c received, stopping polling loop");
                break;
            }
            update = stream.next() => {
                let Some(update) = update else {
                    info!("polling listener stream ended");
                    break;
                };

                match update {
                    Ok(update) => match run_telegram_update(
                        &controller,
                        &bot,
                        &update,
                        bot_username.as_deref(),
                        &workspace_root,
                    )
                    .await
                    {
                        Ok(outcome) => {
                            handle_runtime_outcome(&bot, &executor, outcome).await;
                        }
                        Err(err) => {
                            error!(error = %err, "failed to handle telegram update");
                        }
                    },
                    Err(err) => error!(error = ?err, "polling listener error"),
                }
            }
        }
    }

    info!("vibes polling loop stopped");
    Ok(())
}
