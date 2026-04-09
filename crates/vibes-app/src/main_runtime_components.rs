use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
};
use vibes_app::{AppServiceError, TelegramExecutionError, TelegramPromptExecutor, TopicManager};
use vibes_codex::CodexExecRunner;

use crate::main_support::{codex_request_and_cwd, rendered_or_default};

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
