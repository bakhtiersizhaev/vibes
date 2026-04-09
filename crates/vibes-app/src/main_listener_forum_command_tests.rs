use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, types::Update};

use crate::main_runtime::handle_listener_item;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::PanicExecutor;

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let pid = std::process::id();
        let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("vibes-listener-forum-command-tests-{pid}-{n}.sqlite3"))
    }
}
