use std::time::{Duration, Instant};

use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::{Request, Requester};
use teloxide::types::{ChatId, MessageId, ThreadId};
use tracing::error;
use vibes_app::{TelegramExecutionError, TelegramPromptExecutor};
use vibes_codex::CodexExecRunner;
use vibes_codex::event::{CodexEvent, ParsedCodexLine};

use crate::main_support::{codex_request_and_cwd, rendered_or_default};

pub(crate) struct CodexPromptExecutor<'a> {
    pub(crate) runner: &'a CodexExecRunner,
    pub(crate) bot: teloxide::prelude::Bot,
}

/// Run an async future from sync context without deadlocking tokio.
/// Spawns a dedicated thread with its own runtime to avoid nested runtime panic.
fn run_async<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create one-shot runtime");
        let result = rt.block_on(fut);
        let _ = tx.send(result);
    });
    rx.recv().expect("async helper thread panicked")
}

impl TelegramPromptExecutor for CodexPromptExecutor<'_> {
    fn execute_prompt(
        &self,
        binding: &vibes_core::SessionBinding,
        prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        let (request, cwd) = codex_request_and_cwd(binding, prompt);

        // Send initial "thinking" message
        let chat_id = extract_chat_id_from_scope(&binding.scope);
        let thread_id = extract_thread_id_from_scope(&binding.scope);

        let edit_ctx = chat_id.and_then(|cid| {
            let bot = self.bot.clone();
            run_async(async move {
                let mut send = bot.send_message(ChatId(cid), "⏳ Processing...");
                if let Some(tid) = thread_id {
                    send = send.message_thread_id(ThreadId(MessageId(tid)));
                }
                match send.send().await {
                    Ok(msg) => Some(EditContext {
                        chat_id: cid,
                        message_id: msg.id,
                    }),
                    Err(err) => {
                        error!(error = ?err, "failed to send streaming message");
                        None
                    }
                }
            })
        });

        // Run codex with streaming handler
        let bot = self.bot.clone();
        let mut accumulated = String::new();
        let mut last_edit = Instant::now();
        let edit_interval = Duration::from_millis(2000);

        let result = self
            .runner
            .run_with_handler(
                &request,
                &cwd,
                &vibes_codex::CodexRunControl::default(),
                |line| {
                    if let Some(text) = format_event_line(line) {
                        if !accumulated.is_empty() {
                            accumulated.push('\n');
                        }
                        accumulated.push_str(&text);

                        // Throttle edits to avoid Telegram rate limits
                        #[allow(clippy::collapsible_if)]
                        if let Some(ctx) = &edit_ctx {
                            if last_edit.elapsed() >= edit_interval {
                                let display = truncate_for_telegram(&accumulated);
                                let bot_ref = bot.clone();
                                let cid = ChatId(ctx.chat_id);
                                let mid = ctx.message_id;
                                let _ = run_async(async move {
                                    bot_ref.edit_message_text(cid, mid, display).send().await
                                });
                                last_edit = Instant::now();
                            }
                        }
                    }
                },
            )
            .map_err(|err| {
                // Edit message with error
                if let Some(ctx) = &edit_ctx {
                    let error_text = format!("❌ {err}");
                    let bot_ref = bot.clone();
                    let cid = ChatId(ctx.chat_id);
                    let mid = ctx.message_id;
                    let _ = run_async(async move {
                        bot_ref.edit_message_text(cid, mid, error_text).send().await
                    });
                }
                TelegramExecutionError::new(format!("codex prompt execution failed: {err}"))
            })?;

        let rendered = rendered_or_default(result.transcript.rendered());

        // Final edit with complete result
        if let Some(ctx) = &edit_ctx {
            let final_text = truncate_for_telegram(&rendered);
            let cid = ChatId(ctx.chat_id);
            let mid = ctx.message_id;
            let _ =
                run_async(async move { bot.edit_message_text(cid, mid, final_text).send().await });
        }

        Ok(rendered)
    }
}

struct EditContext {
    chat_id: i64,
    message_id: MessageId,
}

fn extract_chat_id_from_scope(scope: &vibes_core::ChatScope) -> Option<i64> {
    match scope {
        vibes_core::ChatScope::Direct(id) | vibes_core::ChatScope::Group(id) => Some(*id),
        vibes_core::ChatScope::Topic { chat_id, .. } => Some(*chat_id),
    }
}

fn extract_thread_id_from_scope(scope: &vibes_core::ChatScope) -> Option<i32> {
    match scope {
        vibes_core::ChatScope::Topic { topic_id, .. } => Some(*topic_id as i32),
        _ => None,
    }
}

fn format_event_line(line: &ParsedCodexLine) -> Option<String> {
    match line {
        ParsedCodexLine::Event(event) => match event {
            CodexEvent::AssistantMessage { text } | CodexEvent::TextDelta { text } => {
                Some(format!("💬 {text}"))
            }
            CodexEvent::CommandStarted { command } => Some(format!("⚡ $ {command}")),
            CodexEvent::CommandFinished {
                command, output, ..
            } => {
                let mut s = format!("✅ $ {command}");
                if let Some(out) = output {
                    let trimmed = out.trim();
                    if !trimmed.is_empty() {
                        let preview = if trimmed.len() > 300 {
                            format!("{}...", &trimmed[..300])
                        } else {
                            trimmed.to_owned()
                        };
                        s.push_str(&format!("\n{preview}"));
                    }
                }
                Some(s)
            }
            CodexEvent::Reasoning => Some("🧠 Thinking...".to_owned()),
            CodexEvent::TurnStarted => Some("🔄 Processing...".to_owned()),
            _ => None,
        },
        ParsedCodexLine::Noise(_) => None,
    }
}

fn truncate_for_telegram(text: &str) -> String {
    if text.len() <= 4000 {
        text.to_owned()
    } else {
        format!("{}...\n\n(truncated)", &text[..3990])
    }
}
