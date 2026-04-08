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
            request = request.message_thread_id(ThreadId(MessageId(thread_id as i32)));
        }
        request
            .send()
            .await
            .map(|_| ())
            .map_err(|err| TelegramRequestError::new(err.to_string()))
    }
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
            requester.send_text(&target, &text).await?;
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
        RuntimeOutcome::Replied { target, text } => {
            requester.send_text(&target, &text).await?;
            Ok(RuntimeOutcome::Replied { target, text })
        }
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => {
            let text = match executor.execute_prompt(&binding, &prompt) {
                Ok(text) => text,
                Err(err) => format!("Codex execution failed: {err}"),
            };
            requester.send_text(&target, &text).await?;
            Ok(RuntimeOutcome::Replied { target, text })
        }
    }
}
