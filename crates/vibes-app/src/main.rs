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
    AppController, AppService, AppServiceError, RuntimeOutcome, TopicManager, run_telegram_update,
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
    let bot_username = me.user.username.clone();
    let workspace_root = env::var("VIBES_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_owned());
    let db_path = env::var("VIBES_DB_PATH").unwrap_or_else(|_| "vibes.sqlite3".to_owned());

    let store = SqliteBindingStore::open(&db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager { bot: bot.clone() };
    let controller = AppController::new(AppService::new(store, runtime, topics));

    let mut listener = update_listeners::polling_default(bot.clone()).await;
    let stream = listener.as_stream();
    pin!(stream);

    info!(bot_username = ?bot_username, db_path, workspace_root, "vibes polling loop started");

    while let Some(update) = stream.next().await {
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
                Ok(RuntimeOutcome::Ignored) => {}
                Ok(RuntimeOutcome::Replied { target, .. }) => {
                    info!(
                        chat_id = target.chat_id,
                        thread_id = target.message_thread_id,
                        "reply sent"
                    );
                }
                Ok(RuntimeOutcome::PromptReady {
                    target,
                    binding,
                    prompt,
                }) => {
                    info!(
                        chat_id = target.chat_id,
                        thread_id = target.message_thread_id,
                        scope = %binding.scope.scope_key(),
                        session_id = %binding.session.codex_session_id,
                        prompt_len = prompt.len(),
                        "prompt ready for codex execution"
                    );
                }
                Err(err) => {
                    error!(error = %err, "failed to handle telegram update");
                }
            },
            Err(err) => error!(error = ?err, "polling listener error"),
        }
    }

    Ok(())
}
