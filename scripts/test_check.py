"""Exercise the verification command through its process boundary."""

import json
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check"


class CheckCommandTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="rmusic-check-contract-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.bin = self.directory / "bin"
        self.bin.mkdir()
        self.log = self.directory / "calls.jsonl"
        cargo = self.bin / "cargo"
        cargo.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            "with open(os.environ['RMUSIC_CHECK_LOG'], 'a') as log:\n"
            "    log.write(json.dumps({'args': sys.argv[1:], 'cwd': os.getcwd()}) + '\\n')\n"
            "if sys.argv[1] == os.environ.get('RMUSIC_CHECK_FAIL'):\n"
            "    print('fixture diagnostic: selected cargo phase failed', file=sys.stderr)\n"
            "    sys.exit(43)\n"
        )
        cargo.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        RMUSIC_CHECK_LOG=str(self.log))
        self.env.pop("RMUSIC_CHECK_FAIL", None)
        self.env.pop("CARGO_BUILD_WARNINGS", None)

    def run_check(self, **overrides):
        return subprocess.run(["bash", str(CHECK)], cwd=self.directory,
                              env=dict(self.env, **overrides), capture_output=True,
                              text=True, timeout=30)

    def test_failed_phase_preserves_status_and_diagnostic(self):
        phases = [("fmt", "format"), ("check", "check"), ("clippy", "clippy"), ("test", "test")]
        for index, (command, label) in enumerate(phases):
            with self.subTest(phase=label):
                self.log.unlink(missing_ok=True)
                result = self.run_check(RMUSIC_CHECK_FAIL=command)
                self.assertEqual(result.returncode, 43)
                self.assertIn(f"FAILED: {label}", result.stderr)
                self.assertIn("fixture diagnostic", result.stderr)
                calls = [json.loads(line)["args"][0] for line in self.log.read_text().splitlines()]
                self.assertEqual(calls, [phase[0] for phase in phases[:index + 1]])

    def test_success_runs_all_required_checks_with_locking_and_strict_clippy(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual([call["args"] for call in calls], [
            ["fmt", "--all", "--", "--check"],
            ["check", "--workspace", "--all-targets", "--locked"],
            ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
            ["test", "--workspace", "--locked"],
        ])
        self.assertTrue(all(call["cwd"] == str(ROOT) for call in calls))

    def test_missing_cargo_fails_with_installation_guidance(self):
        (self.bin / "cargo").unlink()
        for name in ["bash", "dirname"]:
            (self.bin / name).symlink_to(shutil.which(name))
        result = self.run_check(PATH=str(self.bin))
        self.assertEqual(result.returncode, 127)
        self.assertIn("cargo not found", result.stderr)
        self.assertIn("stable Rust toolchain", result.stderr)


class RustFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="rmusic check fixture ")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        (self.directory / "scripts").mkdir()
        (self.directory / "src").mkdir()
        self.check = self.directory / "scripts" / "check"
        shutil.copy2(CHECK, self.check)
        self.manifest = self.directory / "Cargo.toml"
        self.manifest.write_text('[package]\nname = "check_fixture"\nversion = "0.1.0"\nedition = "2024"\n')
        self.source = self.directory / "src" / "lib.rs"
        self.source.write_text("pub fn answer() -> i32 {\n    4\n}\n")
        self.env = dict(os.environ, CARGO_TERM_COLOR="never")
        self.env.pop("CARGO_TARGET_DIR", None)
        self.env.pop("CARGO_BUILD_WARNINGS", None)
        subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=self.directory,
                       env=self.env, check=True, capture_output=True, text=True, timeout=30)

    def run_fixture(self):
        files = [self.manifest, self.source, self.directory / "Cargo.lock"]
        before = [file.read_bytes() for file in files]
        result = subprocess.run([str(self.check)], cwd=ROOT, env=self.env,
                                capture_output=True, text=True, timeout=120)
        self.assertEqual([file.read_bytes() for file in files], before,
                         "verification changed source, manifest, or lockfile")
        return result

    def assert_phase_fails(self, phase):
        result = self.run_fixture()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(f"FAILED: {phase}", result.stderr)

    def test_clean_workspace_passes_without_modification(self):
        result = self.run_fixture()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for phase in ["format", "check", "clippy", "test"]:
            self.assertIn(f"PASSED: {phase}", result.stdout)

    def test_format_violation_fails_without_reformatting(self):
        self.source.write_text("pub fn answer()->i32{4}\n")
        self.assert_phase_fails("format")

    def test_compile_error_fails_in_the_compilation_phase(self):
        self.source.write_text("pub fn answer() -> i32 {\n    missing_answer\n}\n")
        self.assert_phase_fails("check")

    def test_clippy_warning_is_a_failure(self):
        self.source.write_text("pub fn answer() -> i32 {\n    let result = 4;\n    result\n}\n")
        self.assert_phase_fails("clippy")

    def test_failed_behavior_test_fails_verification(self):
        self.source.write_text(
            "pub fn answer() -> i32 {\n    4\n}\n\n"
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_correct() {\n"
            "        assert_eq!(super::answer(), 5);\n    }\n}\n"
        )
        self.assert_phase_fails("test")


if __name__ == "__main__":
    unittest.main()
