use std::sync::Mutex;

use crate::main_runtime_outcome::handle_prompt_ready;
use crate::main_test_support::{NoopExecutor, RecordingRequester, SharedWriter};

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test(flavor = "current_thread")]
    async fn handle_prompt_ready_logs_completion_error() {
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
        let writer = SharedWriter::default();
        let captured = writer.0.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        handle_prompt_ready(
            &requester,
            &executor,
            target,
            binding,
            "continue parser work".to_owned(),
        )
        .await;

        let rendered = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("failed to complete codex execution outcome"));
        assert!(requester.sent.lock().unwrap().is_empty());
    }
}
