"""Check smoke-command failure behavior using isolated external processes."""

import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
import json
import shutil
import time


ROOT = Path(__file__).resolve().parents[1]


class SmokeCommandTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="rmusic-smoke-command-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.bin = self.directory / "bin"
        self.bin.mkdir()
        self.log = self.directory / "calls.jsonl"
        self.logs = self.directory / "logs"
        self.logs.mkdir()
        self.binary = self.directory / "custom target" / "rmusic"
        self.binary.parent.mkdir()
        self.binary.write_text("#!/bin/sh\nexit 0\n")
        self.binary.chmod(0o755)
        fixture = (
            f"#!{sys.executable}\n"
            "import json, os, pathlib, sys\n"
            "tool = pathlib.Path(sys.argv[0]).name\n"
            "with open(os.environ['RMUSIC_SMOKE_LOG'], 'a') as log:\n"
            "    log.write(json.dumps({'tool': tool, 'args': sys.argv[1:], 'cwd': os.getcwd()}) + '\\n')\n"
            "phase = ('build-parser' if sys.argv[1] == '-c' else 'terminal') if tool == 'python3' else sys.argv[1] if len(sys.argv) > 1 else tool\n"
            "if os.environ.get('RMUSIC_SMOKE_FAIL') == phase:\n"
            "    print('fixture failure diagnostic', file=sys.stderr)\n"
            "    sys.exit(43)\n"
            "if tool == 'python3' and phase == 'build-parser':\n"
            "    os.execv(sys.executable, [sys.executable, *sys.argv[1:]])\n"
            "if tool == 'cargo' and phase == 'build' and not os.environ.get('RMUSIC_SMOKE_NO_ARTIFACT'):\n"
            "    print(json.dumps({'reason': 'compiler-artifact', 'target': {'name': 'rmusic', 'kind': ['bin']}, 'executable': os.environ['RMUSIC_SMOKE_BINARY']}))\n"
            "if tool == 'cargo' and phase == 'test':\n"
            "    count = 0 if os.environ.get('RMUSIC_SMOKE_EMPTY') else 1\n"
            "    print(f'test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out')\n"
        )
        for tool in ["cargo", "mpv", "python3"]:
            path = self.bin / tool
            path.write_text(fixture)
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        RMUSIC_SMOKE_LOG=str(self.log), RMUSIC_SMOKE_BINARY=str(self.binary),
                        TMPDIR=str(self.logs))
        for name in ["RMUSIC_SMOKE_FAIL", "RMUSIC_SMOKE_EMPTY", "RMUSIC_SMOKE_NO_ARTIFACT"]:
            self.env.pop(name, None)

    def run_smoke(self, **overrides):
        return subprocess.run(["bash", str(ROOT / "scripts/smoke")],
                              cwd=self.directory, env=dict(self.env, **overrides),
                              capture_output=True, text=True, timeout=15)

    def test_zero_matching_tests_cannot_make_smoke_checks_green(self):
        result = self.run_smoke(RMUSIC_SMOKE_EMPTY="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required mpv smoke test did not run", result.stderr)

    def test_success_builds_locked_and_runs_only_the_named_opt_in_test_and_terminal(self):
        result = self.run_smoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual([call["args"] for call in calls if call["tool"] == "cargo"], [
            ["build", "--locked", "--bin", "rmusic", "--message-format=json-render-diagnostics"],
            ["test", "--locked", "--bin", "rmusic",
                       "acceptance::real_mpv_plays_local_audio_and_is_reaped_on_stop",
                       "--", "--ignored", "--exact", "--nocapture", "--color", "never"],
        ])
        self.assertIn({"tool": "python3", "args": ["scripts/smoke-terminal.py", "--binary", str(self.binary)], "cwd": str(ROOT)}, calls)
        self.assertTrue(all(call["cwd"] == str(ROOT) for call in calls))
        self.assertFalse(list(self.logs.iterdir()), "temporary player log was not cleaned up")

    def test_failure_in_either_smoke_phase_fails_the_command_with_diagnostics(self):
        for phase in ["build", "test", "terminal"]:
            with self.subTest(phase=phase):
                self.log.unlink(missing_ok=True)
                result = self.run_smoke(RMUSIC_SMOKE_FAIL=phase)
                self.assertEqual(result.returncode, 43)
                self.assertIn("fixture failure diagnostic", result.stdout + result.stderr)
                self.assertIn("FAILED:", result.stdout + result.stderr)
                self.assertFalse(list(self.logs.iterdir()))

    def test_missing_mpv_is_an_error_instead_of_a_skip(self):
        (self.bin / "mpv").unlink()
        for tool in ["bash", "timeout", "tee", "grep", "mktemp", "rm", "dirname", "uname"]:
            (self.bin / tool).symlink_to(shutil.which(tool))
        result = self.run_smoke(PATH=str(self.bin))
        self.assertEqual(result.returncode, 127)
        self.assertIn("required smoke tool missing: mpv", result.stderr)

    def test_command_timeout_is_a_failure(self):
        timeout = self.bin / "timeout"
        timeout.write_text("#!/bin/sh\nexit 124\n")
        timeout.chmod(0o755)
        result = self.run_smoke()
        self.assertEqual(result.returncode, 124)
        self.assertIn("FAILED: build", result.stderr)
        self.assertFalse(list(self.logs.iterdir()))

    def test_missing_build_artifact_cannot_fall_back_to_a_stale_binary(self):
        result = self.run_smoke(RMUSIC_SMOKE_NO_ARTIFACT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("build did not identify an executable rmusic artifact", result.stderr)


class TerminalFailureTests(unittest.TestCase):
    def run_fixture(self, body, expected):
        with tempfile.TemporaryDirectory(prefix="rmusic-smoke-failure-") as directory:
            directory = Path(directory)
            pid_file = directory / "fixture.pid"
            fixture = directory / "unresponsive-player"
            fixture.write_text(
                f"#!{sys.executable}\n"
                "import os, pathlib, signal, time, tty\n"
                "signal.signal(signal.SIGHUP, signal.SIG_IGN)\n"
                f"pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
                + body
            )
            fixture.chmod(0o755)
            pid = None
            try:
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/smoke-terminal.py"),
                     "--binary", str(fixture), "--timeout", "1"],
                    capture_output=True, text=True, timeout=15,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(expected, result.stderr)
                pid = int(pid_file.read_text())
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)
                return result
            finally:
                if pid is None and pid_file.exists():
                    pid = int(pid_file.read_text())
                if pid is not None:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_startup_timeout_kills_and_reaps_the_fixture_child(self):
        self.run_fixture("time.sleep(60)\n", "startup frame: timed out")

    def test_startup_exit_retains_the_child_diagnostic(self):
        result = self.run_fixture(
            "print('fixture startup diagnostic', flush=True)\nraise SystemExit(23)\n",
            "startup frame: child exited with status 23",
        )
        self.assertIn("fixture startup diagnostic", result.stderr)

    def test_shutdown_timeout_kills_and_reaps_the_fixture_child(self):
        self.run_fixture(
            "tty.setraw(0)\n"
            "os.write(1, b'Favorites Stopped')\n"
            "signal.signal(signal.SIGWINCH, lambda *_: os.write(1, b'80x24'))\n"
            "while True:\n"
            "    data = os.read(0, 1024)\n"
            "    os.write(1, data)\n"
            "    if b'/' in data:\n"
            "        os.write(1, b'Search filters')\n",
            "exit: timed out",
        )

    def test_startup_failure_also_terminates_descendants_after_the_leader_exits(self):
        with tempfile.TemporaryDirectory(prefix="rmusic-smoke-descendant-") as directory:
            directory = Path(directory)
            descendant_file = directory / "descendant.pid"
            fixture = directory / "failed-parent"
            fixture.write_text(
                f"#!{sys.executable}\nimport os,pathlib,signal,time\n"
                "signal.signal(signal.SIGHUP, signal.SIG_IGN)\n"
                "child = os.fork()\n"
                "if child == 0:\n    time.sleep(60)\n    os._exit(0)\n"
                f"pathlib.Path({str(descendant_file)!r}).write_text(str(child))\n"
                "raise SystemExit(23)\n"
            )
            fixture.chmod(0o755)
            pid = None
            try:
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/smoke-terminal.py"),
                     "--binary", str(fixture), "--timeout", "1"],
                    capture_output=True, text=True, timeout=10,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("startup frame", result.stderr)
                pid = int(descendant_file.read_text())
                status = Path(f"/proc/{pid}/stat")
                deadline = time.monotonic() + 1
                while time.monotonic() < deadline:
                    try:
                        if status.read_text().rsplit(")", 1)[1].split()[0] == "Z":
                            break
                    except FileNotFoundError:
                        break
                    time.sleep(0.01)
                if status.exists():
                    self.assertEqual(status.read_text().rsplit(")", 1)[1].split()[0], "Z",
                                     "descendant was still running after startup cleanup")
            finally:
                if pid is None and descendant_file.exists():
                    pid = int(descendant_file.read_text())
                if pid is not None:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    unittest.main()
