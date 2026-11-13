#!/usr/bin/env python3
"""Build a tiny trusted CI fixture through the real OCI -> ext4 pipeline.

Only for disposable KVM tests. Guest tools come from the test runner, never a
customer image. The optional runtime input is the complete baked worker runtime.
"""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile


def run(*args, **kwargs):
    return subprocess.check_output([str(x) for x in args], text=True, **kwargs).strip()


def prepare(root, binary, runtime=None, docker=False):
    root.mkdir(parents=True, exist_ok=True)
    source = root / 'fixture-source'
    source.mkdir()
    for name in ['bin', 'usr/bin', 'etc', 'tmp', 'work']:
        (source / name).mkdir(parents=True, exist_ok=True)
    shutil.copyfile(shutil.which('busybox'), source / 'bin/busybox')
    (source / 'bin/busybox').chmod(0o755)
    for name in run(source / 'bin/busybox', '--list').splitlines():
        if name == 'busybox':
            continue
        (source / 'bin' / name).symlink_to('busybox')
    (source / 'usr/bin/env').symlink_to('/bin/busybox')
    # Provider wrappers require the supported base-image ABI: glibc and Bash.
    # Do not mistake a Python-only filesystem for that image contract.
    bash = Path('/bin/bash').resolve()
    (source / 'bin/bash').unlink(missing_ok=True)
    shutil.copy2(bash, source / 'bin/bash')
    git = Path(shutil.which('git')).resolve()
    shutil.copy2(git, source / 'usr/bin/git')
    python = Path(sys.executable).resolve()
    shutil.copy2(python, source / 'usr/bin/python3')
    version = f'python{sys.version_info.major}.{sys.version_info.minor}'
    library = Path(sys.base_prefix) / 'lib' / version
    shutil.copytree(library, source / 'usr/lib' / version,
                    ignore=shutil.ignore_patterns('site-packages', 'dist-packages', '__pycache__', 'test', 'tests', 'config-*'))
    # System Python and extension shared libraries form a small guest fixture.
    # Preserve their absolute loader paths inside the fixture, never host mounts.
    binaries = [python, bash, git, *library.glob('lib-dynload/*.so')]
    if docker:
        # Real Docker exec/health checks must run behind the candidate init, not
        # only in a Python replay of its mounts. Copy the runner's packaged tools
        # and their library closure; no registry downloads are needed in the guest.
        # dockerd resolves docker-proxy at startup even with the bridge disabled;
        # executable helpers are not part of the shared-library closure below.
        for name in ['docker', 'dockerd', 'docker-proxy', 'containerd', 'containerd-shim-runc-v2', 'runc']:
            executable = shutil.which(name)
            if not executable:
                raise RuntimeError(f'fixture requires {name}')
            executable = Path(executable).resolve()
            shutil.copy2(executable, source / 'usr/bin' / name)
            binaries.append(executable)
        compose = next((path for base in ['/usr/libexec', '/usr/lib', '/usr/local/lib']
                        if (path := Path(base) / 'docker/cli-plugins/docker-compose').is_file()), None)
        if compose is None:
            raise RuntimeError('fixture requires the packaged Docker Compose plugin')
        destination = source / 'usr/libexec/docker/cli-plugins/docker-compose'
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(compose, destination)
        binaries.append(compose)
        # Inner host inside the guest (scripts/test-guest-nested-kvm.py --vm-exec):
        # the in-guest g5d prepares a workload template (mkfs.ext4, GNU cp with
        # --reflink, tar) on a loop-mounted XFS root (mkfs.xfs, mount) and
        # launches vm-exec, whose egress policy runs /usr/sbin/nft. GNU cp and
        # tar must shadow the busybox applets, so they land first on PATH.
        for name, where in [('nft', 'usr/sbin'), ('mkfs.ext4', 'usr/sbin'), ('e2fsck', 'usr/sbin'),
                            ('mkfs.xfs', 'usr/sbin'), ('losetup', 'usr/sbin'),
                            ('mount', 'usr/bin'), ('umount', 'usr/bin'), ('cp', 'usr/bin'),
                            ('tar', 'usr/bin'), ('truncate', 'usr/bin')]:
            executable = shutil.which(name)
            if not executable:
                raise RuntimeError(f'fixture requires {name} for the inner host probe')
            executable = Path(executable).resolve()
            target = source / where / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.unlink(missing_ok=True)
            shutil.copy2(executable, target)
            binaries.append(executable)
        for config in [Path('/etc/mke2fs.conf')]:
            if config.is_file():
                shutil.copy2(config, source / str(config).lstrip('/'))
    # glibc loads this lazily for pthread cancellation; it is absent from ldd's
    # Python closure but is needed when the HTTP fixture shuts down its threads.
    libraries = run('ldconfig', '-p')
    for dependency in re.findall(r'lib(?:dl|pthread|rt|resolv)\.so\.\d+[^\n]*=>\s+(\S+)', libraries):
        dependency = Path(dependency)
        target = source / str(dependency).lstrip('/')
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(dependency, target)
    unwind = re.search(r'libgcc_s\.so\.1[^\n]*=>\s+(\S+)', libraries)
    if not unwind:
        raise RuntimeError('fixture requires the system libgcc unwinder')
    unwind = Path(unwind.group(1))
    destination = source / str(unwind).lstrip('/')
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(unwind, destination)
    for executable in binaries:
        result = subprocess.run(['ldd', str(executable)], capture_output=True, text=True)
        if 'not found' in result.stdout:
            raise RuntimeError(f'missing fixture dependency for {executable}')
        for dependency in re.findall(r'(?:=>\s+|^\s*)(/[^\s]+)', result.stdout, re.M):
            target = source / dependency.lstrip('/')
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.exists():
                shutil.copy2(dependency, target)
    # The inner host's identities (install-cloud-hypervisor-platform.sh,
    # 58-install-passt.sh): vm-exec drops the VMM to g5-vmm in the kvm group and
    # passt to g5-passt; both must be unprivileged, with non-root primary groups.
    (source / 'etc/passwd').write_text(
        'root:x:0:0:root:/root:/bin/sh\n'
        'g5-passt:x:998:998:passt sidecar:/nonexistent:/usr/sbin/nologin\n'
        'g5-vmm:x:999:999:VMM:/nonexistent:/usr/sbin/nologin\n')
    (source / 'etc/group').write_text(
        'root:x:0:\n'
        'kvm:x:108:g5-vmm\n'
        'g5-passt:x:998:\n'
        'g5-vmm:x:999:\n')
    layout = root / 'fixture-oci'
    blobs = layout / 'blobs/sha256'
    blobs.mkdir(parents=True)
    def blob(data, kind):
        digest = hashlib.sha256(data).hexdigest()
        (blobs / digest).write_bytes(data)
        return dict(mediaType=kind, digest='sha256:' + digest, size=len(data))
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode='w') as archive:
        for entry in sorted(source.iterdir()):
            archive.add(entry, arcname=entry.name)
    raw = data.getvalue()
    layer = blob(gzip.compress(raw), 'application/vnd.oci.image.layer.v1.tar+gzip')
    config = blob(json.dumps(dict(architecture='amd64', os='linux', config={}, rootfs=dict(type='layers',
        diff_ids=['sha256:'+hashlib.sha256(raw).hexdigest()]))).encode(), 'application/vnd.oci.image.config.v1+json')
    manifest = blob(json.dumps(dict(schemaVersion=2, config=config, layers=[layer])).encode(), 'application/vnd.oci.image.manifest.v1+json')
    (layout / 'index.json').write_text(json.dumps(dict(schemaVersion=2, manifests=[manifest])))
    (layout / 'oci-layout').write_text('{"imageLayoutVersion":"1.0.0"}')
    environment = dict(os.environ, G5D_ROOTFS_STATE_DIR=str(root / 'state'), G5D_OCI_CACHE_DIR=str(root / 'layers'))
    # The Docker fixture also hosts the inner-host probe's template, runtime
    # copy and XFS image on its disk.
    size_mib = '3072' if docker else '512'
    run(binary, 'oci', 'prepare-workload', layout, '--size-mib', size_mib, env=environment)
    templates = list((root / 'state/templates/sha256').iterdir())
    assert len(templates) == 1
    # The minimal runtime (the candidate g5d plus the trusted bootstrap, through
    # the real disk builder). It is the guest's runtime when the caller supplies
    # none, and always the inner host's runtime for the third-level boot in
    # test-guest-nested-kvm.py: the baked worker runtime a Packer gate passes
    # is several GB and must never be copied into the fixture guest.
    inner_runtime = root / 'inner-runtime'
    (inner_runtime / 'bin').mkdir(parents=True)
    shutil.copy2(binary, inner_runtime / 'bin/g5d')
    run('bash', Path(__file__).with_name('build-runtime-disk.sh'), inner_runtime)
    if runtime is None:
        runtime = inner_runtime
    (root / 'boots').mkdir()
    return dict(G5_TEST_VM_ROOT=str(root / 'boots'), G5_TEST_TEMPLATE=str(templates[0]),
                G5_TEST_RUNTIME=str(runtime), G5_TEST_INNER_RUNTIME=str(inner_runtime),
                G5_TEST_DISK_MIB=size_mib, G5_TEST_OCI_LAYOUT=str(layout), G5D_BIN=str(binary))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--runtime', type=Path)
    parser.add_argument('--docker', action='store_true', help='include Docker/Compose for exec regression checks')
    args = parser.parse_args()
    import shlex
    values = prepare(args.root.resolve(), args.binary.resolve(), args.runtime.resolve() if args.runtime else None, docker=args.docker)
    (args.root / 'test.env').write_text(''.join(f'export {k}={shlex.quote(v)}\n' for k,v in values.items()))
