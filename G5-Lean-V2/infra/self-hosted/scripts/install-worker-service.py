#!/usr/bin/env python3
"""Install systemd supervision around the trusted, immutable platform OCI root.

This never mounts a guest disk or extracts a customer environment. Docker is only
an artifact reader; systemd supplies mount setup and scoped cgroup delegation.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def unit_string(value):
    if any(c in value for c in '\n\r\x00'):
        raise ValueError('multiline values are not valid unit settings')
    return json.dumps(value.replace('%', '%%'))


def unit_path(value):
    # Scalar paths (RootDirectory, EnvironmentFile, What, Where) keep quotes.
    # List-valued settings below use unit_string for their word parser instead.
    value = str(value)
    if any(c in value for c in '\n\r\x00') or value != value.strip():
        raise ValueError('unit paths cannot contain line breaks or edge whitespace')
    return value.replace('%', '%%')


def unit_bind(source, destination):
    # The colon must remain outside quotes so systemd parses two paths.
    return f'{unit_string(str(source))}:{unit_string(str(destination))}'


def install(root, runtime, data, disk, env_file):
    for path in (root, runtime, data, disk, env_file):
        if not path.is_absolute() or ':' in str(path):
            raise ValueError('service paths must be absolute and contain no colon')
    for required in ('usr/local/bin/g5d', 'usr/bin/passt', 'usr/sbin/nft', 'opt/g5-host/bin/cloud-hypervisor',
                     'opt/g5-host/boot/vmlinux', 'opt/g5-host/boot/initramfs.cpio',
                     'etc/apparmor.d/usr.bin.passt'):
        if not (root / required).is_file():
            raise ValueError(f'platform worker image is incomplete: {required}')
    # Use distinct host identities, never the image's possibly colliding IDs.
    identities = {}
    for name in ('g5-vmm', 'g5-passt'):
        probe = subprocess.run(['getent', 'passwd', name], capture_output=True, text=True)
        if probe.returncode:
            subprocess.run(['useradd', '--system', '--no-create-home', '--shell', '/usr/sbin/nologin', name], check=True)
        fields = run('getent', 'passwd', name).split(':')
        identities[name] = (fields[2], fields[3])
        if '0' in identities[name]:
            raise ValueError(f'{name} must be unprivileged')
    kvm_gid = str(os.stat('/dev/kvm').st_gid)
    if kvm_gid == '0':
        raise ValueError('/dev/kvm must belong to a non-root KVM group')
    passwd = root / 'etc/passwd'
    lines = passwd.read_text().splitlines()
    for name, (uid, gid) in identities.items():
        lines = [line for line in lines if line.split(':')[0] != name]
        lines.append(f'{name}:x:{uid}:{gid}::/nonexistent:/usr/sbin/nologin')
    passwd.write_text('\n'.join(lines) + '\n')
    group = root / 'etc/group'
    lines = [line for line in group.read_text().splitlines()
             if line.split(':')[0] not in (*identities, 'kvm')]
    lines += [f'{name}:x:{gid}:' for name, (_, gid) in identities.items()]
    lines.append(f'kvm:x:{kvm_gid}:g5-vmm')
    group.write_text('\n'.join(lines) + '\n')
    for name in ('etc/resolv.conf', 'etc/hosts'):
        target = root / name
        target.unlink(missing_ok=True)
        target.touch(mode=0o644)
    for name in ('proc', 'sys', 'dev', 'run', 'tmp', 'var/tmp', 'var/g5d-data',
                 'var/lib/g5d/rootfs', 'opt/g5-runtime'):
        (root / name).mkdir(parents=True, exist_ok=True)
    (data / 'state/home').mkdir(parents=True, exist_ok=True)
    # A restrictive installer umask must not prevent the unprivileged VMM and
    # passt from traversing the canonical rootfs bind. Neither may list it.
    # The VMM joins passt's group; individual disks retain their private modes.
    for directory in (root / 'var/lib/g5d', data / 'rootfs'):
        directory.mkdir(mode=0o710, exist_ok=True)
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError('rootfs store and parent must be real directories')
        os.chown(directory, 0, int(identities['g5-passt'][1]))
        directory.chmod(0o710)
    # Load the image's reviewed profile with the host parser. No unconfined or
    # complain-mode fallback: an incompatible host must fail installation.
    subprocess.run(['apparmor_parser', '-r', str(root / 'etc/apparmor.d/usr.bin.passt')], check=True)
    mount_name = run('systemd-escape', '--path', '--suffix=mount', str(data))
    units = Path('/etc/systemd/system')
    (units / mount_name).write_text(f'''[Unit]
Description=G5 private reflink store
Before=g5d-host.service
[Mount]
What={unit_path(disk)}
Where={unit_path(data)}
Type=xfs
Options=loop
[Install]
WantedBy=multi-user.target
''')
    (units / 'g5d-host.service').write_text(f'''[Unit]
Description=G5 worker host supervisor
After=network-online.target {mount_name} apparmor.service
Wants=network-online.target
Requires={mount_name}
[Service]
Type=simple
RootDirectory={unit_path(root)}
MountAPIVFS=yes
EnvironmentFile={unit_path(env_file)}
ExecStart=/usr/local/bin/g5d host
WorkingDirectory=/var/g5d-data
BindReadOnlyPaths={unit_bind(runtime, '/opt/g5-runtime')}
BindReadOnlyPaths=/etc/resolv.conf:/etc/resolv.conf /etc/hosts:/etc/hosts
BindPaths={unit_bind(data, '/var/g5d-data')}
BindPaths={unit_bind(data / 'rootfs', '/var/lib/g5d/rootfs')}
BindPaths=/dev/kvm
TemporaryFileSystem=/run
PrivateTmp=yes
PrivateDevices=yes
DevicePolicy=closed
DeviceAllow=/dev/kvm rw
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/g5d-data /var/lib/g5d/rootfs
AmbientCapabilities=CAP_NET_ADMIN CAP_CHOWN CAP_DAC_OVERRIDE CAP_FOWNER CAP_SETUID CAP_SETGID CAP_KILL
CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_RESOURCE CAP_CHOWN CAP_DAC_OVERRIDE CAP_FOWNER CAP_SETUID CAP_SETGID CAP_KILL
Delegate=yes
DelegateSubgroup=supervisor
LimitNOFILE=1048576
Restart=on-failure
RestartSec=30s
TimeoutStopSec=2100s
KillMode=control-group
StandardOutput=journal
StandardError=journal
SyslogIdentifier=g5d-host
[Install]
WantedBy=multi-user.target
''')
    # systemd-analyze verifies ExecStart against the manager's root, ignoring
    # RootDirectory (including in systemd 255). Resolve only that executable
    # in a temporary verification copy; the installed service still executes
    # /usr/local/bin/g5d inside its immutable RootDirectory.
    with tempfile.TemporaryDirectory(prefix='g5-worker-units-') as temporary:
        verification = Path(temporary)
        service = (units / 'g5d-host.service').read_text().replace(
            'ExecStart=/usr/local/bin/g5d host',
            f'ExecStart={unit_string(str(root / "usr/local/bin/g5d"))} host')
        (verification / 'g5d-host.service').write_text(service)
        (verification / mount_name).write_text((units / mount_name).read_text())
        subprocess.run(['systemd-analyze', 'verify', str(verification / 'g5d-host.service'),
                        str(verification / mount_name)], check=True)
    subprocess.run(['systemctl', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', 'enable', mount_name, 'g5d-host.service'], check=True)
    subprocess.run(['systemctl', 'start', mount_name, 'g5d-host.service'], check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'runtime', 'data', 'disk', 'env-file'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    install(args.root, args.runtime, args.data, args.disk, args.env_file)
