use vibes_codex::CodexRunRequest;

pub(crate) fn codex_request_and_cwd(
    binding: &vibes_core::SessionBinding,
    prompt: &str,
) -> (CodexRunRequest, std::path::PathBuf) {
    (
        CodexRunRequest {
            prompt: prompt.to_owned(),
            resume_target: Some(binding.session.codex_session_id.clone()),
        },
        std::path::PathBuf::from(&binding.workspace_root),
    )
}

pub(crate) fn rendered_or_default(rendered: String) -> String {
    if rendered.trim().is_empty() {
        "Codex run completed with no transcript output.".to_owned()
    } else {
        rendered
    }
}

pub(crate) fn runtime_paths(
    workspace_root: Option<String>,
    db_path: Option<String>,
) -> (String, String) {
    (
        workspace_root.unwrap_or_else(|| ".".to_owned()),
        db_path.unwrap_or_else(|| "vibes.sqlite3".to_owned()),
    )
}

pub(crate) fn bot_username(user: &teloxide::types::User) -> Option<String> {
    user.username
        .clone()
        .filter(|username| !username.is_empty())
}
