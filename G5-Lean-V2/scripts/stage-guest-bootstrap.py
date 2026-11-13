#!/usr/bin/env python3
"""Stage the platform-owned loader, Git, and library closure into a runtime.

Inputs are trusted pipeline binaries, never executables from a customer image.
The closure stays separate from provider toolchains in runtime/bin.
"""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess


def dependencies(binary):
    result = subprocess.run(['ldd', str(binary)], text=True, capture_output=True)
    if 'not found' in result.stdout or result.returncode:
        raise RuntimeError(f'incomplete dynamic closure for {binary}')
    return [Path(p) for p in re.findall(r'(?:=>\s+|^\s*)(/[^\s]+)', result.stdout, re.M)]


def stage(runtime):
    bootstrap = runtime / 'bootstrap'
    if bootstrap.exists():
        shutil.rmtree(bootstrap)
    libraries = bootstrap / 'libs'
    libraries.mkdir(parents=True)
    binaries = {'git.real': Path(shutil.which('git') or '/usr/bin/git')}
    git_exec = Path(subprocess.check_output([str(binaries['git.real']), '--exec-path'], text=True).strip())
    binaries['git-remote-http.real'] = git_exec / 'git-remote-http'
    closure = set()
    for source in [runtime / 'bin/g5d', *binaries.values()]:
        closure.update(dependencies(source))
    loaders = [p for p in closure if p.name.startswith(('ld-linux', 'ld-musl'))]
    if len(loaders) != 1:
        raise RuntimeError('expected one platform loader')
    shutil.copyfile(loaders[0], bootstrap / 'loader')
    os.chmod(bootstrap / 'loader', 0o755)
    for library in closure:
        destination = libraries / library.name
        if destination.exists() and destination.read_bytes() != library.read_bytes():
            raise RuntimeError(f'conflicting runtime library: {library.name}')
        shutil.copyfile(library, destination)
        os.chmod(destination, 0o755)
    for name, source in binaries.items():
        shutil.copyfile(source, bootstrap / name)
        os.chmod(bootstrap / name, 0o755)
    busybox = Path(shutil.which('busybox') or '/usr/bin/busybox')
    probe = subprocess.run(['ldd', str(busybox)], text=True, capture_output=True)
    if 'statically linked' not in probe.stdout + probe.stderr and 'not a dynamic executable' not in probe.stdout + probe.stderr:
        raise RuntimeError('bootstrap requires busybox-static')
    shutil.copyfile(busybox, bootstrap / 'busybox')
    os.chmod(bootstrap / 'busybox', 0o755)
    shutil.copyfile('/etc/ssl/certs/ca-certificates.crt', bootstrap / 'ca-certificates.crt')
    for program, real in [('git', 'git.real'), ('git-remote-http', 'git-remote-http.real'), ('git-remote-https', 'git-remote-http.real')]:
        # Fixed interpreter, loader, library search and helper path. No image PATH.
        wrapper = bootstrap / program
        wrapper.write_text('#!/opt/g5/bootstrap/busybox sh\n'
                           'export GIT_EXEC_PATH=/opt/g5/bootstrap\n'
                           'exec /opt/g5/bootstrap/loader --inhibit-cache '
                           '--library-path /opt/g5/bootstrap/libs '
                           f'/opt/g5/bootstrap/{real} "$@"\n')
        wrapper.chmod(0o755)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('runtime', type=Path)
    stage(parser.parse_args().runtime.resolve(strict=True))
