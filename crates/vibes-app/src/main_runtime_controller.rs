use teloxide::prelude::Bot;
use vibes_app::{AppController, AppService};
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

use crate::main_runtime_store::open_sqlite_store;
use crate::main_runtime_topics::BotTopicManager;

pub(crate) type RuntimeController =
    AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>;

pub(crate) fn build_runtime_controller(
    bot: &Bot,
    db_path: &str,
    runtime: &CodexExecRunner,
) -> anyhow::Result<RuntimeController> {
    let controller_store = open_sqlite_store(db_path)?;
    Ok(AppController::new(AppService::new(
        controller_store,
        runtime.clone(),
        BotTopicManager::new(bot),
    )))
}
