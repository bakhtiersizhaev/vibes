use teloxide::prelude::Bot;
use vibes_codex::CodexExecRunner;
use vibes_store::SqliteBindingStore;

use crate::main_runtime_controller::{RuntimeController, build_runtime_controller};
use crate::main_runtime_topics::BotTopicManager;
use crate::main_runtime_store::open_sqlite_store;

pub(crate) fn build_runtime_components(
    bot: &Bot,
    db_path: &str,
) -> anyhow::Result<(SqliteBindingStore, CodexExecRunner, BotTopicManager, RuntimeController)> {
    let store = open_sqlite_store(db_path)?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager::new(bot);
    let controller = build_runtime_controller(bot, db_path, &runtime)?;
    Ok((store, runtime, topics, controller))
}
