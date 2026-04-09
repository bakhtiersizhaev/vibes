use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
};
use tokio::pin;
use tokio_stream::StreamExt;
use tracing::info;
use vibes_app::{AppController, AppServiceError, TelegramExecutionError, TelegramPromptExecutor, TopicManager};
use vibes_codex::CodexExecRunner;

use crate::main_support::{codex_request_and_cwd, rendered_or_default};

pub(crate) use crate::main_runtime_handlers::{
    handle_listener_item, handle_next_listener_event, handle_prompt_ready, handle_runtime_outcome,
    handle_update,
};

pub(crate) struct BotTopicManager {
    pub(crate) bot: Bot,
}

impl TopicManager for BotTopicManager {
    fn create_topic(&self, chat_id: i64, title: &str) -> Result<i64, AppServiceError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.bot
                    .create_forum_topic(ChatId(chat_id), title.to_owned())
                    .send()
                    .await
                    .map(|topic| i64::from(topic.thread_id.0.0))
                    .map_err(|err| AppServiceError::Topic(err.to_string()))
            })
        })
    }
}

pub(crate) struct CodexPromptExecutor<'a> {
    pub(crate) runner: &'a CodexExecRunner,
}

impl TelegramPromptExecutor for CodexPromptExecutor<'_> {
    fn execute_prompt(
        &self,
        binding: &vibes_core::SessionBinding,
        prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        let (request, cwd) = codex_request_and_cwd(binding, prompt);
        let result = self.runner.run(&request, &cwd).map_err(|err| {
            TelegramExecutionError::new(format!("codex prompt execution failed: {err}"))
        })?;
        Ok(rendered_or_default(result.transcript.rendered()))
    }
}

pub(crate) async fn run_polling_loop_with_shutdown<S, F, E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    stream: S,
    shutdown: F,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    S: tokio_stream::Stream<Item = Result<teloxide::types::Update, teloxide::RequestError>>,
    F: core::future::Future<Output = ()>,
    E: TelegramPromptExecutor,
{
    pin!(shutdown);
    pin!(stream);
    loop {
        tokio::select! {
            () = &mut shutdown => {
                info!("ctrl-c received, stopping polling loop");
                break;
            }
            update = stream.next() => {
                if !handle_next_listener_event(controller, bot, executor, update, bot_username, workspace_root).await {
                    break;
                }
            }
        }
    }
    info!("vibes polling loop stopped");
}

pub(crate) async fn run_polling_loop<S, E>(
    controller: &AppController<
        vibes_store::SqliteBindingStore,
        vibes_codex::CodexExecRunner,
        BotTopicManager,
    >,
    bot: &Bot,
    executor: &E,
    stream: S,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    S: tokio_stream::Stream<Item = Result<teloxide::types::Update, teloxide::RequestError>>,
    E: TelegramPromptExecutor,
{
    run_polling_loop_with_shutdown(
        controller,
        bot,
        executor,
        stream,
        async {
            let _ = tokio::signal::ctrl_c().await;
        },
        bot_username,
        workspace_root,
    )
    .await;
}
