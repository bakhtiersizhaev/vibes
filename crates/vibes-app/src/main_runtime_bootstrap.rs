use teloxide::Bot;
use vibes_codex::CodexExecRunner;

use crate::main_runtime_components::{build_runtime_components, RuntimeController};
use crate::main_startup_context::build_startup_context;

pub(crate) async fn build_runtime_bootstrap() -> anyhow::Result<(
    Bot,
    Option<String>,
    String,
    CodexExecRunner,
    RuntimeController,
)> {
    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;
    let (_store, runtime, _topics, controller) = build_runtime_components(&bot, &db_path)?;
    Ok((bot, bot_username, workspace_root, runtime, controller))
}
