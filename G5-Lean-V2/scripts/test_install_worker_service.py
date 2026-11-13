#!/usr/bin/env python3
"""Validate generated worker units with the real systemd parser, without installing them."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / 'infra/self-hosted/scripts/install-worker-service.py'
spec = importlib.util.spec_from_file_location('worker_service', SOURCE)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


@unittest.skipUnless(sys.platform == 'linux' and shutil.which('systemd-analyze'),
                     'requires the Linux systemd unit parser')
class WorkerUnits(unittest.TestCase):
    def verify(self, quoted_scalar=False, executable=True):
        original_run, original_stat = subprocess.run, os.stat
        with tempfile.TemporaryDirectory(prefix='g5-unit-test-') as directory:
            # Both list-valued and scalar paths must survive spaces/specifiers.
            base = Path(directory) / 'worker % paths'
            root, runtime, data = (base / name for name in ('root', 'runtime', 'data'))
            units = base / 'units'
            units.mkdir(parents=True)
            for name in ('usr/local/bin/g5d', 'usr/bin/passt', 'usr/sbin/nft',
                         'opt/g5-host/bin/cloud-hypervisor', 'opt/g5-host/boot/vmlinux',
                         'opt/g5-host/boot/initramfs.cpio', 'etc/apparmor.d/usr.bin.passt',
                         'etc/passwd', 'etc/group'):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('#!/bin/sh\nexit 0\n' if name.endswith('/g5d') else '')
            (root / 'usr/local/bin/g5d').chmod(0o755 if executable else 0o644)
            runtime.mkdir()
            (data / 'rootfs').mkdir(parents=True, mode=0o700)
            disk, env_file = base / 'data.xfs', base / 'host.env'
            disk.touch()
            env_file.touch()
            verified = []

            def run(args, **kwargs):
                if args[0] == 'getent':
                    name = args[-1]
                    return subprocess.CompletedProcess(args, 0, f'{name}:x:991:992::/:/bin/false\n', '')
                if args[0] in ('systemd-analyze', 'systemd-escape'):
                    if args[0] == 'systemd-analyze':
                        verified.append(args)
                        return original_run(args, **{**kwargs, 'capture_output': True, 'text': True})
                    return original_run(args, **kwargs)
                if args[0] in ('apparmor_parser', 'systemctl'):
                    return subprocess.CompletedProcess(args, 0, '', '')
                self.fail(f'unexpected installer command: {args}')

            def stat(path, *args, **kwargs):
                if str(path) == '/dev/kvm':
                    return SimpleNamespace(st_gid=993)
                return original_stat(path, *args, **kwargs)

            def unit_path(path):
                return installer.unit_string(str(path)) if quoted_scalar else str(path).replace('%', '%%')

            with patch.object(installer, 'Path', side_effect=lambda p: units if p == '/etc/systemd/system' else Path(p)), \
                 patch.object(installer.subprocess, 'run', side_effect=run), \
                 patch.object(installer.os, 'chown') as chown, \
                 patch.object(installer.os, 'stat', side_effect=stat):
                if quoted_scalar:
                    with patch.object(installer, 'unit_path', side_effect=unit_path):
                        installer.install(root, runtime, data, disk, env_file)
                else:
                    installer.install(root, runtime, data, disk, env_file)
            self.assertEqual(len(verified), 1)
            self.assertEqual(chown.call_count, 2)
            for directory in (root / 'var/lib/g5d', data / 'rootfs'):
                chown.assert_any_call(directory, 0, 992)
                self.assertEqual(directory.stat().st_mode & 0o777, 0o710)
            service = (units / 'g5d-host.service').read_text()
            self.assertIn('ExecStart=/usr/local/bin/g5d host\n', service)
            self.assertNotIn('CAP_SYS_ADMIN', service)

    def test_real_parser_accepts_paths_and_service_boundary(self):
        self.verify()

    def test_real_parser_rejects_original_quoted_scalar_paths(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify(quoted_scalar=True)

    def test_verification_rejects_nonexecutable_worker(self):
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify(executable=False)


if __name__ == '__main__':
    unittest.main()
