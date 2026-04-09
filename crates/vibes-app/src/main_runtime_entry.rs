use crate::main_runtime_builder::{RuntimeBootstrap, build_runtime_bootstrap};
use crate::main_runtime_executor::CodexPromptExecutor;
use crate::main_runtime_loop::start_polling_loop;
use crate::main_tracing::init_tracing;

pub(crate) async fn run_app() -> anyhow::Result<()> {
    init_tracing();

    let RuntimeBootstrap {
        bot,
        bot_username,
        workspace_root,
        runtime,
        controller,
    } = build_runtime_bootstrap().await?;
    let executor = CodexPromptExecutor { runner: &runtime };

    start_polling_loop(
        &controller,
        &bot,
        &executor,
        bot_username.as_deref(),
        &workspace_root,
    )
    .await;
    Ok(())
}
