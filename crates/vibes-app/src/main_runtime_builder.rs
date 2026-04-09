use anyhow::Context;
use teloxide::prelude::Bot;
use vibes_app::{AppController, AppService};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

use crate::main_runtime_topics::BotTopicManager;
use crate::main_startup_context::build_startup_context;

pub(crate) type RuntimeController =
    AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>;

pub(crate) async fn build_runtime_bootstrap(
) -> anyhow::Result<(Bot, Option<String>, String, CodexExecRunner, RuntimeController)> {
    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;
    let (_store, runtime, _topics, controller) = build_runtime_components(&bot, &db_path)?;
    Ok((bot, bot_username, workspace_root, runtime, controller))
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
