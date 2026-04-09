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
    TelegramPromptExecutor, TelegramRequester, TopicManager, complete_runtime_outcome,
    run_telegram_update,
};
use vibes_codex::CodexExecRunner;
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

mod main_support;

use main_support::{bot_username, codex_request_and_cwd, rendered_or_default, runtime_paths};

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

async fn handle_prompt_ready<Q, E>(
    requester: &Q,
    executor: &E,
    target: vibes_telegram::ReplyTarget,
    binding: vibes_core::SessionBinding,
    prompt: String,
) where
    Q: TelegramRequester,
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

async fn handle_runtime_outcome(
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    outcome: RuntimeOutcome,
) {
    match outcome {
        RuntimeOutcome::Ignored => {}
        RuntimeOutcome::Replied { target, .. } => {
            info!(
                chat_id = target.chat_id,
                thread_id = target.message_thread_id,
                "reply sent"
            );
        }
        RuntimeOutcome::PromptReady {
            target,
            binding,
            prompt,
        } => {
            handle_prompt_ready(bot, executor, target, binding, prompt).await;
        }
    }
}

async fn handle_update(
    controller: &AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    update: teloxide::types::Update,
    bot_username: Option<&str>,
    workspace_root: &str,
) {
    match run_telegram_update(controller, bot, &update, bot_username, workspace_root).await {
        Ok(outcome) => {
            handle_runtime_outcome(bot, executor, outcome).await;
        }
        Err(err) => {
            error!(error = %err, "failed to handle telegram update");
        }
    }
}

async fn handle_listener_item(
    controller: &AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    update: Result<teloxide::types::Update, teloxide::RequestError>,
    bot_username: Option<&str>,
    workspace_root: &str,
) {
    match update {
        Ok(update) => {
            handle_update(
                controller,
                bot,
                executor,
                update,
                bot_username,
                workspace_root,
            )
            .await;
        }
        Err(err) => error!(error = ?err, "polling listener error"),
    }
}

fn build_runtime_components(
    bot: &Bot,
    db_path: &str,
) -> Result<
    (
        AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
        CodexExecRunner,
    ),
    anyhow::Error,
> {
    let store = SqliteBindingStore::open(db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))?;
    let runtime = CodexExecRunner::default();
    let topics = BotTopicManager { bot: bot.clone() };
    let controller = AppController::new(AppService::new(store, runtime.clone(), topics));
    Ok((controller, runtime))
}

fn startup_context_from_parts(
    bot: Bot,
    user: &teloxide::types::User,
    workspace_root_override: Option<String>,
    db_path_override: Option<String>,
) -> (Bot, Option<String>, String, String) {
    let bot_username = bot_username(user);
    let (workspace_root, db_path) = runtime_paths(workspace_root_override, db_path_override);
    (bot, bot_username, workspace_root, db_path)
}

async fn build_startup_context() -> anyhow::Result<(Bot, Option<String>, String, String)> {
    let bot = Bot::from_env();
    let me = bot.get_me().send().await.context("get_me failed")?;
    Ok(startup_context_from_parts(
        bot,
        &me.user,
        env::var("VIBES_WORKSPACE_ROOT").ok(),
        env::var("VIBES_DB_PATH").ok(),
    ))
}

async fn handle_next_listener_event(
    controller: &AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    update: Option<Result<teloxide::types::Update, teloxide::RequestError>>,
    bot_username: Option<&str>,
    workspace_root: &str,
) -> bool {
    let Some(update) = update else {
        info!("polling listener stream ended");
        return false;
    };

    handle_listener_item(
        controller,
        bot,
        executor,
        update,
        bot_username,
        workspace_root,
    )
    .await;

    true
}

async fn run_polling_loop<S>(
    controller: &AppController<SqliteBindingStore, CodexExecRunner, BotTopicManager>,
    bot: &Bot,
    executor: &impl TelegramPromptExecutor,
    stream: S,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    S: tokio_stream::Stream<Item = Result<teloxide::types::Update, teloxide::RequestError>>,
{
    pin!(stream);

    let shutdown = tokio::signal::ctrl_c();
    pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("ctrl-c received, stopping polling loop");
                break;
            }
            update = stream.next() => {
                if !handle_next_listener_event(
                    controller,
                    bot,
                    executor,
                    update,
                    bot_username,
                    workspace_root,
                ).await {
                    break;
                }
            }
        }
    }

    info!("vibes polling loop stopped");
}

#[cfg(test)]
mod tests {
    use super::{
        build_runtime_components, handle_listener_item, handle_next_listener_event,
        handle_prompt_ready, handle_runtime_outcome, handle_update, run_polling_loop,
        startup_context_from_parts,
    };
    use std::{
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };
    use teloxide::types::User;
    use teloxide::{ApiError, Bot, RequestError, types::Update};
    use tokio_stream::iter;
    use vibes_app::{
        RuntimeOutcome, TelegramExecutionError, TelegramPromptExecutor, TelegramRequestError,
        TelegramRequester,
    };

    struct NoopExecutor;

    struct RecordingRequester {
        sent: Mutex<Vec<(vibes_telegram::ReplyTarget, String)>>,
        fail: Mutex<Option<String>>,
    }

    #[async_trait::async_trait(?Send)]
    impl TelegramRequester for RecordingRequester {
        async fn send_text(
            &self,
            target: &vibes_telegram::ReplyTarget,
            text: &str,
        ) -> Result<(), TelegramRequestError> {
            if let Some(message) = self.fail.lock().unwrap().clone() {
                return Err(TelegramRequestError::new(message));
            }
            self.sent
                .lock()
                .unwrap()
                .push((target.clone(), text.to_owned()));
            Ok(())
        }
    }

    struct PanicExecutor;

    impl TelegramPromptExecutor for PanicExecutor {
        fn execute_prompt(
            &self,
            _binding: &vibes_core::SessionBinding,
            _prompt: &str,
        ) -> Result<String, TelegramExecutionError> {
            panic!("executor should not run");
        }
    }

    impl TelegramPromptExecutor for NoopExecutor {
        fn execute_prompt(
            &self,
            _binding: &vibes_core::SessionBinding,
            _prompt: &str,
        ) -> Result<String, TelegramExecutionError> {
            Ok("noop".to_owned())
        }
    }

    struct RecordingExecutor {
        seen: Mutex<Vec<(String, String, String)>>,
        response: String,
    }

    impl TelegramPromptExecutor for RecordingExecutor {
        fn execute_prompt(
            &self,
            binding: &vibes_core::SessionBinding,
            prompt: &str,
        ) -> Result<String, TelegramExecutionError> {
            self.seen.lock().unwrap().push((
                binding.workspace_root.clone(),
                binding.session.codex_session_id.clone(),
                prompt.to_owned(),
            ));
            Ok(self.response.clone())
        }
    }

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_ignored_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;

        handle_runtime_outcome(&bot, &executor, RuntimeOutcome::Ignored).await;
    }

    #[tokio::test]
    async fn handle_runtime_outcome_keeps_replied_without_executor_use() {
        let bot = Bot::new("123456:TESTTOKEN");
        let executor = PanicExecutor;
        let outcome = RuntimeOutcome::Replied {
            target: vibes_telegram::ReplyTarget {
                chat_id: 408258968,
                message_thread_id: None,
            },
            text: "already replied".to_owned(),
        };

        handle_runtime_outcome(&bot, &executor, outcome).await;
    }

    #[tokio::test]
    async fn handle_update_processes_new_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 996,
                "message": {
                    "message_id": 774,
                    "date": 1710001108,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_prompt_ready_passes_binding_and_prompt_to_executor() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        };
        let executor = RecordingExecutor {
            seen: Mutex::new(Vec::new()),
            response: "recorded".to_owned(),
        };
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target.clone(),
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let seen = executor.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "/workspace");
        assert_eq!(seen[0].1, "codex-1");
        assert_eq!(seen[0].2, "continue parser work");
        drop(seen);

        let sent = requester.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, target);
        assert_eq!(sent[0].1, "recorded");
    }

    #[tokio::test]
    async fn handle_prompt_ready_sends_reply_on_success() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
        };
        let executor = NoopExecutor;
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target.clone(),
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let sent = requester.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, target);
        assert_eq!(sent[0].1, "noop");
    }

    #[tokio::test]
    async fn handle_prompt_ready_ignores_completion_error_without_panicking() {
        let requester = RecordingRequester {
            sent: Mutex::new(Vec::new()),
            fail: Mutex::new(Some("send boom".to_owned())),
        };
        let executor = NoopExecutor;
        let target = vibes_telegram::ReplyTarget {
            chat_id: 408258968,
            message_thread_id: None,
        };
        let binding = vibes_core::SessionBinding {
            scope: vibes_core::ChatScope::Direct(408258968),
            workspace_root: "/workspace".to_owned(),
            session: vibes_core::SessionHandle {
                codex_session_id: "codex-1".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        handle_prompt_ready(
            &requester,
            &executor,
            target,
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        assert!(requester.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle_update_processes_new_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "date": 1710001109,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_new_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "message_thread_id": null,
                    "date": 1710001109,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_new_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 998,
                "message": {
                    "message_id": 776,
                    "message_thread_id": null,
                    "date": 1710001110,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_new_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1001,
                "message": {
                    "message_id": 779,
                    "message_thread_id": 900,
                    "date": 1710001113,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_new_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1002,
                "message": {
                    "message_id": 780,
                    "message_thread_id": 900,
                    "date": 1710001114,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_resume_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 999,
                "message": {
                    "message_id": 777,
                    "message_thread_id": null,
                    "date": 1710001111,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_forum_root_resume_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1000,
                "message": {
                    "message_id": 778,
                    "message_thread_id": null,
                    "date": 1710001112,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_resume_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1003,
                "message": {
                    "message_id": 781,
                    "message_thread_id": 900,
                    "date": 1710001115,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_resume_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1004,
                "message": {
                    "message_id": 782,
                    "message_thread_id": 900,
                    "date": 1710001116,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_resume_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 995,
                "message": {
                    "message_id": 773,
                    "date": 1710001107,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_resume_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 994,
                "message": {
                    "message_id": 772,
                    "date": 1710001106,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_ignores_non_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1001,
                "callback_query": {
                    "id": "cb-main-2",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-2",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1005,
                "message": {
                    "message_id": 783,
                    "message_thread_id": 900,
                    "date": 1710001117,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser work"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 998,
                "message": {
                    "message_id": 776,
                    "date": 1710001110,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_topic_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1006,
                "message": {
                    "message_id": 784,
                    "message_thread_id": 900,
                    "date": 1710001118,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from topic caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_processes_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 997,
                "message": {
                    "message_id": 775,
                    "date": 1710001109,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "caption only"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_update_ignores_runtime_error_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 999,
                "message": {
                    "message_id": 777,
                    "date": 1710001111,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "message_thread_id": 900,
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello without binding"
                }
            }"#,
        )
        .unwrap();

        handle_update(&controller, &bot, &executor, update, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_ignores_non_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1000,
                "callback_query": {
                    "id": "cb-main-1",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-1",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_new_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1015,
                "message": {
                    "message_id": 793,
                    "message_thread_id": 900,
                    "date": 1710001127,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1007,
                "message": {
                    "message_id": 785,
                    "message_thread_id": 900,
                    "date": 1710001119,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser from topic"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_resume_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1014,
                "message": {
                    "message_id": 792,
                    "date": 1710001126,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/resume rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq2",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_resume_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1013,
                "message": {
                    "message_id": 791,
                    "date": 1710001125,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/resume rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 7
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_new_command_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1011,
                "message": {
                    "message_id": 789,
                    "date": 1710001123,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "/new rust-rewrite",
                    "entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1009,
                "message": {
                    "message_id": 787,
                    "date": 1710001121,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "continue parser from forum root"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_message_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1002,
                "message": {
                    "message_id": 778,
                    "date": 1710002222,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_topic_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1008,
                "message": {
                    "message_id": 786,
                    "message_thread_id": 900,
                    "date": 1710001120,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from topic caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_new_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1012,
                "message": {
                    "message_id": 790,
                    "date": 1710001124,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "/new rust-rewrite",
                    "caption_entities": [{
                        "type": "bot_command",
                        "offset": 0,
                        "length": 4
                    }],
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_forum_root_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1010,
                "message": {
                    "message_id": 788,
                    "date": 1710001122,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "Vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "continue parser from forum root caption",
                    "photo": [{
                        "file_id": "abc",
                        "file_unique_id": "uq1",
                        "width": 100,
                        "height": 100
                    }]
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_processes_caption_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1003,
                "message": {
                    "message_id": 779,
                    "date": 1710003333,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "caption only"
                }
            }"#,
        )
        .unwrap();

        handle_listener_item(&controller, &bot, &executor, Ok(update), None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_listener_item_ignores_request_error_without_executor_use() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = PanicExecutor;

        handle_listener_item(
            &controller,
            &bot,
            &executor,
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            None,
            "/workspace",
        )
        .await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[test]
    fn build_runtime_components_creates_sqlite_store_at_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn build_runtime_components_reopens_existing_sqlite_store() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path =
            std::env::temp_dir().join(format!("vibes-build-runtime-reopen-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let _ = build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();

        assert!(db_path.exists());
        std::fs::remove_file(db_path).unwrap();
    }

    #[test]
    fn startup_context_from_parts_preserves_missing_bot_username() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: User = serde_json::from_str(
            r#"{
                "id": 777,
                "is_bot": true,
                "first_name": "vibes-bot"
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace/custom".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(bot_username, None);
        assert_eq!(workspace_root, "/workspace/custom");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn startup_context_from_parts_normalizes_empty_bot_username() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: User = serde_json::from_str(
            r#"{
                "id": 777,
                "is_bot": true,
                "first_name": "vibes-bot",
                "username": ""
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace/custom".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(bot_username, None);
        assert_eq!(workspace_root, "/workspace/custom");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn startup_context_from_parts_preserves_bot_username_and_runtime_paths() {
        let bot = Bot::new("123456:TESTTOKEN");
        let user: teloxide::types::User = serde_json::from_str(
            r#"{
                "id": 408258968,
                "is_bot": true,
                "first_name": "VibesBot",
                "username": "vibes_bot"
            }"#,
        )
        .unwrap();

        let (_bot, bot_username, workspace_root, db_path) = startup_context_from_parts(
            bot,
            &user,
            Some("/workspace".to_owned()),
            Some("/tmp/vibes.sqlite3".to_owned()),
        );

        assert_eq!(bot_username.as_deref(), Some("vibes_bot"));
        assert_eq!(workspace_root, "/workspace");
        assert_eq!(db_path, "/tmp/vibes.sqlite3");
    }

    #[test]
    fn build_runtime_components_returns_error_for_invalid_db_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir()
            .join(format!("vibes-build-runtime-missing-{unique}"))
            .join("nested")
            .join("vibes.sqlite3");

        let bot = Bot::new("123456:TESTTOKEN");
        let path_string = db_path.to_str().unwrap().to_owned();
        let rendered = match build_runtime_components(&bot, &path_string) {
            Ok(_) => panic!("expected sqlite open failure"),
            Err(err) => err.to_string(),
        };

        assert!(rendered.contains("failed to open sqlite store"));
        assert!(rendered.contains(&path_string));
    }

    #[tokio::test]
    async fn handle_next_listener_event_returns_false_for_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;

        let keep_running =
            handle_next_listener_event(&controller, &bot, &executor, None, None, "/workspace")
                .await;

        assert!(!keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_non_message_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 1,
                "callback_query": {
                    "id": "cbq-1",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance-1",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_request_error() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let request_error = RequestError::Api(ApiError::Unknown("bad request".to_owned()));

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Err(request_error)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_message_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 2,
                "message": {
                    "message_id": 10,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_caption_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 21,
                "message": {
                    "message_id": 13,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello caption"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_message_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 3,
                "message": {
                    "message_id": 11,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello topic"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_caption_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 4,
                "message": {
                    "message_id": 12,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello topic caption"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_message_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 5,
                "message": {
                    "message_id": 14,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello forum root"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_caption_update() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 6,
                "message": {
                    "message_id": 15,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello forum root caption"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_new_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 7,
                "message": {
                    "message_id": 16,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_new_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 8,
                "message": {
                    "message_id": 17,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_new_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 9,
                "message": {
                    "message_id": 18,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_new_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 10,
                "message": {
                    "message_id": 19,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_resume_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 11,
                "message": {
                    "message_id": 20,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_direct_resume_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 12,
                "message": {
                    "message_id": 21,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_resume_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 13,
                "message": {
                    "message_id": 22,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_forum_root_resume_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 14,
                "message": {
                    "message_id": 23,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_new_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 15,
                "message": {
                    "message_id": 24,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_new_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 16,
                "message": {
                    "message_id": 25,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 4, "type": "bot_command" }
                    ],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_resume_command() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 17,
                "message": {
                    "message_id": 26,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn handle_next_listener_event_keeps_running_for_topic_resume_caption() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 18,
                "message": {
                    "message_id": 27,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [
                        { "offset": 0, "length": 8, "type": "bot_command" }
                    ],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let keep_running = handle_next_listener_event(
            &controller,
            &bot,
            &executor,
            Some(Ok(update)),
            None,
            "/workspace",
        )
        .await;

        assert!(keep_running);
        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_returns_immediately_on_empty_stream() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let stream = iter(Vec::<Result<Update, RequestError>>::new());

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_processes_event_and_returns_on_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 19,
                "message": {
                    "message_id": 28,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_processes_multiple_events_before_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update1: Update = serde_json::from_str(
            r#"{
                "update_id": 100,
                "message": {
                    "message_id": 200,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();
        let update2: Update = serde_json::from_str(
            r#"{
                "update_id": 101,
                "callback_query": {
                    "id": "cb-1",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-1",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let update3: Update = serde_json::from_str(
            r#"{
                "update_id": 102,
                "message": {
                    "message_id": 201,
                    "date": 1710000001,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hi caption"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![Ok(update1), Ok(update2), Ok(update3)]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_request_error_then_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 103,
                "message": {
                    "message_id": 202,
                    "date": 1710000002,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello after error"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            Ok(update),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_message_then_request_error_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 104,
                "message": {
                    "message_id": 203,
                    "date": 1710000003,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello before error"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![
            Ok(update),
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_request_error_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let request_error = RequestError::Api(ApiError::Unknown("bad request".to_owned()));
        let stream = iter(vec![Err(request_error)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_request_error_then_non_message_until_stream_end()
     {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 102,
                "callback_query": {
                    "id": "abc123",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
            Ok(update),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_message_then_non_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let message: Update = serde_json::from_str(
            r#"{
                "update_id": 103,
                "message": {
                    "message_id": 11,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();
        let non_message: Update = serde_json::from_str(
            r#"{
                "update_id": 104,
                "callback_query": {
                    "id": "abc123",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![Ok(message), Ok(non_message)]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_non_message_then_request_error_until_stream_end()
     {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let callback: Update = serde_json::from_str(
            r#"{
                "update_id": 1009,
                "callback_query": {
                    "id": "cb-main-9",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "ci-main-9",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let stream = tokio_stream::iter(vec![
            Ok(callback),
            Err(RequestError::Api(ApiError::Unknown("boom".to_owned()))),
        ]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_non_message_then_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let non_message: Update = serde_json::from_str(
            r#"{
                "update_id": 102,
                "callback_query": {
                    "id": "abc123",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();
        let message: Update = serde_json::from_str(
            r#"{
                "update_id": 103,
                "message": {
                    "message_id": 11,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello"
                }
            }"#,
        )
        .unwrap();
        let stream = iter(vec![Ok(non_message), Ok(message)]);

        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_non_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 20,
                "callback_query": {
                    "id": "cbq-2",
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "chat_instance": "instance-2",
                    "data": "noop"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_caption_update_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 21,
                "message": {
                    "message_id": 29,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello from caption"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_message_update_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 22,
                "message": {
                    "message_id": 30,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello from text"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 23,
                "message": {
                    "message_id": 31,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello from topic text"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 24,
                "message": {
                    "message_id": 32,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello from topic caption"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_message_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 25,
                "message": {
                    "message_id": 33,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "text": "hello from forum root text"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 26,
                "message": {
                    "message_id": 34,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption": "hello from forum root caption"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_direct_new_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 27,
                "message": {
                    "message_id": 35,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_direct_new_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 28,
                "message": {
                    "message_id": 36,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_new_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 29,
                "message": {
                    "message_id": 37,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_new_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 30,
                "message": {
                    "message_id": 38,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_direct_resume_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 31,
                "message": {
                    "message_id": 39,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_direct_resume_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 32,
                "message": {
                    "message_id": 40,
                    "date": 1710000000,
                    "chat": {
                        "id": 408258968,
                        "type": "private",
                        "first_name": "Bakhtier"
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_resume_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 33,
                "message": {
                    "message_id": 41,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_forum_root_resume_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 34,
                "message": {
                    "message_id": 42,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_new_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 35,
                "message": {
                    "message_id": 43,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "text": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_new_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 36,
                "message": {
                    "message_id": 44,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 4,
                        "type": "bot_command"
                    }],
                    "caption": "/new rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_resume_command_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 37,
                "message": {
                    "message_id": 45,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "text": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
    }

    #[tokio::test]
    async fn run_polling_loop_keeps_running_through_topic_resume_caption_until_stream_end() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("vibes-build-runtime-{unique}.sqlite3"));
        if db_path.exists() {
            std::fs::remove_file(&db_path).unwrap();
        }

        let bot = Bot::new("123456:TESTTOKEN");
        let (controller, _runtime) =
            build_runtime_components(&bot, db_path.to_str().unwrap()).unwrap();
        let executor = NoopExecutor;
        let update: Update = serde_json::from_str(
            r#"{
                "update_id": 38,
                "message": {
                    "message_id": 46,
                    "message_thread_id": 900,
                    "date": 1710000000,
                    "chat": {
                        "id": -1001293752024,
                        "type": "supergroup",
                        "title": "vibes",
                        "is_forum": true
                    },
                    "from": {
                        "id": 408258968,
                        "is_bot": false,
                        "first_name": "Bakhtier"
                    },
                    "caption_entities": [{
                        "offset": 0,
                        "length": 7,
                        "type": "bot_command"
                    }],
                    "caption": "/resume rust-rewrite"
                }
            }"#,
        )
        .unwrap();

        let stream = iter(vec![Ok(update)]);
        run_polling_loop(&controller, &bot, &executor, stream, None, "/workspace").await;

        if db_path.exists() {
            std::fs::remove_file(db_path).unwrap();
        }
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

    let (bot, bot_username, workspace_root, db_path) = build_startup_context().await?;

    let (controller, runtime) = build_runtime_components(&bot, &db_path)?;
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
