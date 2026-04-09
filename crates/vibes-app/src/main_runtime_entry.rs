use crate::main_runtime::CodexPromptExecutor;
use crate::main_runtime_components::build_runtime_components;
use crate::main_runtime_loop::start_polling_loop;
use crate::main_startup_context::build_startup_context;

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,vibes_app=debug".into()),
        )
        .with_target(false)
        .compact()
        .init();
}

pub(crate) async fn run_app() -> anyhow::Result<()> {
    init_tracing();

    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;

    let (_store, runtime, _topics, controller) = build_runtime_components(&bot, &db_path)?;
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
