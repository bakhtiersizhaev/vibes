
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
mod main_next_listener_topic_tests;
#[cfg(test)]
mod main_next_listener_forum_tests;
#[cfg(test)]
mod main_next_listener_basic_tests;
#[cfg(test)]
mod main_next_listener_stream_end_tests;
#[cfg(test)]
mod main_next_listener_direct_new_tests;
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
mod main_loop_immediate_shutdown_tests;
#[cfg(test)]
mod main_loop_shutdown_signal_tests;
#[cfg(test)]
mod main_prompt_success_tests;
#[cfg(test)]
mod main_prompt_tests;
mod main_runtime_builder;
mod main_runtime_bootstrap;
mod main_runtime_controller;
mod main_runtime_entry;
mod main_runtime_executor;
mod main_tracing;
mod main_runtime_listener;
mod main_runtime_loop;
mod main_runtime_outcome;
mod main_runtime_topics;
mod main_runtime_store;
mod main_runtime_update;
mod main_startup_context;
mod main_support;
#[cfg(test)]
mod main_test_support;
#[cfg(test)]
mod main_build_runtime_create_tests;
#[cfg(test)]
mod main_build_runtime_reopen_tests;
#[cfg(test)]
mod main_build_runtime_error_tests;
mod main_daemon_cli;
#[cfg(test)]
mod main_codex_request_direct_tests;
#[cfg(test)]
mod main_runtime_path_tests;

use clap::Parser;
use std::path::Path;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = main_daemon_cli::Cli::parse();
    if let Some(command) = &cli.command {
        if let main_daemon_cli::Command::Status(_args) = command {
            let result = main_daemon_cli::run_status_command(Path::new("."));
            println!("{}", result.output);
            std::process::exit(result.exit_code);
        }
        let output = main_daemon_cli::run_cli_command(command, Path::new("."))?;
        println!("{output}");
        return Ok(());
    }
    main_runtime_entry::run_app().await
}
