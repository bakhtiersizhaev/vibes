use anyhow::Context;
use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
};
use vibes_app::{AppController, AppService, AppServiceError, TopicManager};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

pub(crate) use crate::main_runtime_executor::CodexPromptExecutor;

pub(crate) type RuntimeController =
    AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>;

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

pub(crate) fn build_runtime_components(
    bot: &Bot,
    db_path: &str,
) -> anyhow::Result<(SqliteBindingStore, CodexExecRunner, BotTopicManager, RuntimeController)> {
    let store = SqliteBindingStore::open(db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager { bot: bot.clone() };
    let controller_store = SqliteBindingStore::open(db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let controller = AppController::new(AppService::new(
        controller_store,
        runtime.clone(),
        BotTopicManager { bot: bot.clone() },
    ));
    Ok((store, runtime, topics, controller))
}
