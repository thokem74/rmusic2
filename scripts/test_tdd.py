#!/usr/bin/env python3
"""Exercise the TDD command at its agreed temporary-workspace boundary."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

DRIVER = Path(__file__).resolve().with_name('tdd')


class TddTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'src').mkdir()
        (self.root / 'Cargo.toml').write_text(
            '[package]\nname="tdd-fixture"\nversion="0.1.0"\nedition="2024"\n'
        )
        self.source = self.root / 'src/lib.rs'
        self.write_behavior(False)
        subprocess.run(['cargo', 'generate-lockfile', '--offline'], cwd=self.root,
                       check=True, capture_output=True)

    def write_behavior(self, correct):
        self.source.write_text(
            f'pub fn answer() -> u8 {{ {42 if correct else 0} }}\n'
            '#[test] fn behavior() { assert_eq!(answer(), 42, "intended diagnostic"); }\n'
        )

    def run_driver(self, phase, selection='behavior', **kwargs):
        return subprocess.run([str(DRIVER), phase, 'answer', selection], cwd=self.root,
                              text=True, capture_output=True, **kwargs)

    def record(self):
        return json.loads((self.root / '.tdd-evidence/answer/record.json').read_text())

    def test_observed_red_then_same_selection_green(self):
        red = self.run_driver('red')
        self.assertEqual(red.returncode, 0, red.stderr + red.stdout)
        self.assertEqual(self.record()['status'], 'red_verified')
        self.assertIn('intended diagnostic', red.stdout)
        self.assertIn(str(self.root / '.tdd-evidence/answer'), red.stdout)
        self.write_behavior(True)
        green = self.run_driver('green')
        self.assertEqual(green.returncode, 0, green.stderr + green.stdout)
        self.assertEqual(self.record()['status'], 'green_verified')
        self.assertEqual(len(self.record()['attempts']), 2)
        self.assertTrue(self.record()['attempts'][0]['commands'])


    def test_build_errors_never_verify_red(self):
        for source in ['mod absent;', 'pub fn broken( {',
                       'pub fn answer() -> u8 { "wrong type" }']:
            with self.subTest(source=source):
                self.run_driver('reset')
                self.source.write_text(source)
                result = self.run_driver('red')
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(self.record()['status'], 'unverified')
                self.assertEqual(self.record()['attempts'][0]['outcome'], 'build_failure')

    def test_zero_matching_and_already_passing_tests_are_not_red(self):
        result = self.run_driver('red', 'absent')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.record()['attempts'][0]['outcome'], 'zero_tests')
        self.run_driver('reset')
        self.write_behavior(True)
        result = self.run_driver('red')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.record()['attempts'][0]['outcome'], 'test_pass')
        self.assertEqual(self.record()['status'], 'unverified')

    def test_green_requires_red_and_rejects_changed_selection_and_test_set(self):
        self.assertEqual(self.run_driver('green').returncode, 2)
        self.assertEqual(self.run_driver('red').returncode, 0)
        self.assertEqual(self.run_driver('green', 'other').returncode, 2)
        self.write_behavior(True)
        self.source.write_text(self.source.read_text().replace('fn behavior()', 'fn behavior_new()'))
        self.assertEqual(self.run_driver('green').returncode, 1)
        self.assertEqual(self.record()['attempts'][-1]['outcome'], 'test_set_mismatch')
        self.assertNotEqual(self.record()['status'], 'green_verified')

    def test_failed_green_retains_failure_and_can_be_retried(self):
        self.assertEqual(self.run_driver('red').returncode, 0)
        result = self.run_driver('green')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.record()['attempts'][-1]['outcome'], 'test_failure')
        self.assertNotEqual(self.record()['status'], 'green_verified')
        self.write_behavior(True)
        self.assertEqual(self.run_driver('green').returncode, 0)

    def test_missing_cargo_is_infrastructure_failure_with_a_log(self):
        env = {**os.environ, 'PATH': str(self.root)}
        # Invoke through Python so the driver's shebang does not require PATH.
        result = subprocess.run([os.sys.executable, str(DRIVER), 'red', 'answer', 'behavior'],
                                cwd=self.root, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1, result.stderr)
        attempt = self.record()['attempts'][0]
        self.assertEqual(attempt['outcome'], 'infrastructure_failure')
        self.assertTrue(Path(attempt['commands'][0]['log']).is_file())

    def test_interrupted_and_malformed_evidence_require_explicit_reset(self):
        import signal
        import time
        self.source.write_text('#[test] fn behavior() { loop { std::thread::sleep('
                               'std::time::Duration::from_millis(10)); } }')
        process = subprocess.Popen([str(DRIVER), 'red', 'answer', 'behavior'], cwd=self.root,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 15
            while not list((self.root / '.tdd-evidence/answer').glob('run-*/2.log')):
                if process.poll() is not None or time.monotonic() > deadline:
                    self.fail('test process did not start')
                time.sleep(0.02)
            process.send_signal(signal.SIGTERM)
            process.communicate(timeout=5)
            self.assertEqual(process.returncode, 1)
            self.assertEqual(self.record()['status'], 'interrupted')
            self.assertEqual(self.run_driver('green').returncode, 2)
            path = self.root / '.tdd-evidence/answer/record.json'
            path.write_text('{damaged')
            self.assertEqual(self.run_driver('green').returncode, 2)
            source = self.source.read_text()
            self.assertEqual(self.run_driver('reset').returncode, 0)
            self.assertFalse(path.exists())
            self.assertEqual(self.source.read_text(), source)
            self.assertTrue(list(path.parent.glob('reset-*.json')))
            self.write_behavior(False)
            self.assertEqual(self.run_driver('red').returncode, 0)
            self.assertEqual(len(list(path.parent.glob('run-*'))), 2)
        finally:
            if process.poll() is None:
                process.terminate()
                process.communicate(timeout=5)

    def test_abrupt_interruption_marker_and_ignored_tests_cannot_verify_green(self):
        self.assertEqual(self.run_driver('red').returncode, 0)
        path = self.root / '.tdd-evidence/answer/record.json'
        record = self.record()
        record['status'] = 'running'
        path.write_text(json.dumps(record))
        self.assertEqual(self.run_driver('green').returncode, 2)
        self.run_driver('reset')
        self.source.write_text('#[test] #[ignore] fn behavior() {}')
        self.assertEqual(self.run_driver('red').returncode, 1)
        self.assertEqual(self.record()['attempts'][0]['outcome'], 'zero_tests')

    def test_structurally_damaged_record_is_rejected_without_overwriting_it(self):
        self.assertEqual(self.run_driver('red').returncode, 0)
        path = self.root / '.tdd-evidence/answer/record.json'
        record = self.record()
        record['attempts'] = ['invalid attempt']
        damaged = json.dumps(record)
        path.write_text(damaged)
        result = self.run_driver('green')
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn('Malformed', result.stderr)
        self.assertEqual(path.read_text(), damaged)

    def test_red_missing_commands_or_diagnostic_cannot_verify_green(self):
        for field in ('commands', 'diagnostic'):
            with self.subTest(field=field):
                self.run_driver('reset')
                self.write_behavior(False)
                self.assertEqual(self.run_driver('red').returncode, 0)
                path = self.root / '.tdd-evidence/answer/record.json'
                record = self.record()
                del record['attempts'][0][field]
                damaged = json.dumps(record)
                path.write_text(damaged)
                self.write_behavior(True)
                result = self.run_driver('green')
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn('Malformed', result.stderr)
                self.assertEqual(path.read_text(), damaged)

    def test_abnormal_test_exit_is_infrastructure_failure(self):
        self.source.write_text('#[test] fn behavior() { std::process::exit(7); }')
        self.assertEqual(self.run_driver('red').returncode, 1)
        self.assertEqual(self.record()['attempts'][0]['outcome'], 'infrastructure_failure')


if __name__ == '__main__':
    unittest.main()
