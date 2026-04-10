use anyhow::bail;
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

pub(crate) fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let Ok(content) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for raw_line in content.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim_start();
        }
        let Some((key_raw, value_raw)) = line.split_once('=') else {
            continue;
        };
        let key = key_raw.trim();
        if key.is_empty() {
            continue;
        }

        let mut value = value_raw.trim().to_owned();
        if let Some(first) = value.chars().next() {
            if first != '\'' && first != '"' {
                if let Some(idx) = value.find(" #") {
                    value.truncate(idx);
                    value = value.trim_end().to_owned();
                }
            }
        }

        if value.len() >= 2 {
            let bytes = value.as_bytes();
            let first = bytes[0] as char;
            let last = bytes[value.len() - 1] as char;
            if (first == '\'' || first == '"') && first == last {
                value = value[1..value.len() - 1].to_owned();
            }
        }

        out.insert(key.to_owned(), value);
    }
    out
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusResult {
    pub output: String,
    pub exit_code: i32,
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

pub(crate) fn status_output(root: &Path) -> String {
    render_status_snapshot(&resolve_status_snapshot(root))
}

pub(crate) fn looks_like_vibes_process(cmdline: &str, root: &Path) -> bool {
    let bot_path = root.join("vibes.py");
    if let Ok(resolved) = bot_path.canonicalize() {
        if cmdline.contains(&resolved.display().to_string()) {
            return true;
        }
    }
    if cmdline.contains("vibes.py") && cmdline.contains(&root.display().to_string()) {
        return true;
    }
    cmdline.contains(" -m vibes")
        || cmdline.ends_with(" -m vibes")
        || cmdline.ends_with(" -m vibes.py")
}

pub(crate) fn run_status_command(root: &Path) -> StatusResult {
    let snapshot = resolve_status_snapshot(root);
    let exit_code = if snapshot.state.is_some() { 0 } else { 3 };
    StatusResult {
        output: render_status_snapshot(&snapshot),
        exit_code,
    }
}

pub(crate) fn run_cli_command(command: &Command, root: &Path) -> anyhow::Result<String> {
    match command {
        Command::Status(_args) => Ok(run_status_command(root).output),
        Command::Init(_) => bail!("init is not wired to Rust daemon runtime yet"),
        Command::Start(args) => run_start_command(root, args),
        Command::Stop(_) => bail!("stop is not wired to Rust daemon runtime yet"),
        Command::Setup(_) => bail!("setup is not wired to Rust daemon runtime yet"),
        Command::Logs(_) => bail!("logs is not wired to Rust daemon runtime yet"),
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartContext {
    pub env_path: PathBuf,
    pub file_env: HashMap<String, String>,
    pub config: ResolvedStartConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartLaunchPlan {
    pub env_path: PathBuf,
    pub paths: DaemonPaths,
    pub cmd: Vec<String>,
    pub env: HashMap<String, String>,
    pub restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartState {
    pub pid: i32,
    pub started_at: String,
    pub cmd: Vec<String>,
    pub cwd: String,
    pub env_path: String,
    pub daemon_log: String,
}

pub(crate) fn build_start_state(root: &Path, plan: &StartLaunchPlan, pid: i32, started_at: String) -> StartState {
    StartState {
        pid,
        started_at,
        cmd: plan.cmd.clone(),
        cwd: root.display().to_string(),
        env_path: plan.env_path.display().to_string(),
        daemon_log: plan.paths.daemon_log_path.display().to_string(),
    }
}

pub(crate) fn resolve_start_context(root: &Path, args: &StartArgs) -> StartContext {
    let env_path = args
        .env
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_env_path(root));
    let file_env = parse_env_file(&env_path);
    let config = resolve_start_config(args, &file_env);
    StartContext {
        env_path,
        file_env,
        config,
    }
}

pub(crate) fn resolve_start_launch_plan(root: &Path, args: &StartArgs) -> anyhow::Result<StartLaunchPlan> {
    let context = resolve_start_context(root, args);
    let token = context
        .config
        .token
        .clone()
        .ok_or_else(|| anyhow::anyhow!(
            "Не найден токен. Создай {} и укажи VIBES_TOKEN=... или передай --token",
            context.env_path.display()
        ))?;
    let paths = daemon_paths(root);
    let python = context
        .config
        .python_bin
        .clone()
        .unwrap_or_else(|| {
            let local = root.join(".venv/bin/python");
            if local.exists() {
                local.display().to_string()
            } else {
                "python3".to_owned()
            }
        });
    let bot_script = root.join("vibes.py");
    if !bot_script.exists() {
        bail!("Не найден {}", bot_script.display());
    }
    let mut env = context.file_env.clone();
    env.insert("VIBES_TOKEN".to_owned(), token);
    if let Some(admin_id) = context.config.admin_id {
        env.insert("VIBES_ADMIN_ID".to_owned(), admin_id.to_string());
    }
    env.insert("PYTHONUNBUFFERED".to_owned(), "1".to_owned());
    Ok(StartLaunchPlan {
        env_path: context.env_path,
        paths,
        cmd: vec![python, bot_script.display().to_string()],
        env,
        restart: context.config.restart,
    })
}

pub(crate) fn run_start_command(root: &Path, args: &StartArgs) -> anyhow::Result<String> {
    let plan = resolve_start_launch_plan(root, args)?;
    Ok(format!(
        concat!(
            "start preflight ready
",
            "env: {}
",
            "runtime: {}
",
            "state: {}
",
            "log: {}
",
            "token: present
",
            "admin_id: {}
",
            "python: {}
",
            "restart: {}
",
            "cmd: {}
",
            "mode: Rust start wiring not finished yet"
        ),
        plan.env_path.display(),
        plan.paths.runtime_dir.display(),
        plan.paths.state_path.display(),
        plan.paths.daemon_log_path.display(),
        plan.env
            .get("VIBES_ADMIN_ID")
            .cloned()
            .unwrap_or_else(|| "none".to_owned()),
        plan.cmd
            .first()
            .cloned()
            .unwrap_or_else(|| "none".to_owned()),
        plan.restart,
        plan.cmd.join(" "),
    ))
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
    fn parse_env_file_matches_python_contract() {
        let path = std::env::temp_dir().join("vibes-daemon-env-parse.env");
        let _ = fs::remove_file(&path);
        fs::write(
            &path,
            "# comment\nexport VIBES_TOKEN=abc123\nVIBES_ADMIN_ID=42 # inline comment\nQUOTED=\"hello world\"\nSINGLE='quoted value'\nEMPTY=\nIGNORED_LINE\n",
        )
        .unwrap();
        let env = parse_env_file(&path);
        assert_eq!(env.get("VIBES_TOKEN").map(String::as_str), Some("abc123"));
        assert_eq!(env.get("VIBES_ADMIN_ID").map(String::as_str), Some("42"));
        assert_eq!(env.get("QUOTED").map(String::as_str), Some("hello world"));
        assert_eq!(env.get("SINGLE").map(String::as_str), Some("quoted value"));
        assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
        assert!(!env.contains_key("IGNORED_LINE"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parse_env_file_returns_empty_for_missing_file() {
        let path = std::env::temp_dir().join("vibes-daemon-missing.env");
        let _ = fs::remove_file(&path);
        let env = parse_env_file(&path);
        assert!(env.is_empty());
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
    fn resolve_start_context_reads_env_file_and_default_path() {
        let root = std::env::temp_dir().join("vibes-daemon-start-context-default");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let env_path = root.join(".env");
        fs::write(&env_path, "VIBES_TOKEN=env-token
VIBES_ADMIN_ID=123
VIBES_PYTHON_BIN=/usr/bin/python3
").unwrap();
        let args = StartArgs {
            token: None,
            admin: None,
            env: None,
            restart: true,
        };
        let context = resolve_start_context(&root, &args);
        assert_eq!(context.env_path, env_path);
        assert_eq!(context.file_env.get("VIBES_TOKEN").map(String::as_str), Some("env-token"));
        assert_eq!(context.config.token.as_deref(), Some("env-token"));
        assert_eq!(context.config.admin_id, Some(123));
        assert_eq!(context.config.python_bin.as_deref(), Some("/usr/bin/python3"));
        assert!(context.config.restart);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_start_context_honors_custom_env_path_and_cli_overrides() {
        let root = std::env::temp_dir().join("vibes-daemon-start-context-custom");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let env_path = root.join("custom.env");
        fs::write(&env_path, "VIBES_TOKEN=file-token
VIBES_ADMIN_ID=999
").unwrap();
        let args = StartArgs {
            token: Some("cli-token".to_owned()),
            admin: Some(123),
            env: Some(env_path.display().to_string()),
            restart: false,
        };
        let context = resolve_start_context(&root, &args);
        assert_eq!(context.env_path, env_path);
        assert_eq!(context.file_env.get("VIBES_TOKEN").map(String::as_str), Some("file-token"));
        assert_eq!(context.config.token.as_deref(), Some("cli-token"));
        assert_eq!(context.config.admin_id, Some(123));
        let _ = fs::remove_dir_all(&root);
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

    #[test]
    fn status_output_renders_running_daemon_snapshot() {
        let root = std::env::temp_dir().join("vibes-daemon-status-output-running");
        let _ = fs::remove_dir_all(&root);
        let paths = daemon_paths(&root);
        let state = DaemonState {
            pid: 321,
            started_at: "2026-04-10T04:00:00Z".to_owned(),
            cmd: vec!["vibes".to_owned(), "start".to_owned()],
            cwd: "/tmp/vibes".to_owned(),
            env_path: "/tmp/vibes/.env".to_owned(),
            daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
        };
        write_state(&paths.state_path, &state).unwrap();
        let rendered = status_output(&root);
        assert!(rendered.contains("pid: 321"));
        assert!(rendered.contains("cmd: vibes start"));
        assert!(rendered.contains("runtime: /tmp" ) == false); // sanity: use actual root-derived paths
        assert!(rendered.contains(&format!("state: {}", paths.state_path.display())));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn status_output_renders_missing_daemon_state() {
        let root = std::env::temp_dir().join("vibes-daemon-status-output-missing");
        let _ = fs::remove_dir_all(&root);
        let rendered = status_output(&root);
        assert!(rendered.contains("state: missing"));
        assert!(rendered.contains("runtime: /tmp") == false);
        assert!(rendered.contains(&format!("state: {}", root.join(".vibes/daemon.json").display())));
    }

    #[test]
    fn run_cli_command_renders_status_output() {
        let root = std::env::temp_dir().join("vibes-daemon-run-cli-status");
        let _ = fs::remove_dir_all(&root);
        let output = run_cli_command(&Command::Status(StatusArgs {}), &root).unwrap();
        assert!(output.contains("state: missing"));
        assert!(output.contains(&format!("runtime: {}", root.join(".vibes").display())));
    }

    #[test]
    fn run_cli_command_rejects_unwired_commands() {
        let root = std::env::temp_dir().join("vibes-daemon-run-cli-unwired");
        let err = run_cli_command(&Command::Stop(StopArgs {
            force: false,
            timeout: 10.0,
        }), &root).unwrap_err();
        assert!(err.to_string().contains("not wired to Rust daemon runtime yet"));
    }

    #[test]
    fn run_cli_command_renders_start_preflight() {
        let root = std::env::temp_dir().join("vibes-daemon-run-cli-start");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vibes.py"), "print(\'hi\')\n").unwrap();
        fs::write(
            root.join(".env"),
            "VIBES_TOKEN=env-token\nVIBES_ADMIN_ID=42\nVIBES_PYTHON_BIN=/usr/bin/python3\n",
        )
        .unwrap();
        let output = run_cli_command(
            &Command::Start(StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: true,
            }),
            &root,
        )
        .unwrap();
        assert!(output.contains("start preflight ready"));
        assert!(output.contains("token: present"));
        assert!(output.contains("admin_id: 42"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_start_command_returns_preflight_summary() {
        let root = std::env::temp_dir().join("vibes-daemon-start-preflight");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vibes.py"), "print(\'hi\')\n").unwrap();
        fs::write(
            root.join(".env"),
            "VIBES_TOKEN=env-token\nVIBES_ADMIN_ID=42\nVIBES_PYTHON_BIN=/usr/bin/python3\n",
        )
        .unwrap();
        let output = run_start_command(
            &root,
            &StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: true,
            },
        )
        .unwrap();
        assert!(output.contains("start preflight ready"));
        assert!(output.contains("token: present"));
        assert!(output.contains("admin_id: 42"));
        assert!(output.contains("python: /usr/bin/python3"));
        assert!(output.contains("restart: true"));
        assert!(output.contains("mode: Rust start wiring not finished yet"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_start_launch_plan_builds_python_command_and_env() {
        let root = std::env::temp_dir().join("vibes-daemon-launch-plan");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vibes.py"), "print('hi')\n").unwrap();
        fs::write(
            root.join(".env"),
            "VIBES_TOKEN=env-token\nVIBES_ADMIN_ID=42\nVIBES_PYTHON_BIN=/usr/bin/python3\nEXTRA=1\n",
        )
        .unwrap();
        let plan = resolve_start_launch_plan(
            &root,
            &StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: true,
            },
        )
        .unwrap();
        assert_eq!(plan.env_path, root.join(".env"));
        assert_eq!(plan.cmd, vec!["/usr/bin/python3".to_owned(), root.join("vibes.py").display().to_string()]);
        assert_eq!(plan.env.get("VIBES_TOKEN").map(String::as_str), Some("env-token"));
        assert_eq!(plan.env.get("VIBES_ADMIN_ID").map(String::as_str), Some("42"));
        assert_eq!(plan.env.get("PYTHONUNBUFFERED").map(String::as_str), Some("1"));
        assert_eq!(plan.env.get("EXTRA").map(String::as_str), Some("1"));
        assert!(plan.restart);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_start_state_uses_plan_paths_and_cmd() {
        let root = std::env::temp_dir().join("vibes-daemon-start-state");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vibes.py"), "print(\'hi\')\n").unwrap();
        fs::write(root.join(".env"), "VIBES_TOKEN=env-token\n").unwrap();
        let plan = resolve_start_launch_plan(
            &root,
            &StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: false,
            },
        )
        .unwrap();
        let state = build_start_state(&root, &plan, 321, "2026-04-10T01:00:00Z".to_owned());
        assert_eq!(state.pid, 321);
        assert_eq!(state.started_at, "2026-04-10T01:00:00Z");
        assert_eq!(state.cmd, plan.cmd);
        assert_eq!(state.cwd, root.display().to_string());
        assert_eq!(state.env_path, root.join(".env").display().to_string());
        assert_eq!(state.daemon_log, root.join(".vibes/daemon.log").display().to_string());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_start_launch_plan_requires_vibes_py() {
        let root = std::env::temp_dir().join("vibes-daemon-launch-plan-missing-script");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".env"), "VIBES_TOKEN=env-token\n").unwrap();
        let err = resolve_start_launch_plan(
            &root,
            &StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Не найден"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_start_command_errors_when_token_missing() {
        let root = std::env::temp_dir().join("vibes-daemon-start-missing-token");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let err = run_start_command(
            &root,
            &StartArgs {
                token: None,
                admin: None,
                env: None,
                restart: false,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Не найден токен"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn looks_like_vibes_process_matches_python_contract() {
        let root = std::env::temp_dir().join("vibes-daemon-looks-like");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("vibes.py"), "print(\'hi\')\n").unwrap();
        let bot_path = root.join("vibes.py").canonicalize().unwrap();
        assert!(looks_like_vibes_process(&format!("/usr/bin/python3 {}", bot_path.display()), &root));
        assert!(looks_like_vibes_process(&format!("python3 {}/vibes.py", root.display()), &root));
        assert!(looks_like_vibes_process("python3 -m vibes", &root));
        assert!(!looks_like_vibes_process("python3 other.py", &root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn looks_like_vibes_process_handles_missing_script() {
        let root = std::env::temp_dir().join("vibes-daemon-looks-like-missing");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(looks_like_vibes_process(&format!("python3 {}/vibes.py", root.display()), &root));
        assert!(!looks_like_vibes_process("python3 other.py", &root));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_status_command_returns_running_exit_code() {
        let root = std::env::temp_dir().join("vibes-daemon-run-status-running");
        let _ = fs::remove_dir_all(&root);
        let paths = daemon_paths(&root);
        let state = DaemonState {
            pid: 321,
            started_at: "2026-04-10T04:00:00Z".to_owned(),
            cmd: vec!["vibes".to_owned(), "start".to_owned()],
            cwd: "/tmp/vibes".to_owned(),
            env_path: "/tmp/vibes/.env".to_owned(),
            daemon_log: "/tmp/vibes/.vibes/daemon.log".to_owned(),
        };
        write_state(&paths.state_path, &state).unwrap();
        let result = run_status_command(&root);
        assert_eq!(result.exit_code, 0);
        assert!(result.output.contains("pid: 321"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_status_command_returns_stopped_exit_code() {
        let root = std::env::temp_dir().join("vibes-daemon-run-status-missing");
        let _ = fs::remove_dir_all(&root);
        let result = run_status_command(&root);
        assert_eq!(result.exit_code, 3);
        assert!(result.output.contains("state: missing"));
    }
}
