use anyhow::Context;
use teloxide::prelude::Bot;
use vibes_app::{AppController, AppService};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

use crate::main_runtime::BotTopicManager;

pub(crate) fn build_runtime_components(
    bot: &Bot,
    db_path: &str,
) -> anyhow::Result<(
    SqliteBindingStore,
    CodexExecRunner,
    BotTopicManager,
    AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
)> {
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

pub(crate) use crate::main_startup_context::{
    build_startup_context, startup_context_from_get_me, startup_context_from_parts,
};
