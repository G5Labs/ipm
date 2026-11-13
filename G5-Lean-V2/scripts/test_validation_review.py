"""Review regressions at worker discovery, Cargo selection and health boundaries."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import local_validation as validation
import workspace_metadata
import compose_health
import validation_cleanup as cleanup

ROOT = Path(__file__).resolve().parents[1]


class ReviewTests(unittest.TestCase):

    def test_health_retries_vanished_container_and_reports_scoped_logs(self):
        config = {'services': {'g5-api': {}}}
        healthy = {'Id': 'new', 'Config': {'Labels': {'com.docker.compose.service': 'g5-api'}}, 'State': {'Running': True, 'Status': 'running'}}
        def response(*args):
            if 'config' in args:
                return json.dumps(config)
            if args[1] == 'ps':
                return 'old' if not seen else 'new'
            if args[1] == 'inspect':
                if not seen:
                    seen.append(True)
                    raise subprocess.CalledProcessError(1, args, stderr='No such container')
                return json.dumps([healthy])
            self.fail(args)
        seen = []
        with patch.object(sys, 'argv', ['health', 'compose.yml', '5', 'selected']), patch.object(compose_health, 'capture', side_effect=response), patch.object(compose_health.time, 'sleep'), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(compose_health.main(), 0)
        healthy['State']['Running'] = False
        with patch.object(sys, 'argv', ['health', 'compose.yml', '0', 'selected']), patch.object(compose_health, 'capture', side_effect=response), patch.object(compose_health.subprocess, 'run') as run, contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(compose_health.main(), 1)
        calls = [call.args[0] for call in run.call_args_list]
        self.assertIn(['docker', 'logs', '--tail', '50', 'new'], calls)

    def test_inventory_uses_actual_cargo_targets(self):
        metadata = {'packages': [
            {'name': 'g5d', 'targets': [{'name': 'custom', 'kind': ['test']}]},
            {'name': 'g5d-dispatch', 'targets': [{'name': 'g5d_dispatch', 'kind': ['lib']}]},
            {'name': 'outside', 'targets': [{'name': 'custom', 'kind': ['test']}]},
        ]}
        source = (ROOT / 'Justfile').read_text()
        with patch.object(validation, 'cargo_metadata', return_value=metadata):
            self.assertEqual(validation.integration_targets('package(=g5d-dispatch)', source), [])
            self.assertEqual(validation.integration_targets('package(=outside)', source), [])
            self.assertEqual(validation.integration_targets('package(=g5d) and binary(=absent)', source), [])
            self.assertEqual(validation.integration_targets('package(=g5d) and binary(=custom)', source), ['-p', 'g5d', '--test', 'custom'])

    def test_recommendations_use_workspace_target_kinds(self):
        metadata = workspace_metadata.cargo_metadata()
        changes = '\n'.join(str(Path(p['manifest_path']).relative_to(ROOT).parent / 'src/review.rs') for p in metadata['packages'] if '/backend/' in p['manifest_path'])
        output = io.StringIO()
        with patch.object(validation, 'capture', side_effect=['base', changes, '']), patch.object(validation, 'cargo_metadata', return_value=metadata), contextlib.redirect_stdout(output):
            validation.recommendations('main')
        commands = output.getvalue()
        self.assertIn("ci-docker-5 'package(=g5d)'", commands)
        self.assertNotIn("ci-docker-5 'package(=g5d-dispatch)'", commands)
        self.assertNotIn("ci-docker-5 'package(=g5-auth)'", commands)
        for package in metadata['packages']:
            if '/backend/' not in package['manifest_path']:
                continue
            kinds = {kind for target in package['targets'] for kind in target['kind']}
            if 'lib' not in kinds:
                self.assertNotIn(f"-p {package['name']} --lib", commands)


    def test_down_removes_generated_tags_without_shared_images(self):
        project = 'g5-local-test'
        config = {'services': {'g5-api': {'build': {}, 'image': project + '/api:test'},
                               'client': {'build': {'context': '.'}},
                               'postgres': {'image': 'postgres:17'}}}
        config['services']['g5-api']['build'] = {'context': '.'}
        tags = [project + '/api:test', project + '-client:latest', project + '-playwright:ci', 'shared-browser:ci', 'postgres:17']
        with patch.object(cleanup, 'capture', side_effect=[json.dumps(config), '\n'.join(tags)]) as capture, patch.object(cleanup.subprocess, 'call', return_value=0) as down, patch.object(cleanup, 'remove_tags', return_value=0) as remove:
            self.assertEqual(cleanup.down(ROOT, 'compose.yml', project), 0)
            self.assertEqual(set(remove.call_args.args[0]), set(tags[:3]))
            self.assertIn('--remove-orphans', down.call_args.args[0])
            self.assertIn('--profile', capture.call_args_list[0].args)
            self.assertIn('*', capture.call_args_list[0].args)
        with patch.object(cleanup, 'capture', side_effect=[json.dumps(config), '\n'.join(tags)]), patch.object(cleanup.subprocess, 'call', return_value=1), patch.object(cleanup, 'remove_tags') as remove:
            self.assertEqual(cleanup.down(ROOT, 'compose.yml', project), 1)
            remove.assert_not_called()

    def test_gc_previews_deleted_worktrees_and_preserves_live_and_unowned_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gone = root / 'deleted'
            with patch.dict(os.environ, {}, clear=True):
                project = validation.project_name(gone)
            tags = [project + '-client:latest', 'shared:latest']
            image = {'Config': {'Labels': {cleanup.WORKTREE_LABEL: str(gone), cleanup.PROJECT_LABEL: project}}, 'RepoTags': tags}
            for prune, exists, containers, expected in [(False, False, '', []), (True, False, '', tags[:1]), (True, True, '', []), (True, False, 'retained-container', [])]:
                if exists:
                    gone.mkdir(exist_ok=True)
                elif gone.exists():
                    gone.rmdir()
                with patch.object(cleanup, 'capture', side_effect=['image-id', json.dumps([image]), containers]), patch.object(cleanup, 'remove_tags', return_value=0) as remove, contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(cleanup.collect_orphan_images(prune), 0)
                actual = list(remove.call_args.args[0]) if remove.called else []
                self.assertEqual(actual, expected)

    def test_lock_contention_exits_retryable_before_preflight_or_docker(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(validation.tempfile, 'gettempdir', return_value=directory):
            with validation.validation_lock('owner'):
                for command in [[sys.executable, str(ROOT / 'scripts/local_validation.py'), 'run', '5'], ['just', 'ci-docker-5']]:
                    result = subprocess.run(command, cwd=ROOT, env=dict(os.environ, TMPDIR=directory), capture_output=True, text=True)
                    self.assertEqual(result.returncode, 75, result.stderr)
                    self.assertIn('retry when it finishes', result.stderr)
                    self.assertNotIn('Preflight:', result.stdout)

if __name__ == '__main__':
    unittest.main()
