use teloxide::prelude::Bot;
use tracing::info;
use vibes_app::TelegramPromptExecutor;

use crate::main_runtime_components::RuntimeController;
use crate::main_runtime_handlers::handle_listener_item;

pub(crate) async fn handle_next_listener_event<E>(
    controller: &RuntimeController,
    bot: &Bot,
    executor: &E,
    update: Option<Result<teloxide::types::Update, teloxide::RequestError>>,
    bot_username: Option<&str>,
    workspace_root: &str,
) -> bool
where
    E: TelegramPromptExecutor,
{
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
