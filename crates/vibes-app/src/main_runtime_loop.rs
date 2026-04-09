use teloxide::prelude::Bot;
use teloxide::update_listeners;
use teloxide::update_listeners::AsUpdateStream;
use tokio::pin;
use tokio_stream::StreamExt;
use tracing::info;
use vibes_app::TelegramPromptExecutor;

use crate::main_runtime_components::RuntimeController;
use crate::main_runtime_handlers::handle_next_listener_event;

pub(crate) async fn run_polling_loop_with_shutdown<S, F, E>(
    controller: &RuntimeController,
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
    controller: &RuntimeController,
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

pub(crate) async fn start_polling_loop<E>(
    controller: &RuntimeController,
    bot: &Bot,
    executor: &E,
    bot_username: Option<&str>,
    workspace_root: &str,
) where
    E: TelegramPromptExecutor,
{
    let mut listener = update_listeners::polling_default(bot.clone()).await;
    let stream = listener.as_stream();

    info!(bot_username = ?bot_username, workspace_root, "vibes polling loop started");

    run_polling_loop(
        controller,
        bot,
        executor,
        stream,
        bot_username,
        workspace_root,
    )
    .await;
}
