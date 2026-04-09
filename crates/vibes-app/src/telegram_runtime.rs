use async_trait::async_trait;
use teloxide::{
    payloads::SendMessageSetters,
    requests::{Request, Requester},
    types::{ChatId, MessageId, ThreadId, Update},
};
use vibes_core::SessionBinding;
use vibes_store::SessionBindingStore;
use vibes_telegram::ReplyTarget;

use crate::{
    AppController, HandleMessageError, SessionRuntime, TelegramUpdateAction, TopicManager,
    handle_telegram_update,
};

#[derive(Debug, thiserror::Error)]
pub enum TelegramRuntimeError {
    #[error(transparent)]
    Handle(#[from] HandleMessageError),
    #[error(transparent)]
    Request(#[from] TelegramRequestError),
    #[error(transparent)]
    Execute(#[from] TelegramExecutionError),
}

#[derive(Debug, thiserror::Error)]
#[error("telegram request failed: {message}")]
pub struct TelegramRequestError {
    message: String,
}

impl TelegramRequestError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("telegram execution failed: {message}")]
pub struct TelegramExecutionError {
    message: String,
}

impl TelegramExecutionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOutcome {
    Ignored,
    Replied {
        target: ReplyTarget,
        text: String,
    },
    PromptReady {
        target: ReplyTarget,
        binding: SessionBinding,
        prompt: String,
    },
}

#[async_trait(?Send)]
pub trait TelegramRequester: Send + Sync {
    async fn send_text(&self, target: &ReplyTarget, text: &str)
    -> Result<(), TelegramRequestError>;
}

pub trait TelegramPromptExecutor: Send + Sync {
    fn execute_prompt(
        &self,
        binding: &SessionBinding,
        prompt: &str,
    ) -> Result<String, TelegramExecutionError>;
}

#[async_trait(?Send)]
impl<R> TelegramRequester for R
where
    R: Requester + Send + Sync,
    R::Err: std::fmt::Display + Send + Sync,
{
    async fn send_text(
        &self,
        target: &ReplyTarget,
        text: &str,
    ) -> Result<(), TelegramRequestError> {
        let mut request = self.send_message(ChatId(target.chat_id), text.to_owned());
        if let Some(thread_id) = target.message_thread_id {
            request = request.message_thread_id(thread_id_to_teloxide(thread_id)?);
        }
        request
            .send()
            .await
            .map(|_| ())
            .map_err(|err| TelegramRequestError::new(err.to_string()))
    }
}

fn thread_id_to_teloxide(thread_id: i64) -> Result<ThreadId, TelegramRequestError> {
    if thread_id <= 0 {
        return Err(TelegramRequestError::new(format!(
            "thread id must be positive: {thread_id}"
        )));
    }

    let raw = i32::try_from(thread_id).map_err(|_| {
        TelegramRequestError::new(format!("thread id out of i32 range: {thread_id}"))
    })?;
    Ok(ThreadId(MessageId(raw)))
}

pub async fn run_telegram_update<S, R, T, Q>(
    controller: &AppController<S, R, T>,
    requester: &Q,
    update: &Update,
    bot_username: Option<&str>,
    default_workspace_root: &str,
) -> Result<RuntimeOutcome, TelegramRuntimeError>
where
    S: SessionBindingStore,
    R: SessionRuntime,
    T: TopicManager,
    Q: TelegramRequester,
{
    let action = handle_telegram_update(controller, update, bot_username, default_workspace_root)?;
    let Some(action) = action else {
        return Ok(RuntimeOutcome::Ignored);
    };

    match action {
        TelegramUpdateAction::Reply { target, text } => {
            send_with_thread_fallback(requester, &target, &text).await?;
            Ok(RuntimeOutcome::Replied { target, text })
        }
        TelegramUpdateAction::DispatchPrompt {
            target,
            binding,
            prompt,
        } => Ok(RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        }),
    }
}

pub async fn complete_runtime_outcome<Q, E>(
    requester: &Q,
    executor: &E,
    outcome: RuntimeOutcome,
) -> Result<RuntimeOutcome, TelegramRuntimeError>
where
    Q: TelegramRequester,
    E: TelegramPromptExecutor,
{
    match outcome {
        RuntimeOutcome::Ignored => Ok(RuntimeOutcome::Ignored),
        RuntimeOutcome::Replied { target, text } => Ok(RuntimeOutcome::Replied { target, text }),
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => {
            let text = match executor.execute_prompt(&binding, &prompt) {
                Ok(text) => text,
                Err(err) => format!("Codex execution failed: {err}"),
            };
            send_with_thread_fallback(requester, &target, &text).await?;
            Ok(RuntimeOutcome::Replied { target, text })
        }
    }
}

async fn send_with_thread_fallback<Q>(
    requester: &Q,
    target: &ReplyTarget,
    text: &str,
) -> Result<(), TelegramRequestError>
where
    Q: TelegramRequester,
{
    match requester.send_text(target, text).await {
        Ok(()) => Ok(()),
        Err(first_err) if target.message_thread_id.is_some() => {
            let fallback_target = ReplyTarget {
                chat_id: target.chat_id,
                message_thread_id: None,
            };
            requester
                .send_text(&fallback_target, text)
                .await
                .map_err(|fallback_err| {
                    TelegramRequestError::new(format!(
                        "{first_err}; fallback without thread failed: {fallback_err}"
                    ))
                })
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::thread_id_to_teloxide;

    #[test]
    fn converts_thread_id_at_lower_positive_bound() {
        let thread_id = thread_id_to_teloxide(1).expect("conversion should succeed");
        assert_eq!(thread_id.0.0, 1);
    }

    #[test]
    fn converts_thread_id_in_i32_range() {
        let thread_id = thread_id_to_teloxide(900).expect("conversion should succeed");
        assert_eq!(thread_id.0.0, 900);
    }

    #[test]
    fn converts_thread_id_at_i32_upper_bound() {
        let thread_id = thread_id_to_teloxide(i64::from(i32::MAX))
            .expect("i32::MAX should still be representable");
        assert_eq!(thread_id.0.0, i32::MAX);
    }

    #[test]
    fn rejects_non_positive_thread_id() {
        let err = thread_id_to_teloxide(0).expect_err("zero should fail");
        assert!(err.to_string().contains("thread id must be positive"));

        let err = thread_id_to_teloxide(-1).expect_err("negative should fail");
        assert!(err.to_string().contains("thread id must be positive"));
    }

    #[test]
    fn rejects_thread_id_just_above_i32_upper_bound() {
        let err =
            thread_id_to_teloxide(i64::from(i32::MAX) + 1).expect_err("i32::MAX + 1 should fail");
        assert!(err.to_string().contains("thread id out of i32 range"));
    }

    #[test]
    fn rejects_thread_id_out_of_i32_range() {
        let err = thread_id_to_teloxide(i64::MAX).expect_err("conversion should fail");
        assert!(err.to_string().contains("thread id out of i32 range"));
    }
}
