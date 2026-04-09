use vibes_app::{TelegramExecutionError, TelegramPromptExecutor};
use vibes_codex::CodexExecRunner;

use crate::main_support::{codex_request_and_cwd, rendered_or_default};

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
