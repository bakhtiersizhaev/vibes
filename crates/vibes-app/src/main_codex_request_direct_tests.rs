#[cfg(test)]
mod tests {
    use crate::main_support::codex_request_and_cwd;
    use vibes_core::{ChatScope, SessionBinding, SessionHandle};

    #[test]
    fn codex_request_and_cwd_preserves_binding_session_and_workspace() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let (request, cwd) = codex_request_and_cwd(&binding, "continue parser work");

        assert_eq!(request.prompt, "continue parser work");
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_prompt_whitespace() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let prompt = "  continue parser work  ";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_padded_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let prompt = "  step 1\nstep 2\nfinal line  ";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Direct(408258968),
            workspace_root: "/tmp/vibes-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-123".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
        };

        let prompt = "step 1
step 2
final line";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }
}
