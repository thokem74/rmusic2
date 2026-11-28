#!/usr/bin/env python3
"""Exercise context retrieval through its external command boundary."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

DRIVER = Path(__file__).resolve().with_name('context')


class ContextTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='rmusic-context-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.cache = self.root / 'cache'
        self.source = self.root / 'reference.md'
        self.source.write_text('# Intro\nSecret body Ω\n\n## Details\nUseful detail\n', encoding='utf-8')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.calls = self.root / 'calls.jsonl'
        self.response = self.root / 'issue.json'
        self.issue = {'number': 6, 'title': 'Context helper Ω',
                      'url': 'https://github.com/thokem74/rmusic2/issues/6',
                      'body': '# Intro\nIssue body Ω\n\n## Details\nUseful detail\n'}
        self.response.write_text(json.dumps(self.issue), encoding='utf-8')
        gh = self.bin / 'gh'
        gh.write_text(
            f'#!{sys.executable}\n'
            'import json, os, pathlib, sys, time\n'
            'with open(os.environ["TEST_GH_LOG"], "a") as log:\n'
            '    log.write(json.dumps(sys.argv[1:]) + "\\n")\n'
            'time.sleep(float(os.environ.get("TEST_GH_DELAY", "0")))\n'
            'if os.environ.get("TEST_GH_FAIL"):\n'
            '    print("authentication/network failure: credential-secret", file=sys.stderr)\n'
            '    sys.exit(int(os.environ["TEST_GH_FAIL"]))\n'
            'print(pathlib.Path(os.environ["TEST_GH_RESPONSE"]).read_text())\n'
        )
        gh.chmod(0o700)
        self.env = {**os.environ, 'PATH': str(self.bin) + os.pathsep + os.environ['PATH'],
                    'TEST_GH_LOG': str(self.calls), 'TEST_GH_RESPONSE': str(self.response)}

    def run_context(self, mode='file', source=None, *options, **env):
        if source is None:
            source = str(self.source) if mode == 'file' else '#6'
        return subprocess.run([sys.executable, str(DRIVER), mode, source,
                               '--cache-dir', str(self.cache), *options],
                              cwd=self.root, env={**self.env, **env}, capture_output=True,
                              text=True, timeout=10)

    def full_source(self, result):
        path = next(line.removeprefix('Full source: ') for line in result.stdout.splitlines()
                    if line.startswith('Full source: '))
        return Path(path)

    def test_local_default_is_metadata_and_cached_snapshot(self):
        result = self.run_context()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn('Secret body', result.stdout)
        self.assertIn(str(self.source), result.stdout)
        self.assertIn('Retrieved:', result.stdout)
        self.assertIn('UTF-8 bytes', result.stdout)
        cached = self.full_source(result)
        self.assertEqual(cached.read_bytes(), self.source.read_bytes())
        self.source.write_text('changed')
        hit = self.run_context()
        self.assertEqual(hit.returncode, 0, hit.stderr)
        self.assertIn('cache hit', hit.stdout)
        self.assertEqual(self.full_source(hit).read_text(), '# Intro\nSecret body Ω\n\n## Details\nUseful detail\n')

    def test_issue_fetch_once_until_explicit_refresh(self):
        result = self.run_context('issue')
        self.assertEqual(result.returncode, 0, result.stderr)
        for value in ['#6', self.issue['title'], self.issue['url'], 'Retrieved:']:
            self.assertIn(value, result.stdout)
        self.assertNotIn('Issue body', result.stdout)
        original = self.full_source(result)
        self.assertEqual(original.read_text(), self.issue['body'])
        self.response.write_text(json.dumps({**self.issue, 'body': 'new snapshot'}))
        hit = self.run_context('issue', 'thokem74/rmusic2#6')
        self.assertIn('cache hit', hit.stdout)
        self.assertEqual(self.full_source(hit), original)
        self.assertEqual(len(self.calls.read_text().splitlines()), 1)
        refreshed = self.run_context('issue', self.issue['url'], '--refresh')
        self.assertEqual(refreshed.returncode, 0, refreshed.stderr)
        self.assertIn('refreshed', refreshed.stdout)
        self.assertEqual(self.full_source(refreshed).read_text(), 'new snapshot')
        self.assertEqual(original.read_text(), self.issue['body'])
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(calls, [['issue', 'view', '6', '--repo', 'thokem74/rmusic2',
                                '--json', 'number,title,url,body']] * 2)

    def test_selected_section_includes_children_and_stops_at_peer(self):
        self.source.write_text('# Parent\nBefore\n## Selected\nΩ body\n'
                               '### Child\nchild content\n```md\n## Fake\n```\n'
                               '## Peer\nnot selected\n# End\noutside\n')
        result = self.run_context('file', None, '--section', 'Selected')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('## Selected\nΩ body\n### Child\nchild content\n```md\n## Fake\n```', result.stdout)
        for value in ['Before', '## Peer', 'not selected', 'outside']:
            self.assertNotIn(value, result.stdout)

    def test_missing_ambiguous_and_empty_headings_have_explicit_results(self):
        self.source.write_text('# Repeat\nfirst\n# Repeat\nsecond\n#\nempty heading\n# End\n')
        for selector, error in [('Absent', 'missing'), ('Repeat', 'ambiguous')]:
            with self.subTest(selector=selector):
                result = self.run_context('file', None, '--section', selector)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error, result.stderr.lower())
                self.assertNotIn('first', result.stdout)
        empty = self.run_context('file', None, '--section', '')
        self.assertEqual(empty.returncode, 0, empty.stderr)
        self.assertIn('#\nempty heading\n', empty.stdout)

    def test_excerpt_has_hard_line_and_byte_bounds_and_full_source_is_intact(self):
        for body in ['row Ω\n' * 300, 'Ω' * 10000]:
            with self.subTest(single_line='\n' not in body):
                self.source.write_text(body)
                result = self.run_context('file', None, '--refresh', '--excerpt',
                                          '--max-lines', '12', '--max-bytes', '1200')
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertLessEqual(len(result.stdout.splitlines()), 12)
                self.assertLessEqual(len(result.stdout.encode('utf-8')), 1200)
                self.assertIn('TRUNCATED', result.stdout)
                self.assertIn('Full source:', result.stdout)
                self.assertEqual(self.full_source(result).read_text(), body)

    def test_huge_title_cannot_bypass_default_output_limits(self):
        self.response.write_text(json.dumps({**self.issue, 'title': '巨大' * 10000}))
        result = self.run_context('issue')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLessEqual(len(result.stdout.encode('utf-8')), 8000)
        self.assertLessEqual(len(result.stdout.splitlines()), 80)
        self.assertIn('TRUNCATED', result.stdout)
        self.assertEqual(self.full_source(result).read_text(), self.issue['body'])
        self.assertIn('Retrieved:', result.stdout)
        self.assertIn(self.issue['url'], result.stdout)
        self.assertIn('Status:', result.stdout)

    def test_refresh_failure_is_not_a_cache_hit_and_preserves_previous_snapshot(self):
        first = self.run_context('issue')
        original = self.full_source(first)
        record = next(self.cache.rglob('record.json'))
        saved = record.read_bytes()
        for status in ('1', '4'):
            failed = self.run_context('issue', None, '--refresh', TEST_GH_FAIL=status)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(failed.stdout, '')
            self.assertIn('No fresh source', failed.stderr)
            self.assertIn('previous snapshot unchanged', failed.stderr)
            self.assertNotIn('credential-secret', failed.stderr)
            self.assertEqual(record.read_bytes(), saved)
        hit = self.run_context('issue', TEST_GH_FAIL='4')
        self.assertEqual(hit.returncode, 0, hit.stderr)
        self.assertIn('cache hit', hit.stdout)
        self.assertEqual(self.full_source(hit), original)
        self.assertEqual(len(self.calls.read_text().splitlines()), 3)
        for path in self.cache.rglob('*'):
            if path.is_file():
                self.assertNotIn(b'credential-secret', path.read_bytes())

    def test_malformed_cache_fails_explicitly_and_can_be_refreshed(self):
        first = self.run_context('issue')
        self.assertEqual(first.returncode, 0, first.stderr)
        path = next(self.cache.rglob('record.json'))
        record = json.loads(path.read_text())
        del record['retrieved']
        path.write_text(json.dumps(record))
        result = self.run_context('issue')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('Traceback', result.stderr)
        self.assertIn('Invalid cache', result.stderr)
        self.assertIn('--refresh', result.stderr)
        self.assertEqual(len(self.calls.read_text().splitlines()), 1)
        refreshed = self.run_context('issue', None, '--refresh')
        self.assertEqual(refreshed.returncode, 0, refreshed.stderr)
        self.assertEqual(len(self.calls.read_text().splitlines()), 2)

    def test_uncached_authentication_failure_and_missing_tool_are_errors(self):
        for env in [{'TEST_GH_FAIL': '4'}, {'PATH': str(self.root / 'missing-bin')}]:
            with self.subTest(env=env):
                result = self.run_context('issue', **env)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, '')
                self.assertNotIn('credential-secret', result.stderr)
                self.assertFalse(list(self.cache.rglob('*.md')))
                self.assertFalse(list(self.cache.rglob('record.json')))

    def test_wrong_repository_or_response_identity_is_rejected(self):
        bad = self.run_context('issue', 'other/repo#6')
        self.assertNotEqual(bad.returncode, 0)
        self.assertFalse(self.calls.exists())
        for response in [{**self.issue, 'number': 7}, {**self.issue, 'url': 'https://example.test/6'},
                         {**self.issue, 'body': None}, 'malformed response']:
            with self.subTest(response=response):
                self.response.write_text(json.dumps(response))
                result = self.run_context('issue')
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Invalid GitHub response', result.stderr)
                self.assertFalse(list(self.cache.rglob('record.json')))

    def test_read_only_sources_unicode_newlines_and_environment_cache(self):
        body = '# Grüße\r\nΚαλημέρα 世界\n\nLast line without newline'
        self.source.write_bytes(body.encode('utf-8'))
        self.issue['body'] = body
        self.response.write_text(json.dumps({**self.issue, 'token': 'credential-secret'}))
        original = self.response.read_bytes()
        result = self.run_context('issue', None, '--excerpt')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('Καλημέρα 世界\n\nLast line without newline', result.stdout)
        self.assertNotIn(r'\nΚαλημέρα', result.stdout)
        self.assertEqual(self.full_source(result).read_bytes(), body.encode('utf-8'))
        self.assertEqual(self.response.read_bytes(), original)
        local = subprocess.run([sys.executable, str(DRIVER), 'file', str(self.source), '--excerpt'],
                               env={**self.env, 'RMUSIC_CONTEXT_CACHE': str(self.cache)},
                               cwd=self.root, text=True, capture_output=True, timeout=10)
        self.assertEqual(local.returncode, 0, local.stderr)
        self.assertEqual(self.full_source(local).read_bytes(), body.encode('utf-8'))
        self.assertEqual(self.source.read_bytes(), body.encode('utf-8'))
        for path in self.cache.rglob('*'):
            if path.is_file():
                self.assertNotIn(b'credential-secret', path.read_bytes())

    def test_corrupt_body_never_silently_refetches(self):
        initial = self.run_context('issue')
        self.full_source(initial).write_text('corrupt')
        result = self.run_context('issue')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Invalid cache', result.stderr)
        self.assertEqual(len(self.calls.read_text().splitlines()), 1)
        self.assertEqual(self.run_context('issue', None, '--refresh').returncode, 0)

    def test_local_refresh_and_deleted_source_use_explicit_snapshot_semantics(self):
        first = self.run_context()
        original = self.full_source(first).read_bytes()
        self.source.unlink()
        hit = self.run_context()
        self.assertEqual(hit.returncode, 0, hit.stderr)
        self.assertIn('cache hit', hit.stdout)
        self.assertEqual(self.full_source(hit).read_bytes(), original)
        failed = self.run_context('file', None, '--refresh')
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn('previous snapshot unchanged', failed.stderr)
        self.source.write_text('replacement')
        refreshed = self.run_context('file', None, '--refresh')
        self.assertEqual(refreshed.returncode, 0, refreshed.stderr)
        self.assertEqual(self.full_source(refreshed).read_text(), 'replacement')

    def test_heading_contract_closing_hashes_fences_and_bodyless_section(self):
        self.source.write_text('~~~markdown\n## Fake\n~~~\n## Selected ###\n'
                               'content\n## Bodyless\n## Peer\nlast\n')
        selected = self.run_context('file', None, '--section', 'Selected')
        self.assertEqual(selected.returncode, 0, selected.stderr)
        self.assertIn('## Selected ###\ncontent\n', selected.stdout)
        self.assertNotIn('## Fake', selected.stdout)
        self.assertNotIn('## Bodyless', selected.stdout)
        empty = self.run_context('file', None, '--section', 'Bodyless')
        self.assertEqual(empty.returncode, 0, empty.stderr)
        self.assertIn('## Bodyless\n', empty.stdout)
        self.assertNotIn('## Peer', empty.stdout)

    def test_heading_with_only_closing_hashes_is_empty(self):
        self.source.write_text('# ###\nempty title\n# End\n')
        result = self.run_context('file', None, '--section', '')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('# ###\nempty title\n', result.stdout)

    def test_concurrent_reads_share_one_fetch(self):
        command = [sys.executable, str(DRIVER), 'issue', '#6', '--cache-dir', str(self.cache)]
        processes = [subprocess.Popen(command, env={**self.env, 'TEST_GH_DELAY': '0.2'},
                                      cwd=self.root, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, text=True) for _ in range(2)]
        try:
            results = [process.communicate(timeout=10) for process in processes]
            for process, (_, stderr) in zip(processes, results):
                self.assertEqual(process.returncode, 0, stderr)
            self.assertEqual(len(self.calls.read_text().splitlines()), 1)
            self.assertEqual(sum('cache hit' in stdout for stdout, _ in results), 1)
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=5)

    def test_invalid_limits_do_not_fetch_and_section_output_is_bounded(self):
        invalid = self.run_context('issue', None, '--max-bytes', '8001')
        self.assertNotEqual(invalid.returncode, 0)
        self.assertFalse(self.calls.exists())
        self.source.write_text('# Selected\n' + 'Ω' * 10000 + '\n# Peer\nother\n')
        result = self.run_context('file', None, '--section', 'Selected')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLessEqual(len(result.stdout.encode('utf-8')), 8000)
        self.assertLessEqual(len(result.stdout.splitlines()), 80)
        self.assertIn('TRUNCATED', result.stdout)
        self.assertNotIn('# Peer', result.stdout)


if __name__ == '__main__':
    unittest.main()
