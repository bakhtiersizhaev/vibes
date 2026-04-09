use teloxide::prelude::Bot;
use tracing::{error, info};
use vibes_app::{RuntimeOutcome, TelegramPromptExecutor, complete_runtime_outcome};

pub(crate) async fn handle_prompt_ready<Q, E>(
    requester: &Q,
    executor: &E,
    target: vibes_telegram::ReplyTarget,
    binding: vibes_core::SessionBinding,
    prompt: String,
) where
    Q: vibes_app::TelegramRequester,
    E: TelegramPromptExecutor,
{
    info!(
        chat_id = target.chat_id,
        thread_id = target.message_thread_id,
        scope = %binding.scope.scope_key(),
        session_id = %binding.session.codex_session_id,
        prompt_len = prompt.len(),
        "prompt ready for codex execution"
    );

    match complete_runtime_outcome(
        requester,
        executor,
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        },
    )
    .await
    {
        Ok(RuntimeOutcome::Replied { target, text }) => {
            info!(
                chat_id = target.chat_id,
                thread_id = target.message_thread_id,
                text_len = text.len(),
                "codex execution reply sent"
            );
        }
        Ok(other) => {
            error!(outcome = ?other, "unexpected runtime completion outcome");
        }
        Err(err) => {
            error!(error = %err, "failed to complete codex execution outcome");
        }
    }
}

pub(crate) async fn handle_runtime_outcome<E>(bot: &Bot, executor: &E, outcome: RuntimeOutcome)
where
    E: TelegramPromptExecutor,
{
    match outcome {
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => handle_prompt_ready(bot, executor, target, binding, prompt).await,
        other => {
            let _ = complete_runtime_outcome(bot, executor, other).await;
        }
    }
}
