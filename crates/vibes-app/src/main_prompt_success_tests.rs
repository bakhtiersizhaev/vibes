use std::sync::Mutex;

use crate::main_runtime::handle_prompt_ready;
use crate::main_test_support::{NoopExecutor, RecordingExecutor, RecordingRequester};

#[cfg(test)]
mod tests {
    use super::*;

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

    }
