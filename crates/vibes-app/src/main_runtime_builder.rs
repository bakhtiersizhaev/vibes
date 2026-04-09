use teloxide::prelude::Bot;
use vibes_app::{AppController, AppService};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

use crate::main_runtime_topics::{BotTopicManager, build_topic_manager};
use crate::main_runtime_store::open_sqlite_store;
use crate::main_startup_context::build_startup_context;

pub(crate) type RuntimeController =
    AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>;


pub(crate) struct RuntimeBootstrap {
    pub(crate) bot: Bot,
    pub(crate) bot_username: Option<String>,
    pub(crate) workspace_root: String,
    pub(crate) runtime: CodexExecRunner,
    pub(crate) controller: RuntimeController,
}

pub(crate) async fn build_runtime_bootstrap() -> anyhow::Result<RuntimeBootstrap> {
    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;
    let (_store, runtime, _topics, controller) = build_runtime_components(&bot, &db_path)?;
    Ok(RuntimeBootstrap {
        bot,
        bot_username,
        workspace_root,
        runtime,
        controller,
    })
}

fn build_runtime_controller(
    bot: &Bot,
    db_path: &str,
    runtime: &CodexExecRunner,
) -> anyhow::Result<RuntimeController> {
    let controller_store = open_sqlite_store(db_path)?;
    Ok(AppController::new(AppService::new(
        controller_store,
        runtime.clone(),
        build_topic_manager(bot),
    )))
}

pub(crate) fn build_runtime_components(
    bot: &Bot,
    db_path: &str,
) -> anyhow::Result<(SqliteBindingStore, CodexExecRunner, BotTopicManager, RuntimeController)> {
    let store = open_sqlite_store(db_path)?;
    let runtime = CodexExecRunner::default();
    let topics = build_topic_manager(bot);
    let controller = build_runtime_controller(bot, db_path, &runtime)?;
    Ok((store, runtime, topics, controller))
}
