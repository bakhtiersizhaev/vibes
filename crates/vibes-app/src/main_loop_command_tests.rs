use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, types::Update};
use tokio_stream::iter;

use crate::main_runtime::run_polling_loop;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::NoopExecutor;

#[cfg(test)]
mod tests {
}
