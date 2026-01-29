from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Optional, Sequence

from vibes_app.daemon.commands import cmd_init, cmd_logs, cmd_setup, cmd_start, cmd_status, cmd_stop, default_env_path
from vibes_app.daemon.process import maybe_reexec_into_venv


def build_parser() -> tuple[argparse.ArgumentParser, argparse._SubParsersAction]:
    p = argparse.ArgumentParser(prog="vibes", add_help=True)
    sub = p.add_subparsers(dest="command")

    p_start = sub.add_parser("start", help="Запустить в фоне")
    p_start.add_argument("-r", "--restart", action="store_true", help="Остановить и перезапустить, если бот уже работает")
    p_start.add_argument("--token", default=None, help="Telegram bot token (иначе из .env/env)")
    p_start.add_argument("--admin", type=int, default=None, help="Telegram user_id (опционально)")
    p_start.add_argument("--python", default=None, help="Путь до python (иначе .venv или текущий)")
    p_start.add_argument("--env", dest="env_path", default=None, help="Путь до .env (по умолчанию рядом со скриптом)")

    sub.add_parser("status", help="Статус")

    p_stop = sub.add_parser("stop", help="Остановить")
    p_stop.add_argument("--force", action="store_true", help="Остановить без проверки команды процесса")
    p_stop.add_argument("--timeout", type=float, default=10.0, help="Сколько секунд ждать SIGTERM")

    p_init = sub.add_parser("init", help="Создать .env по шаблону")
    p_init.add_argument("--force", action="store_true", help="Перезаписать существующий .env")
    p_init.add_argument("--env", dest="env_path", default=None, help="Путь до .env (по умолчанию рядом со скриптом)")

    p_setup = sub.add_parser("setup", help="Интерактивно создать/обновить .env")
    p_setup.add_argument("--start", action="store_true", help="Сразу запустить бота")
    p_setup.add_argument(
        "-r",
        "--restart",
        action="store_true",
        help="С рестартом (если бот уже работает)",
    )
    p_setup.add_argument("--python", default=None, help="Путь до python (иначе .venv или текущий)")
    p_setup.add_argument("--env", dest="env_path", default=None, help="Путь до .env (по умолчанию рядом со скриптом)")

    p_logs = sub.add_parser("logs", help="Путь к daemon.log")
    p_logs.add_argument("-f", "--follow", action="store_true", help="Следить за логом (tail -f)")

    p_help = sub.add_parser("help", help="Показать справку")
    p_help.add_argument("topic", nargs="?", help="Команда (start/status/stop/setup/logs/init)")

    return p, sub


def main(argv: Optional[Sequence[str]] = None, *, root: Optional[Path] = None) -> int:
    if root is None:
        root = Path(__file__).resolve().parents[3]
    if argv is None:
        maybe_reexec_into_venv(root)
    parser, sub = build_parser()
    raw_argv = list(argv) if argv is not None else sys.argv[1:]
    if raw_argv and raw_argv[0] == "help":
        if len(raw_argv) == 1:
            parser.print_help()
            return 0
        topic = raw_argv[1]
        if topic in sub.choices:
            sub.choices[topic].print_help()
            return 0
        print(f"Неизвестная команда: {topic}", file=sys.stderr)
        parser.print_help(sys.stderr)
        return 2
    if raw_argv and raw_argv[0].startswith("-"):
        raw_argv = ["start", *raw_argv]
    ns = parser.parse_args(raw_argv)

    cmd = ns.command or "start"

    env_path = Path(getattr(ns, "env_path", None) or default_env_path(root)).expanduser()

    if cmd == "init":
        return cmd_init(root, env_path, force=bool(ns.force))
    if cmd == "start":
        return cmd_start(
            root=root,
            env_path=env_path,
            token_cli=getattr(ns, "token", None),
            admin_cli=getattr(ns, "admin", None),
            python_cli=getattr(ns, "python", None),
            restart=bool(getattr(ns, "restart", False)),
        )
    if cmd == "status":
        return cmd_status(root=root)
    if cmd == "stop":
        return cmd_stop(root=root, force=bool(ns.force), timeout_s=float(ns.timeout))
    if cmd == "setup":
        return cmd_setup(
            root=root,
            env_path=env_path,
            start=bool(ns.start),
            restart=bool(getattr(ns, "restart", False)),
            python_cli=getattr(ns, "python", None),
        )
    if cmd == "logs":
        return cmd_logs(root=root, follow=bool(ns.follow))
    if cmd == "help":
        topic = getattr(ns, "topic", None)
        if isinstance(topic, str) and topic.strip():
            if topic in sub.choices:
                sub.choices[topic].print_help()
                return 0
            print(f"Неизвестная команда: {topic}", file=sys.stderr)
            parser.print_help(sys.stderr)
            return 2
        parser.print_help()
        return 0

    parser.print_help(sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
