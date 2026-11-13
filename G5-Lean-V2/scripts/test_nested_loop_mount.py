#!/usr/bin/env python3
"""Exercise loop ownership without requiring root, Linux, or a mounted image.

Run with: python3 -m unittest scripts.test_nested_loop_mount
"""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPTS = Path(__file__).resolve().parent
LOOP = "/dev/loop23"
spec = importlib.util.spec_from_file_location("guest_nested_kvm", SCRIPTS / "test-guest-nested-kvm.py")
guest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guest)


class GuestReflinkRootTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = str(Path(self.directory.name) / "root")
        self.image = self.root + ".xfs"
        self.commands = []
        self.mounted = False
        self.failures = {}
        for patcher in (
            mock.patch.object(guest, "INNER_ROOT", self.root),
            mock.patch.object(guest, "INNER_IMAGE_GIB", 0),
            mock.patch.object(guest.os.path, "ismount", side_effect=lambda path: self.mounted),
            mock.patch.object(guest.subprocess, "run", side_effect=self.run_command),
        ):
            patcher.start()
            self.addCleanup(patcher.stop)

    def run_command(self, command, **kwargs):
        self.commands.append(command)
        operation = "detach" if command[:2] == ["losetup", "--detach"] else command[0]
        failure = self.failures.get(operation)
        if isinstance(failure, Exception):
            raise failure
        if failure:
            return subprocess.CompletedProcess(command, 32, "", failure)
        if command[0] == "mount":
            self.mounted = True
        elif command[0] == "cp":
            shutil.copyfile(command[-2], command[-1])
        return subprocess.CompletedProcess(command, 0, LOOP + "\n" if operation == "losetup" else "", "")

    def loop_commands(self):
        return [command for command in self.commands if command[0] in ("losetup", "mount")]

    def test_fresh_root_mounts_only_the_allocated_loop_and_requests_autoclear(self):
        self.assertEqual(guest.reflink_root(), self.root)
        self.assertTrue(Path(self.image).is_file())
        self.assertIn(["mkfs.xfs", "-q", "-m", "reflink=1", self.image], self.commands)
        self.assertEqual(self.loop_commands(), [
            ["losetup", "--find", "--show", self.image],
            ["mount", "-t", "xfs", "-o", "noatime", LOOP, self.root],
            ["losetup", "--detach", LOOP],
        ])
        self.assertFalse((Path(self.root) / "probe").exists())
        self.assertFalse((Path(self.root) / "probe.clone").exists())

    def test_existing_mount_is_reused_and_reflinks_are_rechecked(self):
        self.mounted = True
        for _ in range(2):
            self.assertEqual(guest.reflink_root(), self.root)
        self.assertEqual(self.loop_commands(), [])
        self.assertEqual(sum(command[0] == "cp" for command in self.commands), 2)
        self.assertFalse(Path(self.image).exists())

    def test_mount_failure_detaches_only_owned_loop_and_preserves_primary_error(self):
        for detach_failure in (None, "detach-secondary-failure", OSError("detach-secondary-failure")):
            with self.subTest(detach_failure=detach_failure):
                self.commands.clear()
                self.failures = {"mount": "mount-primary-failure", "detach": detach_failure}
                with self.assertRaisesRegex(AssertionError, "mount-primary-failure"):
                    guest.reflink_root()
                self.assertEqual(self.loop_commands(), [
                    ["losetup", "--find", "--show", self.image],
                    ["mount", "-t", "xfs", "-o", "noatime", LOOP, self.root],
                    ["losetup", "--detach", LOOP],
                ])
                self.assertFalse(any(command[0] == "cp" for command in self.commands))

    def test_mount_launch_failure_also_releases_the_owned_loop(self):
        self.failures = {"mount": OSError("mount-primary-failure"),
                         "detach": OSError("detach-secondary-failure")}
        with self.assertRaisesRegex(OSError, "mount-primary-failure"):
            guest.reflink_root()
        self.assertEqual(self.loop_commands()[-1], ["losetup", "--detach", LOOP])

    def test_detach_failure_after_successful_mount_stops_before_reflink_probe(self):
        self.failures = {"detach": "detach-primary-failure"}
        with self.assertRaisesRegex(AssertionError, "detach-primary-failure"):
            guest.reflink_root()
        self.assertTrue(self.mounted)
        self.assertEqual(self.loop_commands(), [
            ["losetup", "--find", "--show", self.image],
            ["mount", "-t", "xfs", "-o", "noatime", LOOP, self.root],
            ["losetup", "--detach", LOOP],
        ])
        self.assertFalse(any(command[0] == "cp" for command in self.commands))
        self.assertEqual(list(Path(self.root).iterdir()), [])

    def test_failed_allocation_never_detaches_an_unowned_loop(self):
        self.failures = {"losetup": "allocation-primary-failure"}
        with self.assertRaisesRegex(AssertionError, "allocation-primary-failure"):
            guest.reflink_root()
        self.assertEqual(self.loop_commands(), [["losetup", "--find", "--show", self.image]])


FAKE_COMMAND = r'''import json
import os
from pathlib import Path
import shutil
import sys

name = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["TEST_COMMAND_LOG"], "a") as log:
    log.write(json.dumps([name, *args]) + "\n")
mounted = Path(os.environ["TEST_MOUNTED"])
if name == "uname":
    print("Linux")
elif name == "id":
    print("0")
elif name == "mountpoint":
    sys.exit(0 if mounted.exists() else 1)
elif name == "findmnt":
    print(os.environ.get("TEST_FSTYPE", "xfs") if "FSTYPE" in args else "/dev/loop23")
elif name == "losetup":
    if "--detach" in args:
        if os.environ.get("TEST_DETACH_FAIL"):
            print("detach-secondary-failure", file=sys.stderr)
            sys.exit(41)
    elif os.environ.get("TEST_ALLOCATE_FAIL"):
        print("allocation-primary-failure", file=sys.stderr)
        sys.exit(42)
    else:
        print("/dev/loop23")
elif name == "mount":
    if os.environ.get("TEST_MOUNT_FAIL"):
        print("mount-primary-failure", file=sys.stderr)
        sys.exit(32)
    mounted.touch()
elif name == "cp":
    shutil.copyfile(args[-2], args[-1])
elif name == "truncate":
    Path(args[-1]).touch()
elif name == "df":
    print("Filesystem 1024-blocks Used Available Capacity Mounted on")
    print("test 67108864 0 67108864 0% /tmp")
elif name == "mkfs.xfs":
    if os.environ.get("TEST_MKFS_FAIL"):
        print("mkfs-primary-failure", file=sys.stderr)
        sys.exit(43)
elif name not in ("grep", "chown", "chmod", "modprobe"):
    raise AssertionError("unexpected fake command: " + name)
'''


class DevHostReflinkRootTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.directory_path = Path(self.directory.name)
        self.root = self.directory_path / "host root"
        self.image = self.directory_path / "host image.xfs"
        self.log = self.directory_path / "commands.jsonl"
        self.mounted = self.directory_path / "mounted"
        self.bin = self.directory_path / "bin"
        self.bin.mkdir()
        dispatcher = self.bin / "fake-command"
        dispatcher.write_text(f"#!{sys.executable}\n" + FAKE_COMMAND)
        dispatcher.chmod(0o755)
        for name in ("uname", "id", "mountpoint", "findmnt", "losetup", "mount", "cp",
                     "truncate", "df", "mkfs.xfs", "grep", "chown", "chmod", "modprobe"):
            (self.bin / name).symlink_to(dispatcher)
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin", TEST_COMMAND_LOG=str(self.log),
                        TEST_MOUNTED=str(self.mounted), G5_DEV_HOST_REFLINK_IMAGE=str(self.image),
                        G5_DEV_HOST_REFLINK_ROOT=str(self.root), G5_DEV_HOST_REFLINK_OWNER="1234:5678")

    def invoke(self, fsize_kib=None, **env):
        """Run the mount script with the fake commands; `fsize_kib` lowers the hard file-size limit first."""
        script = str(SCRIPTS / "dev-host-reflink-root.sh")
        command = ["/bin/bash", script]
        if fsize_kib is not None:
            # A lowered hard file-size limit, as the agent harness hands its
            # process tree, that the script cannot lift back. Root holds
            # CAP_SYS_RESOURCE and would lift it (the script tests run as root
            # in Docker CI), so the capability leaves the bounding set first;
            # file access and the fake root id are unaffected.
            command = ["/bin/bash", "-c", f'ulimit -f {fsize_kib} && exec /bin/bash "$0"', script]
            if os.geteuid() == 0:
                setpriv = shutil.which("setpriv")
                if setpriv is None:
                    self.skipTest("root without setpriv cannot make the file-size limit unraisable")
                command = [setpriv, "--bounding-set=-sys_resource", "--", *command]
        return subprocess.run(command, env=dict(self.env, **env), capture_output=True, text=True,
                              timeout=20)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def truncate_sizes(self):
        """The `-s` sizes the script passed to truncate, in order."""
        return [command[2] for command in self.commands() if command[0] == "truncate"]

    def loop_commands(self):
        return [command for command in self.commands() if command[0] in ("losetup", "mount")]

    def test_fresh_root_mounts_only_allocated_loop_and_sets_requested_owner(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.image.is_file())
        self.assertEqual(self.loop_commands(), [
            ["losetup", "--find", "--show", str(self.image)],
            ["mount", "-t", "xfs", "-o", "noatime", LOOP, str(self.root)],
            ["losetup", "--detach", LOOP],
        ])
        self.assertIn(["chown", "1234:5678", str(self.root)], self.commands())
        self.assertIn(["chmod", "0755", str(self.root)], self.commands())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_existing_mount_is_reused_and_reflinks_are_rechecked(self):
        self.mounted.touch()
        for _ in range(2):
            result = self.invoke()
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.loop_commands(), [])
        self.assertEqual(sum(command[0] == "cp" for command in self.commands()), 2)
        self.assertFalse(self.image.exists())

    def test_mount_failure_detaches_only_owned_loop_and_preserves_primary_error(self):
        self.image.touch()
        for detach_failure in ("", "1"):
            with self.subTest(detach_failure=detach_failure):
                self.log.write_text("")
                result = self.invoke(TEST_MOUNT_FAIL="1", TEST_DETACH_FAIL=detach_failure)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("mount-primary-failure", result.stderr)
                self.assertIn("dev-host-reflink-root", result.stderr)
                self.assertEqual(self.loop_commands(), [
                    ["losetup", "--find", "--show", str(self.image)],
                    ["mount", "-t", "xfs", "-o", "noatime", LOOP, str(self.root)],
                    ["losetup", "--detach", LOOP],
                ])
                self.assertFalse(any(command[0] == "cp" for command in self.commands()))
                # The owner holds the mount point even though the mount failed,
                # so the stop recipes can create the host's directories under it.
                names = [command[0] for command in self.commands()]
                self.assertLess(names.index("chown"), names.index("losetup"))

    def test_failed_allocation_never_detaches_an_unowned_loop(self):
        self.image.touch()
        result = self.invoke(TEST_ALLOCATE_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("allocation-primary-failure", result.stderr)
        self.assertEqual(self.loop_commands(), [["losetup", "--find", "--show", str(self.image)]])

    def test_detach_failure_after_successful_mount_stops_before_chown_or_probe(self):
        self.image.touch()
        result = self.invoke(TEST_DETACH_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("detach-secondary-failure", result.stderr)
        self.assertIn("cannot mark /dev/loop23 for cleanup on unmount", result.stderr)
        self.assertTrue(self.mounted.exists())
        self.assertEqual(self.loop_commands(), [
            ["losetup", "--find", "--show", str(self.image)],
            ["mount", "-t", "xfs", "-o", "noatime", LOOP, str(self.root)],
            ["losetup", "--detach", LOOP],
        ])
        self.assertFalse(any(command[0] in ("cp", "chmod") for command in self.commands()))
        self.assertEqual(list(self.root.iterdir()), [])

    def test_a_foreign_mount_is_refused_before_its_ownership_changes(self):
        self.mounted.touch()
        result = self.invoke(TEST_FSTYPE="ext4")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("mounted but not XFS", result.stderr)
        self.assertFalse(any(command[0] in ("chown", "chmod", "cp", "losetup") for command in self.commands()))

    def test_image_is_sized_within_a_file_size_limit_the_script_cannot_lift(self):
        # 12 GiB in the 1 KiB blocks `ulimit -f` counts; the fake df offers 64 GiB.
        result = self.invoke(fsize_kib=12 * 1024 * 1024)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.truncate_sizes(), ["12G"])
        self.assertTrue(self.image.is_file())
        self.assertFalse(Path(str(self.image) + ".tmp").exists())

    def test_a_file_size_limit_below_the_minimum_is_refused_before_any_image_write(self):
        result = self.invoke(fsize_kib=4 * 1024 * 1024)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ulimit -f", result.stderr)
        self.assertIn("dev-host-reflink-root", result.stderr)
        self.assertEqual(self.truncate_sizes(), [])
        self.assertFalse(self.image.exists())
        self.assertEqual(self.loop_commands(), [])

    def test_failed_image_creation_leaves_no_partial_image_behind(self):
        result = self.invoke(TEST_MKFS_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("mkfs-primary-failure", result.stderr)
        self.assertFalse(self.image.exists())
        self.assertFalse(Path(str(self.image) + ".tmp").exists())
        self.assertEqual(self.loop_commands(), [])


if __name__ == "__main__":
    unittest.main()
