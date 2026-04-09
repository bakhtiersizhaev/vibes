#[cfg(test)]
mod tests {
    use crate::main_support::{bot_username, codex_request_and_cwd, rendered_or_default, runtime_paths};
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
    fn codex_request_and_cwd_preserves_topic_binding_session_and_workspace() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let (request, cwd) = codex_request_and_cwd(&binding, "continue topic parser work");

        assert_eq!(request.prompt, "continue topic parser work");
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_topic_binding_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let prompt = "step 1\nstep 2\nfinish topic parser";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_topic_binding_prompt_whitespace() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let prompt = "  continue topic parser work  ";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
    }

    #[test]
    fn codex_request_and_cwd_preserves_topic_binding_padded_multiline_prompt() {
        let binding = SessionBinding {
            scope: ChatScope::Topic {
                chat_id: -1001293752024,
                topic_id: 900,
            },
            workspace_root: "/tmp/vibes-topic-workspace".to_owned(),
            session: SessionHandle {
                codex_session_id: "sess-topic-123".to_owned(),
                display_name: "rust-rewrite-topic".to_owned(),
            },
        };

        let prompt = "  step 1\nstep 2\nfinish topic parser  ";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-topic-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-topic-workspace"));
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

        let prompt = "step 1\nstep 2\nfinal line";
        let (request, cwd) = codex_request_and_cwd(&binding, prompt);

        assert_eq!(request.prompt, prompt);
        assert_eq!(request.resume_target.as_deref(), Some("sess-123"));
        assert_eq!(cwd, std::path::PathBuf::from("/tmp/vibes-workspace"));
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_blank_transcript() {
        assert_eq!(
            rendered_or_default("   \n\t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_crlf_only_transcript() {
        assert_eq!(
            rendered_or_default("\r\n \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_returns_fallback_for_cr_only_transcript() {
        assert_eq!(
            rendered_or_default("\r   \t".to_owned()),
            "Codex run completed with no transcript output."
        );
    }

    #[test]
    fn rendered_or_default_keeps_non_empty_transcript() {
        assert_eq!(
            rendered_or_default("done transcript".to_owned()),
            "done transcript"
        );
    }

    #[test]
    fn rendered_or_default_preserves_multiline_transcript() {
        let rendered = "step 1\nstep 2\nfinal line".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_crlf_prefixed_non_empty_transcript() {
        let rendered = "\r\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_cr_prefixed_non_empty_transcript() {
        let rendered = "\rstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_lf_prefixed_non_empty_transcript() {
        let rendered = "\nstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_preserves_tab_prefixed_non_empty_transcript() {
        let rendered = "\tstep 1\nstep 2".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn rendered_or_default_keeps_padded_non_empty_transcript() {
        let rendered = "  done transcript  ".to_owned();
        assert_eq!(rendered_or_default(rendered.clone()), rendered);
    }

    #[test]
    fn runtime_paths_use_defaults_when_env_is_missing() {
        let (workspace_root, db_path) = runtime_paths(None, None);

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_env_overrides() {
        let (workspace_root, db_path) = runtime_paths(
            Some("/tmp/custom-workspace".to_owned()),
            Some("/tmp/custom.sqlite3".to_owned()),
        );

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_workspace_override_with_default_db() {
        let (workspace_root, db_path) =
            runtime_paths(Some("/tmp/custom-workspace".to_owned()), None);

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_db_override_with_default_workspace() {
        let (workspace_root, db_path) = runtime_paths(None, Some("/tmp/custom.sqlite3".to_owned()));

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_empty_string_overrides() {
        let (workspace_root, db_path) = runtime_paths(Some(String::new()), Some(String::new()));

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_empty_workspace_override_with_default_db() {
        let (workspace_root, db_path) = runtime_paths(Some(String::new()), None);

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "vibes.sqlite3");
    }

    #[test]
    fn runtime_paths_preserve_empty_db_override_with_default_workspace() {
        let (workspace_root, db_path) = runtime_paths(None, Some(String::new()));

        assert_eq!(workspace_root, ".");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_workspace_override_with_empty_db() {
        let (workspace_root, db_path) = runtime_paths(
            Some("/tmp/custom-workspace".to_owned()),
            Some(String::new()),
        );

        assert_eq!(workspace_root, "/tmp/custom-workspace");
        assert_eq!(db_path, "");
    }

    #[test]
    fn runtime_paths_preserve_empty_workspace_with_db_override() {
        let (workspace_root, db_path) =
            runtime_paths(Some(String::new()), Some("/tmp/custom.sqlite3".to_owned()));

        assert_eq!(workspace_root, "");
        assert_eq!(db_path, "/tmp/custom.sqlite3");
    }

    #[test]
    fn bot_username_returns_some_when_present() {
        let user: teloxide::types::User = serde_json::from_str(
            r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":"vibes_bot"}"#,
        )
        .unwrap();

        assert_eq!(bot_username(&user).as_deref(), Some("vibes_bot"));
    }

    #[test]
    fn bot_username_returns_none_when_missing() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes"}"#).unwrap();

        assert_eq!(bot_username(&user), None);
    }

    #[test]
    fn bot_username_returns_none_for_empty_string() {
        let user: teloxide::types::User =
            serde_json::from_str(r#"{"id":1,"is_bot":true,"first_name":"Vibes","username":""}"#)
                .unwrap();

        assert_eq!(bot_username(&user), None);
    }
}
