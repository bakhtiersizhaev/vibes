use teloxide::prelude::Bot;
use vibes_codex::CodexExecRunner;

use crate::main_runtime_builder::{RuntimeController, build_runtime_components};
use crate::main_startup_context::build_startup_context;

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
