use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::process::Command as StdCommand;
use std::thread;
use std::time::Duration;

use anyhow::{Context, bail};

use crate::main_daemon_cli::{
    DaemonState, InitArgs, LogsArgs, SetupArgs, StartArgs, StopArgs, daemon_paths,
    default_env_path, load_state, parse_env_file, resolve_start_context, write_state,
};

const ENV_TEMPLATE: &str = r#"# vibes configuration
# Required: Telegram Bot API token (from @BotFather)
VIBES_TOKEN=

# Optional: Telegram user ID for admin-only commands
# VIBES_ADMIN_ID=

# Optional: Codex sandbox mode (e.g. "networking")
# VIBES_CODEX_SANDBOX=

# Optional: Codex approval policy
# VIBES_CODEX_APPROVAL_POLICY=

# Optional: Claude model override
# VIBES_CLAUDE_MODEL=

# Optional: Claude permission mode
# VIBES_CLAUDE_PERMISSION_MODE=

# Optional: default project directory for new sessions
# VIBES_DEFAULT_PROJECTS_DIR=
"#;

pub(crate) fn run_init(args: &InitArgs, root: &Path) -> anyhow::Result<String> {
    let env_path = args
        .env
        .as_deref()
        .map(|p| root.join(p))
        .unwrap_or_else(|| default_env_path(root));

    if env_path.exists() && !args.force {
        bail!(
            "{} already exists (use --force to overwrite)",
            env_path.display()
        );
    }

    fs::write(&env_path, ENV_TEMPLATE)
        .with_context(|| format!("failed to write {}", env_path.display()))?;

    Ok(format!("created {}", env_path.display()))
}

fn is_vibes_process(pid: i32) -> bool {
    let cmdline_path = format!("/proc/{pid}/cmdline");
    if let Ok(data) = fs::read(&cmdline_path) {
        let text = String::from_utf8_lossy(&data).to_lowercase();
        text.contains("vibes")
    } else {
        false
    }
}

fn process_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

pub(crate) fn run_start(args: &StartArgs, root: &Path) -> anyhow::Result<String> {
    let ctx = resolve_start_context(root, args);
    let token = ctx
        .config
        .token
        .as_deref()
        .context("no token found (set VIBES_TOKEN in .env or pass --token)")?;

    let paths = daemon_paths(root);
    fs::create_dir_all(&paths.runtime_dir)?;

    // Check existing daemon
    if let Some(existing) = load_state(&paths.state_path) {
        if process_alive(existing.pid) {
            if !args.restart {
                bail!(
                    "daemon already running (pid {}). Use --restart to replace it.",
                    existing.pid
                );
            }
            // Stop existing before restarting
            stop_pid(existing.pid, false, 5.0)?;
        }
        let _ = fs::remove_file(&paths.state_path);
    }

    let exe = std::env::current_exe().context("cannot determine own executable path")?;
    let log_file = fs::File::create(&paths.daemon_log_path)
        .with_context(|| format!("cannot create log {}", paths.daemon_log_path.display()))?;
    let log_err = log_file
        .try_clone()
        .context("cannot clone log file handle")?;

    let mut cmd = StdCommand::new(&exe);
    cmd.stdout(log_file).stderr(log_err);
    cmd.env("VIBES_TOKEN", token);
    cmd.env("TELOXIDE_TOKEN", token);
    cmd.env("VIBES_DB_PATH", paths.runtime_dir.join("db.sqlite"));
    if let Some(admin) = ctx.config.admin_id {
        cmd.env("VIBES_ADMIN_ID", admin.to_string());
    }
    // Pass through remaining env from file
    for (k, v) in &ctx.file_env {
        if k != "VIBES_TOKEN" && k != "VIBES_ADMIN_ID" {
            cmd.env(k, v);
        }
    }

    let child = cmd.spawn().context("failed to spawn daemon process")?;
    let pid = child.id() as i32;

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let state = DaemonState {
        pid,
        started_at: now_utc_iso8601(),
        cmd: vec![exe.display().to_string()],
        cwd,
        env_path: ctx.env_path.display().to_string(),
        daemon_log: paths.daemon_log_path.display().to_string(),
    };
    write_state(&paths.state_path, &state)?;

    // Wait briefly and check if process died immediately
    thread::sleep(Duration::from_millis(500));
    if !process_alive(pid) {
        let _ = fs::remove_file(&paths.state_path);
        bail!(
            "daemon exited immediately. Check log: {}",
            paths.daemon_log_path.display()
        );
    }

    Ok(format!(
        "started (pid {}) — log: {}",
        pid,
        paths.daemon_log_path.display()
    ))
}

fn stop_pid(pid: i32, force: bool, timeout_secs: f64) -> anyhow::Result<()> {
    if !process_alive(pid) {
        return Ok(());
    }

    if !force && !is_vibes_process(pid) {
        bail!("pid {pid} does not look like a vibes process (use --force to override)");
    }

    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    let deadline = std::time::Instant::now() + Duration::from_secs_f64(timeout_secs);
    while std::time::Instant::now() < deadline {
        if !process_alive(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    // SIGKILL if still alive
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    thread::sleep(Duration::from_millis(200));
    Ok(())
}

pub(crate) fn run_stop(args: &StopArgs, root: &Path) -> anyhow::Result<String> {
    let paths = daemon_paths(root);

    let Some(state) = load_state(&paths.state_path) else {
        return Ok("not running (no state file)".to_owned());
    };

    if !process_alive(state.pid) {
        let _ = fs::remove_file(&paths.state_path);
        return Ok(format!("not running (stale pid {})", state.pid));
    }

    stop_pid(state.pid, args.force, args.timeout)?;
    let _ = fs::remove_file(&paths.state_path);
    Ok(format!("stopped (pid {})", state.pid))
}

pub(crate) fn run_logs(args: &LogsArgs, root: &Path) -> anyhow::Result<String> {
    let paths = daemon_paths(root);

    if !paths.daemon_log_path.exists() {
        bail!("log file not found: {}", paths.daemon_log_path.display());
    }

    if args.follow {
        let file = fs::File::open(&paths.daemon_log_path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::End(0))?;

        println!("--- following {} ---", paths.daemon_log_path.display());
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => thread::sleep(Duration::from_millis(200)),
                Ok(_) => print!("{line}"),
                Err(e) => bail!("read error: {e}"),
            }
        }
    } else {
        let content = fs::read_to_string(&paths.daemon_log_path)?;
        let lines: Vec<&str> = content.lines().collect();
        let tail = if lines.len() > 50 {
            &lines[lines.len() - 50..]
        } else {
            &lines
        };
        Ok(format!(
            "log: {}\n{}",
            paths.daemon_log_path.display(),
            tail.join("\n")
        ))
    }
}

pub(crate) fn run_setup(args: &SetupArgs, root: &Path) -> anyhow::Result<String> {
    let env_path = args
        .env
        .as_deref()
        .map(|p| root.join(p))
        .unwrap_or_else(|| default_env_path(root));

    if !env_path.exists() {
        fs::write(&env_path, ENV_TEMPLATE)?;
    }

    let env = parse_env_file(&env_path);
    let mut lines = Vec::new();

    if env.get("VIBES_TOKEN").is_none_or(|v| v.is_empty()) {
        lines.push("VIBES_TOKEN is not set in .env — set it before starting the bot.");
    }

    if args.start || args.restart {
        let start_args = StartArgs {
            token: None,
            admin: None,
            env: args.env.clone(),
            restart: args.restart,
        };
        let result = run_start(&start_args, root)?;
        lines.push(&result);
        return Ok(lines.join("\n"));
    }

    if lines.is_empty() {
        Ok(format!("env ready: {}", env_path.display()))
    } else {
        Ok(lines.join("\n"))
    }
}

fn now_utc_iso8601() -> String {
    let now = time::OffsetDateTime::now_utc();
    let fmt = time::format_description::well_known::Rfc3339;
    now.format(&fmt).unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_env_template() {
        let root = std::env::temp_dir().join("vibes-cmd-init");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let result = run_init(
            &InitArgs {
                force: false,
                env: None,
            },
            &root,
        )
        .unwrap();
        assert!(result.contains("created"));
        assert!(root.join(".env").exists());
        let content = fs::read_to_string(root.join(".env")).unwrap();
        assert!(content.contains("VIBES_TOKEN"));

        // Second call fails without force
        let err = run_init(
            &InitArgs {
                force: false,
                env: None,
            },
            &root,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already exists"));

        // Force overwrites
        let result = run_init(
            &InitArgs {
                force: true,
                env: None,
            },
            &root,
        )
        .unwrap();
        assert!(result.contains("created"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stop_returns_not_running_when_no_state() {
        let root = std::env::temp_dir().join("vibes-cmd-stop-empty");
        let _ = fs::remove_dir_all(&root);
        let result = run_stop(
            &StopArgs {
                force: false,
                timeout: 5.0,
            },
            &root,
        )
        .unwrap();
        assert!(result.contains("not running"));
    }

    #[test]
    fn logs_fails_when_no_log_file() {
        let root = std::env::temp_dir().join("vibes-cmd-logs-missing");
        let _ = fs::remove_dir_all(&root);
        let err = run_logs(&LogsArgs { follow: false }, &root).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn logs_reads_tail_of_existing_log() {
        let root = std::env::temp_dir().join("vibes-cmd-logs-read");
        let _ = fs::remove_dir_all(&root);
        let paths = daemon_paths(&root);
        fs::create_dir_all(&paths.runtime_dir).unwrap();
        fs::write(&paths.daemon_log_path, "line1\nline2\nline3\n").unwrap();
        let result = run_logs(&LogsArgs { follow: false }, &root).unwrap();
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn now_utc_iso8601_produces_valid_timestamp() {
        let ts = now_utc_iso8601();
        assert!(ts.contains('T'));
        assert!(ts.len() >= 20);
    }
}
