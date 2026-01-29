from __future__ import annotations

import datetime as dt
import getpass
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, Optional

from vibes_app.daemon.envfile import parse_env_file, pick_int, pick_str, update_env_file
from vibes_app.daemon.process import (
    detect_local_venv_python,
    format_timedelta,
    looks_like_vibes_process,
    parse_ps_etime,
    pid_is_running,
    try_get_cmdline,
)
from vibes_app.daemon.state import daemon_log_path, load_state, runtime_dir, state_path, write_state


ENV_TOKEN_KEYS: tuple[str, ...] = (
    "VIBES_TOKEN",
    "VIBES_TELEGRAM_TOKEN",
    "TELEGRAM_BOT_TOKEN",
    "BOT_TOKEN",
    "TALKING_TOKEN",
    "TALKING",
    "Talking",
)

ENV_ADMIN_KEYS: tuple[str, ...] = (
    "VIBES_ADMIN_ID",
    "VIBES_TELEGRAM_ADMIN_ID",
    "TELEGRAM_ADMIN_ID",
    "ADMIN_ID",
)

ENV_PYTHON_KEYS: tuple[str, ...] = (
    "VIBES_PYTHON",
    "VIBES_PYTHON_BIN",
)


def default_env_path(root: Path) -> Path:
    return root / ".env"


def cmd_init(root: Path, env_path: Path, *, force: bool) -> int:
    template = (
        "# Telegram bot token (required)\n"
        "VIBES_TOKEN=\n"
        "\n"
        "# Your Telegram numeric user_id (optional)\n"
        "# VIBES_ADMIN_ID=\n"
        "\n"
        "# Codex CLI sandbox mode (optional)\n"
        "# Allowed: read-only | workspace-write | danger-full-access\n"
        "# VIBES_CODEX_SANDBOX=workspace-write\n"
        "\n"
        "# Codex CLI approval policy (optional)\n"
        "# Allowed: untrusted | on-failure | on-request | never\n"
        "# VIBES_CODEX_APPROVAL_POLICY=never\n"
        "\n"
        "# Optional: python interpreter for the bot\n"
        f"# VIBES_PYTHON={(root / '.venv' / 'bin' / 'python')}\n"
    )

    if env_path.exists() and not force:
        print(f".env уже существует: {env_path}", file=sys.stderr)
        return 1

    env_path.write_text(template, encoding="utf-8")
    print(f"Создано: {env_path}")
    return 0


def cmd_start(
    *,
    root: Path,
    env_path: Path,
    token_cli: Optional[str],
    admin_cli: Optional[int],
    python_cli: Optional[str],
    restart: bool,
) -> int:
    rt = runtime_dir(root)
    rt.mkdir(parents=True, exist_ok=True)
    sp = state_path(rt)
    log_path = daemon_log_path(rt)

    state = load_state(sp) or {}
    existing_pid = state.get("pid")
    cmdline: Optional[str]
    looks_like = False
    if isinstance(existing_pid, int) and pid_is_running(existing_pid):
        cmdline = try_get_cmdline(existing_pid)
        looks_like = isinstance(cmdline, str) and looks_like_vibes_process(cmdline, root)
    else:
        cmdline = None

    if isinstance(existing_pid, int) and pid_is_running(existing_pid):
        if restart:
            if not looks_like:
                print(
                    "Найден запущенный pid, но команда не похожа на vibes-бота — не перезапускаю.",
                    file=sys.stderr,
                )
                return 1
            stop_result = cmd_stop(root=root, force=False, timeout_s=10.0)
            if stop_result != 0:
                return stop_result
            state = load_state(sp) or {}
            existing_pid = state.get("pid")
            looks_like = False
        else:
            if looks_like:
                print(f"Уже запущено (pid {existing_pid}).")
                return 0
            print(
                f"Внимание: pid {existing_pid} жив, но не похоже что это vibes-бот.\n"
                f"Проверь: ps -p {existing_pid} -o command=\n"
                f"Если это мусорный pidfile, удали: {sp}",
                file=sys.stderr,
            )
            return 1

    file_env = parse_env_file(env_path)
    token = pick_str(token_cli, file_env, ENV_TOKEN_KEYS)
    if not token:
        print(
            "Не найден токен.\n"
            f"Создай {env_path} (или запусти `vibes init`) и укажи VIBES_TOKEN=...\n"
            "Либо передай `vibes start --token ...`",
            file=sys.stderr,
        )
        return 2

    admin_id = pick_int(admin_cli, file_env, ENV_ADMIN_KEYS)
    python_bin = pick_str(python_cli, file_env, ENV_PYTHON_KEYS)
    if python_bin:
        python_path = Path(python_bin).expanduser()
    else:
        python_path = detect_local_venv_python(root) or Path(sys.executable)

    bot_script = root / "vibes.py"
    if not bot_script.exists():
        print(f"Не найден {bot_script}", file=sys.stderr)
        return 2

    cmd = [str(python_path), str(bot_script)]
    env = os.environ.copy()
    env.update(file_env)
    env["VIBES_TOKEN"] = token
    if admin_id is not None:
        env["VIBES_ADMIN_ID"] = str(admin_id)
    env["PYTHONUNBUFFERED"] = "1"

    started_at = dt.datetime.now(dt.timezone.utc).isoformat()
    with log_path.open("a", encoding="utf-8") as log:
        proc = subprocess.Popen(
            cmd,
            cwd=str(root),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=log,
            start_new_session=True,
        )

    write_state(
        sp,
        {
            "pid": proc.pid,
            "started_at": started_at,
            "cmd": cmd,
            "cwd": str(root),
            "env_path": str(env_path),
            "daemon_log": str(log_path),
        },
    )

    time.sleep(0.5)
    if proc.poll() is not None:
        print(f"Не удалось запустить (процесс сразу завершился). Логи: {log_path}", file=sys.stderr)
        return 1

    print(f"Запущено (pid {proc.pid}). Логи: {log_path}")
    return 0


def cmd_status(*, root: Path) -> int:
    rt = runtime_dir(root)
    sp = state_path(rt)
    state = load_state(sp)
    if not state:
        print("Остановлено.")
        return 3

    pid = state.get("pid")
    if not isinstance(pid, int):
        print(f"Некорректный state-файл: {sp}", file=sys.stderr)
        return 2

    if not pid_is_running(pid):
        print(f"Не запущено (stale pid {pid}).")
        return 3

    cmdline = try_get_cmdline(pid)

    rss_mb: Optional[float] = None
    cpu_pct: Optional[float] = None
    uptime_s: Optional[float] = None
    try:
        import psutil  # type: ignore

        proc = psutil.Process(pid)
        rss_mb = proc.memory_info().rss / (1024 * 1024)
        try:
            proc.cpu_percent(interval=None)
            cpu_pct = proc.cpu_percent(interval=0.05)
        except Exception:
            cpu_pct = None
        try:
            uptime_s = time.time() - float(proc.create_time())
        except Exception:
            uptime_s = None
    except Exception:
        pass

    if rss_mb is None and cpu_pct is None and uptime_s is None:
        try:
            out = subprocess.check_output(
                ["ps", "-p", str(pid), "-o", "etime=,%cpu=,rss="],
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
            fields = out.split()
            if len(fields) >= 3:
                uptime_s = parse_ps_etime(fields[0])
                try:
                    cpu_pct = float(fields[1])
                except ValueError:
                    cpu_pct = None
                try:
                    rss_mb = float(fields[2]) / 1024.0
                except ValueError:
                    rss_mb = None
        except Exception:
            pass

    parts = [f"Запущено (pid {pid})"]
    if uptime_s is not None:
        parts.append(f"uptime {format_timedelta(uptime_s)}")
    if cpu_pct is not None:
        parts.append(f"cpu {cpu_pct:.1f}%")
    if rss_mb is not None:
        parts.append(f"rss {rss_mb:.1f}MB")

    print(" · ".join(parts))
    if cmdline:
        print(cmdline)
    daemon_log = state.get("daemon_log")
    if isinstance(daemon_log, str) and daemon_log:
        print(f"Логи: {daemon_log}")
    return 0


def cmd_stop(*, root: Path, force: bool, timeout_s: float) -> int:
    rt = runtime_dir(root)
    sp = state_path(rt)
    state = load_state(sp)
    if not state:
        print("Уже остановлено.")
        return 0

    pid = state.get("pid")
    if not isinstance(pid, int):
        print(f"Некорректный state-файл: {sp}", file=sys.stderr)
        return 2

    if not pid_is_running(pid):
        try:
            sp.unlink()
        except Exception:
            pass
        print("Уже остановлено.")
        return 0

    cmdline = try_get_cmdline(pid)
    if cmdline is None and not force:
        print(
            "Не могу безопасно проверить что PID принадлежит vibes-боту.\n"
            f"Проверь вручную: ps -p {pid} -o command=\n"
            "Или используй `vibes stop --force`.",
            file=sys.stderr,
        )
        return 1
    if isinstance(cmdline, str) and not looks_like_vibes_process(cmdline, root) and not force:
        print(
            "PID жив, но команда не похожа на vibes-бота — не останавливаю.\n"
            f"{cmdline}\n"
            "Если уверен, используй `vibes stop --force`.",
            file=sys.stderr,
        )
        return 1

    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    except PermissionError as exc:
        print(f"Нет прав остановить pid {pid}: {exc}", file=sys.stderr)
        return 1

    deadline = time.time() + max(0.0, timeout_s)
    while time.time() < deadline:
        if not pid_is_running(pid):
            break
        time.sleep(0.2)

    if pid_is_running(pid):
        try:
            os.kill(pid, signal.SIGKILL)
        except Exception:
            pass

    try:
        sp.unlink()
    except Exception:
        pass

    print("Остановлено.")
    return 0


def cmd_setup(
    *,
    root: Path,
    env_path: Path,
    start: bool,
    restart: bool,
    python_cli: Optional[str],
) -> int:
    file_env = parse_env_file(env_path)

    token = pick_str(None, file_env, ENV_TOKEN_KEYS)
    if not token:
        token = getpass.getpass("Telegram bot token (VIBES_TOKEN): ").strip()
    if not token:
        print("Пустой токен.", file=sys.stderr)
        return 2
    if "\n" in token or "\r" in token:
        print("Токен содержит перевод строки.", file=sys.stderr)
        return 2

    admin_id = pick_int(None, file_env, ENV_ADMIN_KEYS)
    if admin_id is None and sys.stdin.isatty():
        raw = input("Admin user_id (опционально, Enter чтобы пропустить): ").strip()
        if raw:
            try:
                admin_id = int(raw)
            except ValueError:
                print("Admin user_id должен быть числом (или пусто).", file=sys.stderr)
                return 2

    updates: Dict[str, Optional[str]] = {"VIBES_TOKEN": token}
    if admin_id is not None:
        updates["VIBES_ADMIN_ID"] = str(admin_id)
    update_env_file(env_path, updates)
    print(f"Готово: {env_path}")

    if start:
        return cmd_start(
            root=root,
            env_path=env_path,
            token_cli=None,
            admin_cli=None,
            python_cli=python_cli,
            restart=restart,
        )
    return 0


def cmd_logs(*, root: Path, follow: bool) -> int:
    rt = runtime_dir(root)
    sp = state_path(rt)
    state = load_state(sp) or {}
    daemon_log_raw = state.get("daemon_log")
    if isinstance(daemon_log_raw, str) and daemon_log_raw.strip():
        log_path = Path(daemon_log_raw).expanduser()
    else:
        log_path = daemon_log_path(rt)

    print(str(log_path))
    if not follow:
        return 0

    try:
        from collections import deque

        with log_path.open("r", encoding="utf-8", errors="replace") as f:
            tail = deque(f, maxlen=200)
        for line in tail:
            sys.stdout.write(line)
        sys.stdout.flush()
    except FileNotFoundError:
        print("Файл логов ещё не создан.", file=sys.stderr)
    except Exception:
        pass

    try:
        with log_path.open("r", encoding="utf-8", errors="replace") as f:
            f.seek(0, os.SEEK_END)
            while True:
                line = f.readline()
                if line:
                    sys.stdout.write(line)
                    sys.stdout.flush()
                    continue
                time.sleep(0.2)
    except KeyboardInterrupt:
        return 0
    except FileNotFoundError:
        return 1
    except Exception as exc:
        print(f"Не удалось читать логи: {exc}", file=sys.stderr)
        return 1

