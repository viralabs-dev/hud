"""Uso do Codex: parse do `rate_limits` e leitura dos rollouts (fixtures sintéticas)."""

import json
import os
import tempfile
import unittest
from pathlib import Path

from hud import usage as us

REAL = {"limit_id": "codex", "limit_name": None,
        "primary": {"used_percent": 1.0, "window_minutes": 10080, "resets_at": 1791979352},
        "secondary": None,
        "credits": {"has_credits": False, "unlimited": False, "balance": "0"},
        "individual_limit": None, "spend_control_reached": None,
        "plan_type": "prolite", "rate_limit_reached_type": None}


def token_count(rl: dict) -> str:
    return json.dumps({"timestamp": "2026-10-08T00:00:00Z", "type": "event_msg",
                       "payload": {"type": "token_count", "info": None, "rate_limits": rl}})


def write_rollout(path: Path, lines: list[str], mtime: float | None = None) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")
    os.chmod(path, 0o600)
    if mtime is not None:
        os.utime(path, (mtime, mtime))
    return path


class ParseTest(unittest.TestCase):
    def test_real_format(self):
        u = us.parse_codex_rate_limits(REAL, 123.0)
        self.assertEqual(u.plan, "prolite")
        self.assertEqual(u.fetched_at, 123.0)
        self.assertEqual(len(u.windows), 1)
        w = u.windows[0]
        self.assertEqual((w.label, w.window_minutes, w.used, w.resets_at),
                         ("semana", 10080, 1.0, 1791979352.0))
        self.assertEqual(w.left, 99.0)

    def test_primary_and_secondary(self):
        rl = dict(REAL, primary={"used_percent": 40, "window_minutes": 300, "resets_at": 10},
                  secondary={"used_percent": 12.5, "window_minutes": 10080, "resets_at": None},
                  extra_desconhecido={"x": 1})
        u = us.parse_codex_rate_limits(rl)
        self.assertEqual([w.label for w in u.windows], ["5h", "semana"])
        self.assertEqual(u.windows[0].left, 60)
        self.assertIsNone(u.windows[1].resets_at)

    def test_both_null_and_plan_cleaned(self):
        u = us.parse_codex_rate_limits({"primary": None, "secondary": None,
                                        "plan_type": "pro\x1b[31m\nx"})
        self.assertEqual(u.windows, [])
        self.assertNotIn("\x1b", u.plan)
        self.assertNotIn("\n", u.plan)
        self.assertEqual(us.parse_codex_rate_limits({"plan_type": 5}).plan, "")

    def test_invalid_values(self):
        def win(**kw):
            base = {"used_percent": 10, "window_minutes": 300, "resets_at": 1}
            base.update(kw)
            return us.parse_codex_rate_limits({"primary": base})
        self.assertIsNone(win(used_percent=101))
        self.assertIsNone(win(used_percent=-1))
        self.assertIsNone(win(used_percent="10"))
        self.assertIsNone(win(used_percent=True))
        self.assertIsNone(win(window_minutes=0))
        self.assertIsNone(win(window_minutes=-5))
        self.assertIsNone(win(window_minutes="300"))
        self.assertIsNone(win(resets_at="amanhã"))
        self.assertIsNone(us.parse_codex_rate_limits({"primary": "x"}))
        self.assertIsNone(us.parse_codex_rate_limits(None))
        self.assertIsNone(us.parse_codex_rate_limits([1]))

    def test_window_label(self):
        self.assertEqual([us.window_label(m) for m in (300, 10080, 1440, 2880, 60, 120)],
                         ["5h", "semana", "1d", "2d", "1h", "2h"])

    def test_codex_home(self):
        old = os.environ.get("CODEX_HOME")
        try:
            os.environ["CODEX_HOME"] = "/x/codex"
            self.assertEqual(us.codex_home(), Path("/x/codex"))
            del os.environ["CODEX_HOME"]
            self.assertEqual(us.codex_home(), Path.home() / ".codex")
        finally:
            if old is not None:
                os.environ["CODEX_HOME"] = old


class TrackerTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_nested_in_payload_and_last_wins(self):
        p = write_rollout(self.root / "2026/10/08/rollout-a.jsonl", [
            '{"type":"session_meta","payload":{}}',
            token_count(dict(REAL, primary={"used_percent": 5, "window_minutes": 10080})),
            token_count(dict(REAL, primary={"used_percent": 7, "window_minutes": 10080})),
            token_count({"primary": {"used_percent": 900, "window_minutes": 1}}),  # inválido
            "linha quebrada {",
        ], mtime=1000)
        t = us.CodexUsageTracker(self.root)
        self.assertTrue(t.poll())
        self.assertEqual(t.current.windows[0].used, 7)
        self.assertEqual(t.current.fetched_at, 1000)
        self.assertFalse(t.poll())  # nada mudou
        with p.open("a") as f:
            f.write(token_count(dict(REAL, primary={"used_percent": 9, "window_minutes": 10080})) + "\n")
        os.utime(p, (2000, 2000))
        self.assertTrue(t.poll())
        self.assertEqual(t.current.windows[0].used, 9)
        self.assertEqual(t.current.fetched_at, 2000)

    def test_picks_newest_across_date_dirs(self):
        line = lambda u: [token_count(dict(REAL, primary={"used_percent": u, "window_minutes": 300}))]
        write_rollout(self.root / "2026/09/30/rollout-old.jsonl", line(1), mtime=100)
        write_rollout(self.root / "2026/10/01/rollout-a.jsonl", line(2), mtime=200)
        write_rollout(self.root / "2026/10/01/rollout-b.jsonl", line(3), mtime=500)
        write_rollout(self.root / "2026/10/02/rollout-c.jsonl", line(4), mtime=300)
        (self.root / "2026/10/02/outro.jsonl").write_text("x")
        self.assertEqual(us.latest_rollout(self.root).name, "rollout-b.jsonl")
        t = us.CodexUsageTracker(self.root)
        self.assertTrue(t.poll())
        self.assertEqual(t.current.windows[0].used, 3)

    def test_only_three_most_recent_dates(self):
        line = [token_count(REAL)]
        write_rollout(self.root / "2025/12/31/rollout-z.jsonl", line, mtime=9999)
        for d in ("01", "02", "03"):
            write_rollout(self.root / f"2026/01/{d}/rollout-{d}.jsonl", line, mtime=100)
        self.assertEqual(us.latest_rollout(self.root).name, "rollout-03.jsonl")

    def test_rejects_world_writable(self):
        p = write_rollout(self.root / "2026/10/08/rollout-a.jsonl", [token_count(REAL)])
        os.chmod(p, 0o666)
        self.assertFalse(us.CodexUsageTracker(self.root).poll())
        self.assertIsNone(us.read_codex_rollout(p))
        os.chmod(p, 0o600)
        t = us.CodexUsageTracker(self.root)
        self.assertTrue(t.poll())
        self.assertEqual(t.current.plan, "prolite")

    def test_rejects_symlink(self):
        real = write_rollout(self.root / "fora.jsonl", [token_count(REAL)])
        d = self.root / "2026/10/08"
        d.mkdir(parents=True)
        (d / "rollout-link.jsonl").symlink_to(real)
        self.assertIsNone(us.read_codex_rollout(d / "rollout-link.jsonl"))
        self.assertFalse(us.CodexUsageTracker(self.root).poll())

    @unittest.skipUnless(os.getuid() == 0, "chown precisa de root")
    def test_rejects_foreign_owner(self):
        p = write_rollout(self.root / "2026/10/08/rollout-a.jsonl", [token_count(REAL)])
        os.chown(p, 65534, 65534)
        self.assertIsNone(us.read_codex_rollout(p))

    def test_foreign_owner_by_uid(self):
        p = write_rollout(self.root / "2026/10/08/rollout-a.jsonl", [token_count(REAL)])
        real_getuid = os.getuid
        os.getuid = lambda: real_getuid() + 1
        try:
            self.assertIsNone(us.read_codex_rollout(p))
        finally:
            os.getuid = real_getuid
        self.assertIsNotNone(us.read_codex_rollout(p))

    def test_only_last_256kb_read(self):
        filler = json.dumps({"type": "response_item", "payload": {"text": "x" * 1000}})
        lines = [token_count(REAL)] + [filler] * 300  # ~300 KB depois do rate_limits
        p = write_rollout(self.root / "2026/10/08/rollout-a.jsonl", lines)
        self.assertGreater(p.stat().st_size, us.CODEX_TAIL)
        self.assertIsNone(us.read_codex_rollout(p))
        self.assertFalse(us.CodexUsageTracker(self.root).poll())
        self.assertIsNotNone(us.read_codex_rollout(p, tail=p.stat().st_size))

    def test_missing_dir(self):
        self.assertFalse(us.CodexUsageTracker(self.root / "nada").poll())


if __name__ == "__main__":
    unittest.main()
