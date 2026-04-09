use std::env;

use anyhow::Context;
use teloxide::{
    prelude::{Bot, Request, Requester},
    types::User,
};

use crate::main_support::{bot_username, runtime_paths};
use vibes_store::SqliteBindingStore;

pub(crate) async fn build_startup_context() -> anyhow::Result<(Bot, Option<String>, String, String)> {
    let bot = Bot::from_env();
    startup_context_from_get_me(
        bot.clone(),
        bot.get_me().send().await,
        env::var("VIBES_WORKSPACE_ROOT").ok(),
        env::var("VIBES_DB_PATH").ok(),
    )
    .await
}

pub(crate) async fn startup_context_from_get_me(
    bot: Bot,
    me_result: Result<teloxide::types::Me, teloxide::RequestError>,
    workspace_root_override: Option<String>,
    db_path_override: Option<String>,
) -> anyhow::Result<(Bot, Option<String>, String, String)> {
    let me = me_result.context("get_me failed")?;
    Ok(startup_context_from_parts(
        bot,
        &me.user,
        workspace_root_override,
        db_path_override,
    ))
}

pub(crate) fn startup_context_from_parts(
    bot: Bot,
    user: &User,
    workspace_root_override: Option<String>,
    db_path_override: Option<String>,
) -> (Bot, Option<String>, String, String) {
    let bot_username = bot_username(user);
    let (workspace_root, db_path) = runtime_paths(workspace_root_override, db_path_override);
    (bot, bot_username, workspace_root, db_path)
}


pub(crate) fn open_sqlite_store(db_path: &str) -> anyhow::Result<SqliteBindingStore> {
    SqliteBindingStore::open(db_path)
        .with_context(|| format!("failed to open sqlite store at {db_path}"))
}
