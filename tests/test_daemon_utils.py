import os
import unittest
from importlib.machinery import SourceFileLoader
from importlib.util import module_from_spec, spec_from_loader
from pathlib import Path
from tempfile import TemporaryDirectory


def _load_daemon_module():
    repo_root = Path(__file__).resolve().parents[1]
    daemon_path = repo_root / "vibes"  # executable python script (no .py suffix)
    spec = spec_from_loader("vibes_daemon", SourceFileLoader("vibes_daemon", str(daemon_path)))
    assert spec is not None
    mod = module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)  # type: ignore[assignment]
    return mod


vibes_daemon = _load_daemon_module()


class EnvFileParsingTests(unittest.TestCase):
    def test_parse_env_file_supports_export_comments_and_quotes(self) -> None:
        with TemporaryDirectory() as td:
            env_path = Path(td) / ".env"
            env_path.write_text(
                "\n".join(
                    [
                        "# comment",
                        "export VIBES_TOKEN='abc'",
                        'VIBES_ADMIN_ID=123 # trailing comment',
                        "EMPTY=",
                        "BADLINE",
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            parsed = vibes_daemon._parse_env_file(env_path)
            self.assertEqual(parsed.get("VIBES_TOKEN"), "abc")
            self.assertEqual(parsed.get("VIBES_ADMIN_ID"), "123")
            self.assertEqual(parsed.get("EMPTY"), "")
            self.assertNotIn("BADLINE", parsed)

    def test_pick_str_precedence_cli_env_file(self) -> None:
        file_env = {"VIBES_TOKEN": "from_file"}
        keys = ("VIBES_TOKEN",)

        old = os.environ.get("VIBES_TOKEN")
        try:
            os.environ["VIBES_TOKEN"] = "from_env"
            self.assertEqual(vibes_daemon._pick_str("from_cli", file_env, keys), "from_cli")
            self.assertEqual(vibes_daemon._pick_str("   ", file_env, keys), "from_env")
            del os.environ["VIBES_TOKEN"]
            self.assertEqual(vibes_daemon._pick_str(None, file_env, keys), "from_file")
        finally:
            if old is None:
                os.environ.pop("VIBES_TOKEN", None)
            else:
                os.environ["VIBES_TOKEN"] = old

    def test_pick_int_reads_env_and_ignores_non_int(self) -> None:
        file_env = {"VIBES_ADMIN_ID": "42"}
        keys = ("VIBES_ADMIN_ID",)

        old = os.environ.get("VIBES_ADMIN_ID")
        try:
            os.environ["VIBES_ADMIN_ID"] = "not-int"
            self.assertIsNone(vibes_daemon._pick_int(None, file_env, keys))

            os.environ.pop("VIBES_ADMIN_ID", None)
            self.assertEqual(vibes_daemon._pick_int(None, file_env, keys), 42)

            os.environ["VIBES_ADMIN_ID"] = "100"
            self.assertEqual(vibes_daemon._pick_int(None, file_env, keys), 100)
        finally:
            if old is None:
                os.environ.pop("VIBES_ADMIN_ID", None)
            else:
                os.environ["VIBES_ADMIN_ID"] = old


class DaemonStateTests(unittest.TestCase):
    def test_write_state_roundtrip(self) -> None:
        with TemporaryDirectory() as td:
            p = Path(td) / "daemon.json"
            vibes_daemon._write_state(p, {"pid": 123, "ok": True})
            loaded = vibes_daemon._load_state(p)
            self.assertEqual(loaded, {"pid": 123, "ok": True})

    def test_looks_like_vibes_process_matches_known_patterns(self) -> None:
        root = Path("D:/Projects/CodexMobile").resolve()
        bot_path = str((root / "vibes.py").resolve())

        self.assertTrue(vibes_daemon._looks_like_vibes_process(f"python {bot_path}", root))
        self.assertTrue(vibes_daemon._looks_like_vibes_process(f"python -m vibes", root))
        self.assertTrue(vibes_daemon._looks_like_vibes_process(f"/usr/bin/python -m vibes", root))
        self.assertFalse(vibes_daemon._looks_like_vibes_process("python something_else.py", root))


class PsEtimeParsingTests(unittest.TestCase):
    def test_parse_ps_etime_variants(self) -> None:
        self.assertEqual(vibes_daemon._parse_ps_etime("00:01"), 1)
        self.assertEqual(vibes_daemon._parse_ps_etime("01:02"), 62)
        self.assertEqual(vibes_daemon._parse_ps_etime("01:02:03"), 3723)
        self.assertEqual(vibes_daemon._parse_ps_etime("2-00:00:00"), 2 * 24 * 3600)
        self.assertIsNone(vibes_daemon._parse_ps_etime(""))
        self.assertIsNone(vibes_daemon._parse_ps_etime("bad"))
