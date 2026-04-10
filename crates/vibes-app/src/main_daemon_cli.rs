use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const ENV_TOKEN_KEYS: &[&str] = &[
    "VIBES_TOKEN",
    "VIBES_TELEGRAM_TOKEN",
    "TELEGRAM_BOT_TOKEN",
    "BOT_TOKEN",
    "TALKING_TOKEN",
    "TALKING",
    "Talking",
];

pub(crate) const ENV_ADMIN_KEYS: &[&str] = &[
    "VIBES_ADMIN_ID",
    "VIBES_TELEGRAM_ADMIN_ID",
    "TELEGRAM_ADMIN_ID",
    "ADMIN_ID",
];

pub(crate) const ENV_PYTHON_KEYS: &[&str] = &[
    "VIBES_PYTHON",
    "VIBES_PYTHON_BIN",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DaemonPaths {
    pub env_path: PathBuf,
    pub runtime_dir: PathBuf,
    pub state_path: PathBuf,
    pub daemon_log_path: PathBuf,
}

pub(crate) fn default_env_path(root: &Path) -> PathBuf {
    root.join(".env")
}

pub(crate) fn daemon_paths(root: &Path) -> DaemonPaths {
    let runtime_dir = root.join(".vibes");
    DaemonPaths {
        env_path: default_env_path(root),
        state_path: runtime_dir.join("daemon.json"),
        daemon_log_path: runtime_dir.join("daemon.log"),
        runtime_dir,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DaemonState {
    pub pid: i32,
    pub started_at: String,
    pub cmd: Vec<String>,
    pub cwd: String,
    pub env_path: String,
    pub daemon_log: String,
}

pub(crate) fn load_state(path: &Path) -> Option<DaemonState> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<DaemonState>(&text).ok()
}

pub(crate) fn write_state(path: &Path, state: &DaemonState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{ext}.tmp"),
        _ => "tmp".to_owned(),
    });
    let payload = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusSnapshot {
    pub paths: DaemonPaths,
    pub state: Option<DaemonState>,
}

pub(crate) fn resolve_status_snapshot(root: &Path) -> StatusSnapshot {
    let paths = daemon_paths(root);
    let state = load_state(&paths.state_path);
    StatusSnapshot { paths, state }
}

pub(crate) fn render_status_snapshot(snapshot: &StatusSnapshot) -> String {
    let mut lines = vec![
        format!("env: {}", snapshot.paths.env_path.display()),
        format!("runtime: {}", snapshot.paths.runtime_dir.display()),
        format!("state: {}", snapshot.paths.state_path.display()),
        format!("log: {}", snapshot.paths.daemon_log_path.display()),
    ];
    match &snapshot.state {
        Some(state) => {
            lines.push(format!("pid: {}", state.pid));
            lines.push(format!("started_at: {}", state.started_at));
            lines.push(format!("cwd: {}", state.cwd));
            lines.push(format!("env_path: {}", state.env_path));
            lines.push(format!("daemon_log: {}", state.daemon_log));
            lines.push(format!("cmd: {}", state.cmd.join(" ")));
        }
        None => lines.push("state: missing".to_owned()),
    }
    lines.join("
")
}

fn first_non_empty<'a>(override_value: Option<&'a str>, env: &'a HashMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    override_value.filter(|value| !value.trim().is_empty()).or_else(|| {
        keys.iter().find_map(|key| {
            env.get(*key)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
        })
    })
}

pub(crate) fn resolve_token<'a>(override_value: Option<&'a str>, env: &'a HashMap<String, String>) -> Option<&'a str> {
    first_non_empty(override_value, env, ENV_TOKEN_KEYS)
}

pub(crate) fn resolve_python<'a>(override_value: Option<&'a str>, env: &'a HashMap<String, String>) -> Option<&'a str> {
    first_non_empty(override_value, env, ENV_PYTHON_KEYS)
}

pub(crate) fn resolve_admin_id(override_value: Option<i64>, env: &HashMap<String, String>) -> Option<i64> {
    override_value.or_else(|| {
        ENV_ADMIN_KEYS.iter().find_map(|key| {
            env.get(*key)
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .and_then(|value| value.parse::<i64>().ok())
        })
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedStartConfig {
    pub token: Option<String>,
    pub admin_id: Option<i64>,
    pub python_bin: Option<String>,
    pub env_path: Option<String>,
    pub restart: bool,
}

pub(crate) fn resolve_start_config(args: &StartArgs, env: &HashMap<String, String>) -> ResolvedStartConfig {
    ResolvedStartConfig {
        token: resolve_token(args.token.as_deref(), env).map(str::to_owned),
        admin_id: resolve_admin_id(args.admin, env),
        python_bin: resolve_python(None, env).map(str::to_owned),
        env_path: args.env.clone(),
        restart: args.restart,
    }
}

#[derive(Debug, Parser, PartialEq)]
#[command(name = "vibes", about = "Rust daemon/runtime entrypoint for vibes")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq)]
pub(crate) enum Command {
    Init(InitArgs),
    Start(StartArgs),
    Status(StatusArgs),
    Stop(StopArgs),
    Setup(SetupArgs),
    Logs(LogsArgs),
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct InitArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StartArgs {
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long)]
    pub admin: Option<i64>,
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long)]
    pub restart: bool,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StatusArgs {}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct StopArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long, default_value_t = 10.0)]
    pub timeout: f64,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct SetupArgs {
    #[arg(long)]
    pub start: bool,
    #[arg(long)]
    pub restart: bool,
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Debug, Args, PartialEq)]
pub(crate) struct LogsArgs {
    #[arg(long)]
    pub follow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_start_flags() {
        let cli = Cli::parse_from([
            "vibes",
            "start",
            "--token",
            "abc",
            "--admin",
            "123",
            "--restart",
            "--env",
            ".env.custom",
        ]);
        assert_eq!(
            cli.command,
            Some(Command::Start(StartArgs {
                token: Some("abc".to_owned()),
                admin: Some(123),
                env: Some(".env.custom".to_owned()),
                restart: true,
            }))
        );
    }

    #[test]
    fn parses_stop_defaults() {
        let cli = Cli::parse_from(["vibes", "stop"]);
        assert_eq!(
            cli.command,
            Some(Command::Stop(StopArgs {
                force: false,
                timeout: 10.0,
            }))
        );
    }

    #[test]
    fn parses_logs_follow() {
        let cli = Cli::parse_from(["vibes", "logs", "--follow"]);
        assert_eq!(cli.command, Some(Command::Logs(LogsArgs { follow: true })));
    }

    #[test]
    fn resolve_token_prefers_override_then_env_priority() {
        let mut env = HashMap::new();
        env.insert("BOT_TOKEN".to_owned(), "bot-token".to_owned());
        env.insert("VIBES_TOKEN".to_owned(), "primary-token".to_owned());
        assert_eq!(resolve_token(None, &env), Some("primary-token"));
        assert_eq!(resolve_token(Some("override-token"), &env), Some("override-token"));
    }

    #[test]
    fn resolve_admin_id_prefers_override_and_skips_invalid_values() {
        let mut env = HashMap::new();
        env.insert("ADMIN_ID".to_owned(), "oops".to_owned());
        env.insert("TELEGRAM_ADMIN_ID".to_owned(), "456".to_owned());
        assert_eq!(resolve_admin_id(None, &env), Some(456));
        assert_eq!(resolve_admin_id(Some(123), &env), Some(123));
    }

    #[test]
    fn resolve_python_uses_first_non_empty_env_value() {
        let mut env = HashMap::new();
        env.insert("VIBES_PYTHON".to_owned(), "".to_owned());
        env.insert("VIBES_PYTHON_BIN".to_owned(), "/usr/bin/python3".to_owned());
        assert_eq!(resolve_python(None, &env), Some("/usr/bin/python3"));
    }

    #[test]
    fn daemon_paths_follow_python_daemon_layout() {
        let root = Path::new("/tmp/vibes-root");
        let paths = daemon_paths(root);
        assert_eq!(paths.env_path, PathBuf::from("/tmp/vibes-root/.env"));
        assert_eq!(paths.runtime_dir, PathBuf::from("/tmp/vibes-root/.vibes"));
        assert_eq!(paths.state_path, PathBuf::from("/tmp/vibes-root/.vibes/daemon.json"));
        assert_eq!(paths.daemon_log_path, PathBuf::from("/tmp/vibes-root/.vibes/daemon.log"));
    }

    #[test]
    fn load_state_returns_none_for_missing_or_invalid_json() {
        let root = std::env::temp_dir().join("vibes-daemon-load-state-invalid");
        let _ = fs::remove_dir_all(&root);
        let missing = root.join("daemon.json");
        assert_eq!(load_state(&missing), None);
        fs::create_dir_all(&root).unwrap();
        let invalid = root.join("bad.json");
        fs::write(&invalid, "not json").unwrap();
        assert_eq!(load_state(&invalid), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_state_roundtrips_and_creates_parent_dir() {
        let root = std::env::temp_dir().join("vibes-daemon-write-state-roundtrip");
        let _ = fs::remove_dir_all(&root);
        let path = root.join("nested/daemon.json");
        let state = DaemonState {
            pid: 123,
            started_at: "2026-04-10T00:00:00Z".to_owned(),
            cmd: vec!["vibes".to_owned(), "start".to_owned()],
            cwd: "/tmp/vibes".to_owned(),
            env_path: "/tmp/vibes/.env".to_owned(),
            daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
        };
        write_state(&path, &state).unwrap();
        assert_eq!(load_state(&path), Some(state));
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_start_config_combines_cli_and_env() {
        let args = StartArgs {
            token: Some("cli-token".to_owned()),
            admin: None,
            env: Some(".env.custom".to_owned()),
            restart: true,
        };
        let mut env = HashMap::new();
        env.insert("TELEGRAM_ADMIN_ID".to_owned(), "456".to_owned());
        env.insert("VIBES_PYTHON_BIN".to_owned(), "/usr/bin/python3".to_owned());
        assert_eq!(
            resolve_start_config(&args, &env),
            ResolvedStartConfig {
                token: Some("cli-token".to_owned()),
                admin_id: Some(456),
                python_bin: Some("/usr/bin/python3".to_owned()),
                env_path: Some(".env.custom".to_owned()),
                restart: true,
            }
        );
    }

    #[test]
    fn resolve_start_config_uses_env_for_missing_values() {
        let args = StartArgs {
            token: None,
            admin: Some(123),
            env: None,
            restart: false,
        };
        let mut env = HashMap::new();
        env.insert("VIBES_TOKEN".to_owned(), "env-token".to_owned());
        env.insert("VIBES_PYTHON".to_owned(), "/opt/python".to_owned());
        assert_eq!(
            resolve_start_config(&args, &env),
            ResolvedStartConfig {
                token: Some("env-token".to_owned()),
                admin_id: Some(123),
                python_bin: Some("/opt/python".to_owned()),
                env_path: None,
                restart: false,
            }
        );
    }

    #[test]
    fn daemon_state_roundtrips_to_python_compatible_json() {
        let state = DaemonState {
            pid: 12345,
            started_at: "2026-04-10T00:00:00Z".to_owned(),
            cmd: vec!["python3".to_owned(), "vibes.py".to_owned()],
            cwd: "/tmp/vibes".to_owned(),
            env_path: "/tmp/vibes/.env".to_owned(),
            daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
        };
        let value = serde_json::to_value(&state).unwrap();
        assert_eq!(value["pid"], 12345);
        assert_eq!(value["started_at"], "2026-04-10T00:00:00Z");
        assert_eq!(value["cmd"][0], "python3");
        assert_eq!(value["cwd"], "/tmp/vibes");
        assert_eq!(value["env_path"], "/tmp/vibes/.env");
        assert_eq!(value["daemon_log"], "/tmp/vibes/.vibes/daemon.log");
        let restored: DaemonState = serde_json::from_value(value).unwrap();
        assert_eq!(restored, state);
    }

    #[test]
    fn resolve_status_snapshot_returns_paths_and_state() {
        let root = std::env::temp_dir().join("vibes-daemon-status-snapshot");
        let _ = fs::remove_dir_all(&root);
        let paths = daemon_paths(&root);
        let state = DaemonState {
            pid: 99,
            started_at: "2026-04-10T02:00:00Z".to_owned(),
            cmd: vec!["vibes".to_owned(), "start".to_owned()],
            cwd: "/tmp/vibes".to_owned(),
            env_path: "/tmp/vibes/.env".to_owned(),
            daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
        };
        write_state(&paths.state_path, &state).unwrap();
        let snapshot = resolve_status_snapshot(&root);
        assert_eq!(snapshot.paths, paths);
        assert_eq!(snapshot.state, Some(state));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_status_snapshot_handles_missing_state() {
        let root = std::env::temp_dir().join("vibes-daemon-status-snapshot-missing");
        let _ = fs::remove_dir_all(&root);
        let snapshot = resolve_status_snapshot(&root);
        assert_eq!(snapshot.paths.state_path, root.join(".vibes/daemon.json"));
        assert_eq!(snapshot.state, None);
    }

    #[test]
    fn render_status_snapshot_includes_running_state_fields() {
        let snapshot = StatusSnapshot {
            paths: DaemonPaths {
                env_path: PathBuf::from("/tmp/vibes/.env"),
                runtime_dir: PathBuf::from("/tmp/vibes/.vibes"),
                state_path: PathBuf::from("/tmp/vibes/.vibes/daemon.json"),
                daemon_log_path: PathBuf::from("/tmp/vibes/.vibes/daemon.log"),
            },
            state: Some(DaemonState {
                pid: 123,
                started_at: "2026-04-10T03:00:00Z".to_owned(),
                cmd: vec!["vibes".to_owned(), "start".to_owned()],
                cwd: "/tmp/vibes".to_owned(),
                env_path: "/tmp/vibes/.env".to_owned(),
                daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
            }),
        };
        let rendered = render_status_snapshot(&snapshot);
        assert!(rendered.contains("pid: 123"));
        assert!(rendered.contains("started_at: 2026-04-10T03:00:00Z"));
        assert!(rendered.contains("cmd: vibes start"));
        assert!(rendered.contains("log: /tmp/vibes/.vibes/daemon.log"));
    }

    #[test]
    fn render_status_snapshot_marks_missing_state() {
        let snapshot = StatusSnapshot {
            paths: DaemonPaths {
                env_path: PathBuf::from("/tmp/vibes/.env"),
                runtime_dir: PathBuf::from("/tmp/vibes/.vibes"),
                state_path: PathBuf::from("/tmp/vibes/.vibes/daemon.json"),
                daemon_log_path: PathBuf::from("/tmp/vibes/.vibes/daemon.log"),
            },
            state: None,
        };
        let rendered = render_status_snapshot(&snapshot);
        assert!(rendered.contains("state: missing"));
        assert!(rendered.contains("env: /tmp/vibes/.env"));
        assert!(rendered.contains("log: /tmp/vibes/.vibes/daemon.log"));
    }
}
