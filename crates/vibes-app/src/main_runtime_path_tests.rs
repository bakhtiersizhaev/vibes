#[cfg(test)]
mod tests {
    use crate::main_support::{bot_username, runtime_paths};

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

}
