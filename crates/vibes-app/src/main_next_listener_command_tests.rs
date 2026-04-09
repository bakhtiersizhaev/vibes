use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use teloxide::{Bot, types::Update};

use crate::main_runtime::handle_next_listener_event;
use crate::main_startup::build_runtime_components;
use crate::main_test_support::NoopExecutor;

#[cfg(test)]
mod tests {
}
