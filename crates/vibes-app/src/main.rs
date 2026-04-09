use teloxide::update_listeners;
use teloxide::update_listeners::AsUpdateStream;
use tracing::info;

#[cfg(test)]
mod main_listener_forum_new_tests;
#[cfg(test)]
mod main_listener_forum_resume_tests;
#[cfg(test)]
mod main_listener_topic_new_tests;
#[cfg(test)]
mod main_listener_topic_resume_tests;
#[cfg(test)]
mod main_listener_basic_tests;
#[cfg(test)]
mod main_listener_error_tests;
#[cfg(test)]
mod main_next_listener_topic_plain_tests;
#[cfg(test)]
mod main_next_listener_forum_plain_tests;
#[cfg(test)]
mod main_next_listener_direct_tests;
#[cfg(test)]
mod main_next_listener_stream_end_tests;
#[cfg(test)]
mod main_next_listener_direct_new_command_tests;
#[cfg(test)]
mod main_next_listener_direct_resume_tests;
#[cfg(test)]
mod main_next_listener_forum_new_tests;
#[cfg(test)]
mod main_next_listener_forum_resume_tests;
#[cfg(test)]
mod main_loop_event_tests;
#[cfg(test)]
mod main_loop_stream_end_tests;
#[cfg(test)]
mod main_loop_transition_tests;
#[cfg(test)]
mod main_loop_non_message_tests;
#[cfg(test)]
mod main_loop_mixed_non_message_tests;
#[cfg(test)]
mod main_loop_request_error_tests;
#[cfg(test)]
mod main_loop_mixed_tests;
#[cfg(test)]
mod main_loop_direct_tests;
#[cfg(test)]
mod main_loop_topic_tests;
#[cfg(test)]
mod main_loop_forum_tests;
#[cfg(test)]
mod main_loop_direct_new_tests;
#[cfg(test)]
mod main_loop_forum_new_tests;
#[cfg(test)]
mod main_loop_forum_resume_tests;
#[cfg(test)]
mod main_loop_direct_resume_tests;
#[cfg(test)]
mod main_loop_topic_new_tests;
#[cfg(test)]
mod main_loop_topic_resume_tests;
#[cfg(test)]
mod main_loop_ctrlc_tests;
#[cfg(test)]
mod main_loop_shutdown_signal_tests;
#[cfg(test)]
mod main_prompt_success_tests;
#[cfg(test)]
mod main_prompt_tests;
mod main_runtime;
mod main_runtime_handlers;
mod main_startup;
mod main_support;
#[cfg(test)]
mod main_test_support;
#[cfg(test)]
mod main_build_runtime_create_tests;
#[cfg(test)]
mod main_build_runtime_reopen_tests;
#[cfg(test)]
mod main_build_runtime_error_tests;
#[cfg(test)]
mod main_codex_request_direct_tests;
#[cfg(test)]
mod main_runtime_path_tests;

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
