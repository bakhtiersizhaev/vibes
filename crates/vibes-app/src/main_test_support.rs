use std::{
    io,
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tracing_subscriber::fmt::MakeWriter;
use vibes_app::{
    TelegramExecutionError, TelegramPromptExecutor, TelegramRequestError, TelegramRequester,
};

static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn unique_db_path(prefix: &str) -> PathBuf {
    let pid = std::process::id();
    let n = TEST_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{pid}-{n}.sqlite3"))
}

#[derive(Clone, Default)]
pub(crate) struct SharedWriter(pub(crate) Arc<Mutex<Vec<u8>>>);

pub(crate) struct SharedWriterGuard(pub(crate) Arc<Mutex<Vec<u8>>>);

impl<'a> MakeWriter<'a> for SharedWriter {
    type Writer = SharedWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SharedWriterGuard(self.0.clone())
    }
}

impl Write for SharedWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) struct NoopExecutor;

impl TelegramPromptExecutor for NoopExecutor {
    fn execute_prompt(
        &self,
        _binding: &vibes_core::SessionBinding,
        _prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        Ok("noop".to_owned())
    }
}

pub(crate) struct PanicExecutor;

impl TelegramPromptExecutor for PanicExecutor {
    fn execute_prompt(
        &self,
        _binding: &vibes_core::SessionBinding,
        _prompt: &str,
    ) -> Result<String, TelegramExecutionError> {
        panic!("executor should not run");
    }
}

pub(crate) struct RecordingRequester {
    pub(crate) sent: Mutex<Vec<(vibes_telegram::ReplyTarget, String)>>,
    pub(crate) fail: Mutex<Option<String>>,
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

pub(crate) struct RecordingExecutor {
    pub(crate) seen: Mutex<Vec<(String, String, String)>>,
    pub(crate) response: String,
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
