use std::env;

use anyhow::Context;
use teloxide::{
    prelude::{Bot, Request, Requester},
    types::ChatId,
    update_listeners::{self, AsUpdateStream},
};
use tokio::pin;
use tokio_stream::StreamExt;
use tracing::{error, info};
use vibes_app::{
    AppController, AppService, AppServiceError, RuntimeOutcome, TelegramExecutionError,
    TelegramPromptExecutor, TopicManager, complete_runtime_outcome, run_telegram_update,
};
use vibes_codex::{CodexExecRunner, CodexRunRequest};
use vibes_store::SqliteBindingStore;

struct BotTopicManager {
    bot: Bot,
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

struct CodexPromptExecutor<'a> {
    runner: &'a CodexExecRunner,
}

fn codex_request_and_cwd(
    binding: &vibes_core::SessionBinding,
    prompt: &str,
) -> (CodexRunRequest, std::path::PathBuf) {
    (
        CodexRunRequest {
            prompt: prompt.to_owned(),
            resume_target: Some(binding.session.codex_session_id.clone()),
        },
        std::path::PathBuf::from(&binding.workspace_root),
    )
}

fn rendered_or_default(rendered: String) -> String {
    if rendered.trim().is_empty() {
        "Codex run completed with no transcript output.".to_owned()
    } else {
        rendered
    }
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

#[cfg(test)]
mod tests {
    use super::{codex_request_and_cwd, rendered_or_default};
    use vibes_core::{ChatScope, SessionBinding, SessionHandle};

    #[test]
    fn codex_request_and_cwd_preserves_binding_session_and_workspace() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let (request, cwd) = codex_request_and_cwd(&binding, "continue parser work");

        assert_eq!(request.prompt, "continue parser work");
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_prompt_whitespace() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let prompt = "  continue parser work  ";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_topic_binding_session_and_workspace() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let (request, cwd) = codex_request_and_cwd(&binding, "continue topic parser work");

        assert_eq!(request.prompt, "continue topic parser work");
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_topic_binding_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let prompt = "step 1\nstep 2\nfinish topic parser";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let prompt = "step 1\nstep 2\nfinal line";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_blank_transcript() {
        assert_eq!(
            rendered_or_default("   \n\t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_crlf_only_transcript() {
        assert_eq!(
            rendered_or_default("\r\n \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_cr_only_transcript() {
        assert_eq!(
            rendered_or_default("\r   \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_keeps_non_empty_transcript() {
        assert_eq!(
            rendered_or_default("done transcript".to_owned()),
            "done transcript"
        );
    }

    #[test]
    fn rendered_or_default_preserves_multiline_transcript() {
        let rendered = "step 1\nstep 2\nfinal line".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_crlf_prefixed_non_empty_transcript() {
        let rendered = "\r\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_cr_prefixed_non_empty_transcript() {
        let rendered = "\rstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_lf_prefixed_non_empty_transcript() {
        let rendered = "\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_tab_prefixed_non_empty_transcript() {
        let rendered = "\tstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_keeps_padded_non_empty_transcript() {
        let rendered = "  done transcript  ".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }
}

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

    let bot = Bot::from_env();
    let me = bot.get_me().send().await.context("get_me failed")?;
    let bot_username = me.user.username.clone();
    let workspace_root = env::var("VIBES_WORKSPACE_ROOT").unwrap_or_else(|_| ".".to_owned());
    let db_path = env::var("VIBES_DB_PATH").unwrap_or_else(|_| "vibes.sqlite3".to_owned());

    let store = SqliteBindingStore::open(&db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager { bot: bot.clone() };
    let controller = AppController::new(AppService::new(store, runtime.clone(), topics));
    let executor = CodexPromptExecutor { runner: &runtime };

    let mut listener = update_listeners::polling_default(bot.clone()).await;
    let stream = listener.as_stream();
    pin!(stream);

    info!(bot_username = ?bot_username, db_path, workspace_root, "vibes polling loop started");

    let shutdown = tokio::signal::ctrl_c();
    pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("ctrl-c received, stopping polling loop");
                break;
            }
            update = stream.next() => {
                let Some(update) = update else {
                    info!("polling listener stream ended");
                    break;
                };

                match update {
                    Ok(update) => match run_telegram_update(
                        &controller,
                        &bot,
                        &update,
                        bot_username.as_deref(),
                        &workspace_root,
                    )
                    .await
                    {
                        Ok(RuntimeOutcome::Ignored) => {}
                        Ok(RuntimeOutcome::Replied { target, .. }) => {
                            info!(
                                chat_id = target.chat_id,
                                thread_id = target.message_thread_id,
                                "reply sent"
                            );
                        }
                        Ok(RuntimeOutcome::PromptReady {
                            target,
                            binding,
                            prompt,
                        }) => {
                            info!(
                                chat_id = target.chat_id,
                                thread_id = target.message_thread_id,
                                scope = %binding.scope.scope_key(),
                                session_id = %binding.session.codex_session_id,
                                prompt_len = prompt.len(),
                                "prompt ready for codex execution"
                            );
                            match complete_runtime_outcome(
                                &bot,
                                &executor,
                                RuntimeOutcome::PromptReady {
                                    target,
                                    binding,
                                    prompt,
                                },
                            )
                            .await
                            {
                                Ok(RuntimeOutcome::Replied { target, .. }) => {
                                    info!(
                                        chat_id = target.chat_id,
                                        thread_id = target.message_thread_id,
                                        "codex execution reply sent"
                                    );
                                }
                                Ok(other) => {
                                    info!(outcome = ?other, "unexpected runtime completion outcome");
                                }
                                Err(err) => {
                                    error!(error = %err, "failed to complete codex execution outcome");
                                }
                            }
                        }
                        Err(err) => {
                            error!(error = %err, "failed to handle telegram update");
                        }
                    },
                    Err(err) => error!(error = ?err, "polling listener error"),
                }
            }
        }
    }

    info!("vibes polling loop stopped");
    Ok(())
}
