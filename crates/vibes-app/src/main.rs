use teloxide::update_listeners::AsUpdateStream;
use teloxide::update_listeners;
use tracing::info;

#[cfg(test)]
mod main_event_tests;
#[cfg(test)]
mod main_loop_tests;
#[cfg(test)]
mod main_prompt_tests;
mod main_runtime;
mod main_startup;
mod main_support;
mod main_test_support;

use main_runtime::{CodexPromptExecutor, run_polling_loop};
use main_startup::{build_runtime_components, build_startup_context};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,vibes_app=debug".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;

    let (_store, runtime, _topics, controller) = build_runtime_components(&bot, &db_path)?;
    let executor = CodexPromptExecutor { runner: &runtime };

    let mut listener = update_listeners::polling_default(bot.clone()).await;
    let stream = listener.as_stream();

    info!(bot_username = ?bot_username, db_path, workspace_root, "vibes polling loop started");

    run_polling_loop(
        &controller,
        &bot,
        &executor,
        stream,
        bot_username.as_deref(),
        &workspace_root,
    )
    .await;
    Ok(())
}
