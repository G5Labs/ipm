#!/usr/bin/env python3
"""Real Git checks for the local committed seed boundary."""
import importlib.util
import contextlib
import io
import concurrent.futures
import json
import os
import subprocess
import uuid
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('lean_seed', Path(__file__).with_name('seed-lean-workspace.py'))
seed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seed)


class SeedCliTests(unittest.TestCase):
    def test_command_failures_preserve_diagnostics_without_echoing_arguments(self):
        for diagnostic, expected in (
            (None, 'Command failed with exit status 128.'),
            ('', 'Command failed with exit status 128.'),
            (' \n', 'Command failed with exit status 128.'),
            ('fatal: missing object\n', 'fatal: missing object'),
        ):
            with self.subTest(stderr=diagnostic):
                error = subprocess.CalledProcessError(
                    128, ['command', '--token', 'test-secret'], stderr=diagnostic,
                )
                output = io.StringIO()
                with patch.object(seed, 'main', side_effect=error), contextlib.redirect_stderr(output):
                    with self.assertRaises(SystemExit) as raised:
                        seed.cli()
                self.assertEqual(raised.exception.code, 1)
                self.assertEqual(output.getvalue(), 'Seed failed: ' + expected + '\n')

    def test_validation_failure_preserves_message(self):
        output = io.StringIO()
        with patch.object(seed, 'main', side_effect=ValueError('Invalid source')), contextlib.redirect_stderr(output):
            with self.assertRaises(SystemExit) as raised:
                seed.cli()
        self.assertEqual(raised.exception.code, 1)
        self.assertEqual(output.getvalue(), 'Seed failed: Invalid source\n')

    def test_success_does_not_report_an_error(self):
        output = io.StringIO()
        with patch.object(seed, 'main') as main, contextlib.redirect_stderr(output):
            seed.cli()
        main.assert_called_once_with()
        self.assertEqual(output.getvalue(), '')


class SeedGitTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.source = self.base / 'source'
        seed.run('git', 'init', '-q', str(self.source))
        seed.git(self.source, 'config', 'user.email', 'test@example.test')
        seed.git(self.source, 'config', 'user.name', 'Test')
        (self.source / '.g5/ontology').mkdir(parents=True)
        (self.source / '.g5/ontology/root.g5.md').write_text('# Root\n')
        self.commit('root')

    def commit(self, message):
        seed.git(self.source, 'add', '.')
        seed.git(self.source, 'commit', '-qm', message)
        return seed.git(self.source, 'rev-parse', 'HEAD')

    def test_history_pin_dirty_exclusion_and_independence(self):
        ancestor = seed.git(self.source, 'rev-parse', 'HEAD')
        (self.source / 'tracked.txt').write_text('committed\n')
        pinned = self.commit('unpushed commit')
        self.assertEqual(seed.pin_source(self.source), pinned)
        (self.source / 'tracked.txt').write_text('later committed\n')
        self.commit('branch moves after pin')
        (self.source / 'tracked.txt').write_text('dirty\n')
        (self.source / '.env').write_text('not-for-import\n')
        before = seed.git(self.source, 'status', '--porcelain')
        code, docs = seed.stage(self.source, pinned, self.base / 'stage', 'G5-Lean')
        self.assertEqual(seed.git(code, 'rev-parse', 'HEAD'), pinned)
        self.assertEqual(seed.git(code, 'rev-parse', 'HEAD^'), ancestor)
        self.assertEqual(seed.git(code, 'show', 'HEAD:tracked.txt'), 'committed')
        self.assertFalse((code / '.env').exists())
        self.assertFalse((code / 'objects/info/alternates').exists())
        self.assertEqual(seed.git(code, 'rev-parse', '--is-bare-repository'), 'true')
        self.assertEqual(seed.git(docs, 'rev-parse', '--is-bare-repository'), 'true')
        self.assertEqual(seed.git(code, 'remote'), '')
        self.assertEqual(seed.git(docs, 'ls-tree', '-r', 'HEAD'), '')
        self.assertEqual(seed.git(self.source, 'status', '--porcelain'), before)
        self.source.rename(self.base / 'source-hidden')
        seed.git(code, 'fsck', '--full')
        self.assertEqual(seed.git(code, 'show', 'HEAD:.g5/ontology/root.g5.md'), '# Root')

    def test_push_to_parent_after_server_side_merge(self):
        pinned = seed.pin_source(self.source)
        code, _ = seed.stage(self.source, pinned, self.base / 'stage', 'parent')
        child = self.base / 'child'
        seed.run('git', 'clone', '-q', str(code), str(child))
        seed.git(child, 'checkout', '-qb', 'child')
        (child / 'child.txt').write_text('child edit\n')
        seed.git(child, 'add', '.')
        seed.git(child, '-c', 'user.name=Test', '-c', 'user.email=test@example.test',
                 'commit', '-qm', 'child edit')
        seed.git(child, 'push', '-q', 'origin', 'child')
        merged = seed.git(child, 'rev-parse', 'HEAD')
        seed.git(code, 'update-ref', 'refs/heads/parent', merged, pinned)
        parent = self.base / 'parent'
        seed.run('git', 'clone', '-q', str(code), str(parent))
        (parent / 'after.txt').write_text('after merge\n')
        seed.git(parent, 'add', '.')
        seed.git(parent, '-c', 'user.name=Test', '-c', 'user.email=test@example.test',
                 'commit', '-qm', 'post-merge edit')
        seed.git(parent, 'push', '-q', 'origin', 'parent')
        self.assertEqual(seed.git(code, 'show', 'parent:child.txt'), 'child edit')
        self.assertEqual(seed.git(code, 'show', 'parent:after.txt'), 'after merge')

    def test_shallow_refused(self):
        clone = self.base / 'shallow'
        seed.run('git', 'clone', '-q', '--depth=1', self.source.as_uri(), str(clone))
        with self.assertRaisesRegex(ValueError, 'Shallow'):
            seed.pin_source(clone)

    def test_submodule_refused(self):
        sha = seed.git(self.source, 'rev-parse', 'HEAD')
        seed.git(self.source, 'update-index', '--add', '--cacheinfo', f'160000,{sha},nested')
        seed.git(self.source, 'commit', '-qm', 'gitlink')
        with self.assertRaisesRegex(ValueError, 'Submodules'):
            seed.pin_source(self.source)

    def test_missing_ontology_refused(self):
        seed.git(self.source, 'rm', '.g5/ontology/root.g5.md')
        seed.git(self.source, 'commit', '-qm', 'no ontology')
        with self.assertRaises(seed.subprocess.CalledProcessError):
            seed.pin_source(self.source)

    def test_nonlocal_api_refused(self):
        for url in ('https://dev.g5labs.ai', 'http://localhost.evil.test', 'http://localhost:7500/path', 'http://secret@localhost:7500'):
            with self.subTest(url=url), self.assertRaises(ValueError):
                seed.Stack(url)


@unittest.skipUnless(os.environ.get('G5_TEST_LEAN_SEED_LIVE') == '1', 'requires running local dev stack')
class SeedLiveTests(SeedGitTests):
    # Reuse the small real Git source, not mocks of the product API/DB/Git store.
    def setUp(self):
        super().setUp()
        self.name = 'Lean-seed-test-' + uuid.uuid4().hex[:10]
        self.stack = seed.Stack('http://localhost:' + os.environ.get('G5_PORT_API', '7500'))
        self.stack.token = self.stack.request('/api/v1/auth/test-token', {'email': 'demo@g5labs.ai'})['jwt']
        self.org = next(o['orgId'] for o in self.stack.request('/api/v1/orgs') if o['orgSlug'] == 'g5labs')
        self.addCleanup(self.cleanup_workspaces)

    def invoke(self, *args):
        return subprocess.run(['python3', str(Path(seed.__file__)), '--source', str(self.source), '--name', self.name, *args], text=True, capture_output=True)

    def rows(self):
        return json.loads(self.stack.sql(f"SELECT coalesce(json_agg(json_build_object('id',id,'parent',cloned_from_id,'deleted',deleted_at IS NOT NULL)), '[]'::json) FROM workspaces WHERE org_id={seed.literal(self.org)} AND name={seed.literal(self.name)};"))

    def cleanup_workspaces(self):
        rows = self.rows()
        for row in rows:
            if not row['deleted']:
                self.stack.request('/api/v1/workspaces/' + row['id'] + '/delete', {})

    def test_create_refuse_replace(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        original = self.rows()[0]['id']
        self.assertIsNone(self.rows()[0]['parent'])
        self.assertEqual(self.stack.sql(f"SELECT count(*) FROM workspaces WHERE name='seed-source/{original}';"), '0')
        duplicate = self.invoke()
        self.assertNotEqual(duplicate.returncode, 0)
        self.assertIn('already exists', duplicate.stderr)
        self.assertEqual(len(self.rows()), 1)
        wrong = self.invoke('--replace', str(uuid.uuid4()))
        self.assertNotEqual(wrong.returncode, 0)
        self.assertEqual(len(self.rows()), 1)
        replacement = self.invoke('--replace', original)
        self.assertEqual(replacement.returncode, 0, replacement.stdout + replacement.stderr)
        rows = self.rows()
        self.assertEqual(len(rows), 2)
        self.assertTrue(next(r for r in rows if r['id'] == original)['deleted'])
        self.assertEqual(sum(not r['deleted'] for r in rows), 1)

    def test_standalone_can_fork(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        parent = self.rows()[0]['id']
        self.assertIsNone(self.rows()[0]['parent'])
        fork = self.stack.request('/api/v1/workspaces/' + parent + '/clone', {
            'name': self.name + '-fork', 'source': 'durable', 'idempotencyKey': str(uuid.uuid4())
        })['workspace']
        # Runs before the parent cleanup, preserving ordinary deletion guards.
        self.addCleanup(self.stack.request, '/api/v1/workspaces/' + fork['id'] + '/delete', {})
        detail = self.stack.request('/api/v1/workspaces/' + fork['id'])
        self.assertEqual(detail['clonedFromId'], parent)
        self.assertFalse(detail['isBase'])
        parent_detail = self.stack.request('/api/v1/workspaces/' + parent)
        self.assertEqual({b['repoId'] for b in detail['branches']}, {b['repoId'] for b in parent_detail['branches']})
        ontology = self.stack.request('/api/v1/workspaces/' + fork['id'] + '/ontology')
        self.assertTrue(ontology['nodes'])
        self.assertFalse(any(r['incomplete'] for r in ontology['discovery']))

    def test_concurrent_name_claim(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(lambda _: self.invoke(), range(2)))
        self.assertEqual(sorted(r.returncode for r in results), [0, 1], '\n'.join(r.stdout + r.stderr for r in results))
        self.assertEqual(len(self.rows()), 1)


if __name__ == '__main__':
    unittest.main()
