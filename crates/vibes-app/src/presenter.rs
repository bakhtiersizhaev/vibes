use vibes_core::{ChatScope, SessionBinding, short_session_id};

use crate::service::BindOutcome;

pub fn render_start_success(outcome: &BindOutcome) -> String {
    let short_id = short_session_id(&outcome.binding.session.codex_session_id);
    if let Some(topic_id) = outcome.created_topic_id {
        let title = outcome
            .created_topic_title
            .as_deref()
            .unwrap_or("codex-session");
        return format!(
            "Created topic `{title}` (topic #{topic_id}) and bound Codex session `{short_id}`."
        );
    }

    format!(
        "Started Codex session `{short_id}` for `{}`.",
        outcome.binding.session.display_name
    )
}

pub fn render_resume_success(outcome: &BindOutcome) -> String {
    let SessionBinding { session, scope, .. } = &outcome.binding;
    let short_id = short_session_id(&session.codex_session_id);
    format!(
        "Bound `{}` to {} using Codex session `{short_id}`.",
        session.display_name,
        scope_label(scope)
    )
}

pub fn render_missing_binding_error(scope: &ChatScope) -> String {
    format!(
        "No Codex session is bound to {} yet. Use `/new` or `/resume <session-id-or-name>` first.",
        scope_label(scope)
    )
}

pub fn render_missing_binding_hint(scope_key: &str) -> String {
    format!(
        "No Codex session is bound to `{scope_key}` yet. Use `/new` or `/resume <session-id-or-name>` first."
    )
}

fn scope_label(scope: &ChatScope) -> String {
    match scope {
        ChatScope::Direct(_) => "this DM".to_owned(),
        ChatScope::Group(chat_id) => format!("group {chat_id}"),
        ChatScope::Topic { chat_id, topic_id } => format!("topic {topic_id} in group {chat_id}"),
    }
}

#[cfg(test)]
mod tests {
    use vibes_core::{ChatScope, SessionBinding, SessionHandle};

    use crate::presenter::{
        render_missing_binding_error, render_missing_binding_hint, render_resume_success,
        render_start_success,
    };
    use crate::service::BindOutcome;

    fn binding(scope: ChatScope) -> SessionBinding {
        SessionBinding {
            scope,
            session: SessionHandle {
                codex_session_id: "019d6361-f755-7992-b08a".to_owned(),
                display_name: "rust-rewrite".to_owned(),
            },
            workspace_root: "/workspace".to_owned(),
        }
    }

    #[test]
    fn renders_new_session_with_topic_metadata() {
        let text = render_start_success(&BindOutcome {
            binding: binding(ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            }),
            created_topic_id: Some(114),
            created_topic_title: Some("codex-rust-rewrite".to_owned()),
        });

        assert!(text.contains("codex-rust-rewrite"));
        assert!(text.contains("019d6361"));
        assert!(text.contains("topic #114"));
    }

    #[test]
    fn renders_resume_success_for_topic_scope() {
        let text = render_resume_success(&BindOutcome {
            binding: binding(ChatScope::Topic {
                chat_id: -1003562096175,
                topic_id: 114,
            }),
            created_topic_id: None,
            created_topic_title: None,
        });

        assert!(text.contains("rust-rewrite"));
        assert!(text.contains("topic 114"));
        assert!(text.contains("019d6361"));
    }

    #[test]
    fn renders_missing_binding_help() {
        let text = render_missing_binding_error(&ChatScope::Direct(1668851955));
        assert!(text.contains("this DM"));
        assert!(text.contains("/new"));
        assert!(text.contains("/resume"));
    }

    #[test]
    fn renders_missing_binding_help_from_scope_key() {
        let text = render_missing_binding_hint("topic:-100:114");
        assert!(text.contains("topic:-100:114"));
        assert!(text.contains("/new"));
        assert!(text.contains("/resume"));
    }
}
