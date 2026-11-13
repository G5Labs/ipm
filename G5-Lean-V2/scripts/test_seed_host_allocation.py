#!/usr/bin/env python3
"""Exercise host bootstrap against API replies, including a settings race."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
FAKE_CURL = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
url = next(a for a in args if a.startswith('http'))
body = json.loads(args[args.index('-d') + 1]) if '-d' in args else None
with open(os.environ['CALLS'], 'a') as out:
    out.write(json.dumps({'url': url, 'body': body}) + '\\n')
if url.endswith('/healthz'):
    result = {}
elif url.endswith('/auth/test-token'):
    result = {'data': {'jwt': 'fixture-user'}}
elif url.endswith('/orgs'):
    result = {'data': [{'orgId': 'org', 'orgSlug': 'g5labs'}]}
elif url.endswith('/admin-settings'):
    result = {'data': body}
elif url.endswith('/worker-hosts'):
    slots = body['workspaceVmLimit']
    result = {'data': {'hostId': 'host', 'hostJwt': 'fixture-host'}}
    if not os.environ.get('MISSING_ALLOCATION'):
        # Another administrator edits V after the fixture's settings write.
        # Creation returns the actual saved value, not the earlier request.
        result['data']['diskAllocation'] = {
            'hostDiskGib': 120, 'vmDiskMib': 32768,
            'slots': slots, 'diskGib': 120 + 32 * slots,
        }
else:
    raise AssertionError(url)
print(json.dumps(result))
'''


@unittest.skipUnless(shutil.which('jq'), 'host bootstrap requires jq')
class HostAllocationBootstrap(unittest.TestCase):
    def run_bootstrap(self, kind, *, missing=False, disk_mib='10240'):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            (root / 'infra/docker').mkdir(parents=True)
            (root / 'bin').mkdir()
            curl = root / 'bin/curl'
            curl.write_text(FAKE_CURL)
            curl.chmod(0o755)
            script = root / 'scripts/seed.sh'
            if kind == 'local':
                script.write_text((ROOT / 'scripts/seed-dev-worker-token.sh').read_text())
            else:
                source = (ROOT / 'scripts/seed-ci-org.sh').read_text()
                script.write_text('set -euo pipefail\n' + source[source.index('# 10. The candidate microVM lane'):])
            calls = root / 'calls.jsonl'
            env = dict(os.environ, PATH=f'{root / "bin"}:{os.environ["PATH"]}',
                       CALLS=str(calls), G5D_SCRATCH_DISK_MIB=disk_mib,
                       G5D_HOST_MAX_MICROVMS='1', G5_E2E_HOST_WORKSPACE_VM_LIMIT='3',
                       G5_E2E_PROVISION_WORKER_HOST='1', API_URL='http://fixture',
                       ORG_ID='org', REPO_ID='repo', AUTH_TOKEN='fixture-user')
            env.pop('G5_WORKER_ORG', None)
            if missing:
                env['MISSING_ALLOCATION'] = '1'
            result = subprocess.run(['bash', str(script), 'http://fixture'], env=env,
                                    capture_output=True, text=True, timeout=10)
            requests = [json.loads(line) for line in calls.read_text().splitlines()]
            return result, requests

    def test_launchers_use_the_creation_snapshot_after_a_settings_edit(self):
        for kind, slots in [('local', 1), ('ci', 3)]:
            with self.subTest(kind=kind):
                result, requests = self.run_bootstrap(kind)
                self.assertEqual(result.returncode, 0, result.stderr)
                mutations = [r for r in requests if r['url'].endswith(('/admin-settings', '/worker-hosts'))]
                self.assertEqual(mutations[0]['body'], {'vmDiskAllowanceGib': 10})
                self.assertEqual(mutations[1]['body']['workspaceVmLimit'], slots)
                self.assertIn('export G5D_SCRATCH_DISK_MIB=32768\n', result.stdout)
                if kind == 'ci':
                    self.assertIn('export G5D_HOST_XFS_SIZE_GB=216\n', result.stdout)
                else:
                    self.assertIn('export G5D_HOST_MAX_MICROVMS=1\n', result.stdout)

    def test_missing_snapshot_refuses_bootstrap(self):
        for kind in ['local', 'ci']:
            with self.subTest(kind=kind):
                result, _ = self.run_bootstrap(kind, missing=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('export G5_HOST_JWT=', result.stdout)

    def test_non_gib_local_override_cannot_create_a_host(self):
        result, requests = self.run_bootstrap('local', disk_mib='1536')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(r['url'].endswith('/worker-hosts') for r in requests))


if __name__ == '__main__':
    unittest.main()
