#!/usr/bin/env python3
"""Real trusted-bootstrap Git recovery with and without a seed, plus image-code probe.

Only generated repositories and fixture credentials are served on a private host
port. Run in a disposable Linux/KVM test unit using the same inputs as guest.sh.
"""
import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import uuid


def run(*args, **kw):
    return subprocess.check_output([str(a) for a in args], text=True, **kw).strip()


def git(repo, *args):
    return run('git', '-c', 'user.name=QA fixture', '-c', 'user.email=qa@example.invalid', '-C', repo, *args)


def main(incident=False):
    root = Path(os.environ['G5_TEST_VM_ROOT'])
    base = Path(tempfile.mkdtemp(prefix='seed-check-', dir=root))
    guests = []
    success = False
    server = None
    # Leave room for the system-Python fixture inside the private filesystem.
    size = '81920' if incident else str(max(1024, int(os.environ.get('G5_TEST_DISK_MIB', '32768'))))
    g5d = os.environ['G5D_BIN']
    try:
        # Synthetic content-addressed fixture, isolated from the daemon's cache.
        # Reflink the pristine host tree; no guest-written disk is mounted.
        template = base / 'templates/sha256' / ('a' * 64)
        template.parent.mkdir(parents=True)
        run('cp', '-a', '--reflink=always', os.environ['G5_TEST_TEMPLATE'], template)
        image = template / 'rootfs'
        (image / 'image-generation').write_text('requested\n')
        # The image must be unable to run this constructor during vm-init/Git.
        # It must still run normally when the customer workload finally starts.
        probe = base / 'probe.c'
        probe.write_text(r'''
#define _GNU_SOURCE
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
__attribute__((constructor)) static void image_code(void) {
    char b[4096]; int f=open("/proc/self/cmdline",O_RDONLY); int n=f<0?0:read(f,b,sizeof(b)); if(f>=0)close(f);
    if(n>0 && (memmem(b,n,"vm-init",7) || memmem(b,n,"git.real",8) || memmem(b,n,"git-remote-http.real",20))) _exit(88);
    f=open("/work/image-code-ran",O_WRONLY|O_CREAT,0600); if(f>=0)close(f);
}
''')
        run('cc', '-shared', '-fPIC', '-Werror', '-Wall', probe, '-o', image / 'image-probe.so')
        preload = image / 'etc/ld.so.preload'
        preload.unlink(missing_ok=True)
        preload.write_text('/image-probe.so\n')
        # A deliberately unusable image NSS setting must not govern Git recovery.
        nss = image / 'etc/nsswitch.conf'
        nss.unlink(missing_ok=True)
        nss.write_text('hosts: missing_customer_module\n')
        sources = base / 'sources'
        sources.mkdir()
        app = sources / 'app'
        docs = sources / 'docs'
        reference = sources / 'reference'
        personal = sources / 'personal'
        for repo in [app, docs, reference, personal]:
            repo.mkdir()
            git(repo, 'init', '-q', '-b', 'work')
            (repo / 'file').write_text('baseline\n')
            (repo / 'deleted').write_text('removed by dirty receipt\n')
            git(repo, 'add', '.')
            git(repo, 'commit', '-qm', 'baseline')
        old = git(app, 'rev-parse', 'HEAD')
        bundle = base / 'app.bundle'
        git(app, 'bundle', 'create', bundle, 'HEAD')
        (app / 'file').write_text('new commit\n')
        git(app, 'add', '.')
        git(app, 'commit', '-qm', 'incremental change')
        current = git(app, 'rev-parse', 'HEAD')
        doc_oid = git(docs, 'rev-parse', 'HEAD')
        references = [dict(repo_id=str(uuid.uuid4()), root_path=name, role='code', branch='work', commit=git(repo, 'rev-parse', 'HEAD')) for name, repo in [('reference', reference), ('personal', personal)]]
        ws, org, owner, app_id, docs_id, flush = [str(uuid.uuid4()) for _ in range(6)]
        # Immutable dirty receipt restores local HEAD independently from tree state.
        (app / 'file').write_text('dirty receipt\n')
        (app / 'untracked').write_text('saved untracked\n')
        (app / 'deleted').unlink()
        (app / 'link').symlink_to('file')
        (app / 'executable').write_text('#!/bin/sh\necho saved\n')
        (app / 'executable').chmod(0o755)
        git(app, 'add', '.')
        tree = git(app, 'write-tree')
        snapshot = git(app, 'commit-tree', tree, '-p', current, '-m', 'receipt')
        for repo, branch, snap in [(app, current, snapshot), (docs, doc_oid, doc_oid)]:
            git(repo, 'update-ref', f'refs/g5/flush/{ws}/{flush}/branch', branch)
            git(repo, 'update-ref', f'refs/g5/flush/{ws}/{flush}/snapshot', snap)
        www = base / 'http'
        www.mkdir()
        for name, repo in [('app', app), ('docs', docs), ('reference', reference), ('personal', personal)]:
            bare = www / f'{name}.git'
            run('git', 'clone', '-q', '--bare', repo, bare)
            # clone --bare only imports branch/tag refs; add receipt refs explicitly.
            git(bare, 'fetch', '-q', str(repo), '+refs/g5/*:refs/g5/*')
            git(bare, 'update-server-info')
        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args): pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=www))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        port = server.server_address[1]
        api = f'http://localhost:{port}'
        for ref in references:
            ref['git_url'] = f"{api}/{ref['root_path']}.git"
        members = [dict(repo_id=app_id, root_path='app', role='code', git_url=f'{api}/app.git', branch='work', branch_oid=current),
                   dict(repo_id=docs_id, root_path='docs', role='docs', git_url=f'{api}/docs.git', branch='work', branch_oid=doc_oid)]
        manifest = base / 'seed.json'
        manifest.write_text(json.dumps(dict(scope=dict(org_id=org, repos=[{k: members[0][k] for k in ['repo_id','root_path','git_url']}]),
            repositories=[dict(repo_id=app_id, bundle=str(bundle), commit=old)])))
        start_time = time.monotonic()
        run(g5d, 'vm-exec', '--prepare-block-template', '--rootfs', image,
            '--scratch-disk-mib', size, '--block-template-reserve-mib', '0', '--block-template-repo-seed', manifest)
        print(f'seed_fixture_preparation_seconds={time.monotonic()-start_time:.3f}', flush=True)
        cases = [(False, False)] if incident else [(True, False), (True, True), (False, False), (False, True)]
        for seeded, dirty in cases:
            pressure = base / 'pressure'
            if not seeded and not dirty:
                # Reproduce the old cold-path refusal on the real filesystem:
                # an older prepared image and free space between V and 2 × V.
                older = base / 'templates/sha256' / ('b' * 64)
                run('cp', '-a', '--reflink=always', template, older)
                (older / 'rootfs/image-generation').write_text('older\n')
                run(g5d, 'vm-exec', '--prepare-block-template', '--rootfs', older / 'rootfs', '--scratch-disk-mib', size)
                stats = os.statvfs(base)
                available = stats.f_bavail * stats.f_frsize
                target = int(int(size) * 1024 * 1024 * 1.96)
                assert available > target, f'fixture needs {target} free bytes, has {available}'
                run('fallocate', '-l', available - target, pressure)
                stats = os.statvfs(base)
                available = stats.f_bavail * stats.f_frsize
                assert int(size) * 1024 * 1024 <= available < int(size) * 2 * 1024 * 1024
                if incident:
                    assert abs(available - 156.8 * (1 << 30)) < 1 << 20
                print(f'COLD_IMAGE_AVAILABLE_BYTES={available} V_BYTES={int(size)*1024*1024}', flush=True)
            owned = Path(tempfile.mkdtemp(prefix='seed-boot-', dir=root))
            guests.append(owned)
            rootfs = owned / 'rootfs'
            (rootfs / '.g5').mkdir(parents=True)
            pin = base / f'pin-{seeded}-{dirty}'
            (pin / '.g5').mkdir(parents=True)
            if seeded:
                run('cp', '--reflink=always', base / 'templates/workload-block/repo-current/image.ext4', pin / '.g5/repo-seed.ext4')
                (pin / '.g5/repo-seed.ext4').chmod(0o600)
            vm = str(uuid.uuid4())
            spec = dict(spec_version=1, dispatcher_contract_version=1,
                incarnation=dict(workspace_id=ws, vm_id=vm, epoch=1, org_id=org, owner_user_id=owner, lease_ttl_secs=60, idle_timeout_secs=300),
                image={'ref':'synthetic-image', 'digest':'a'*64}, size=dict(cpus=2, mem_mib=2048, scratch_disk_mib=int(size)),
                workspace_root=dict(retain_hours=0,cache_generation=1), repos=members, reference_repos=references, env={}, network='passt', entrypoint=['/bin/true'],
                module_catalog_version='fixture', module_catalog_hash='b'*64)
            if dirty:
                spec['restore']=dict(flush_id=flush, repos=[dict(repo_id=m['repo_id'], branch_oid=m['branch_oid'],
                    local_branch_oid=m['branch_oid'], local_head_oid=m['branch_oid'], head_ref='refs/heads/work', sequencer='none',
                    snapshot_oid=snapshot if m['repo_id']==app_id else doc_oid, dirty=m['repo_id']==app_id) for m in members])
            start = dict(workspace_id=ws,vm_id=vm,spec=spec,workspace_token='generated-fixture-token')
            env=os.environ.copy()
            env.update(G5D_INTERNAL_OWNED_WORKLOAD_ROOT=str(rootfs), G5D_INTERNAL_WORKLOAD_TEMPLATE=str(template),
                G5D_INTERNAL_GUEST_RESTORE=json.dumps(start), G5D_PASST_HOST_API_URL=api)
            expected='dirty receipt' if dirty else 'new commit'
            script=f'''set -eu
[ "$(cat /sys/class/block/vda/size)" = {int(size) * 2048} ]
[ "$(cat /image-generation)" = requested ]
[ "$(git -C /work/app rev-parse HEAD)" = {current} ]
[ "$(git -C /work/app symbolic-ref --short HEAD)" = work ]
[ "$(cat /work/app/file)" = '{expected}' ]
[ "$(git -C /work/docs rev-parse HEAD)" = {doc_oid} ]
[ "$(readlink /work/project_docs)" = docs/docs/{ws} ]
[ -f /work/image-code-ran ]
[ ! -e /work/app/.git/objects/info/alternates ]
'''
            # Cover both the seeded member and the additional member initialized
            # in the guest. Bootstrap's host-door rewrite must not become origin.
            for ref in references:
                script += f'''[ "$(git -C /work/{ref['root_path']} rev-parse HEAD)" = '{ref['commit']}' ]
[ "$(git -C /work/{ref['root_path']} symbolic-ref --short HEAD)" = work ]
[ "$(git -C /work/{ref['root_path']} config remote.origin.url)" = '{ref['git_url']}' ]
[ -w /work/{ref['root_path']} ]
'''
            for member in members:
                checkout = f"/work/{member['root_path']}"
                script += f'''[ "$(git -C {checkout} remote)" = origin ]
[ "$(git -C {checkout} config --local remote.origin.url)" = '{member['git_url']}' ]
[ "$(git -C {checkout} config --get-all remote.origin.fetch)" = '+refs/heads/*:refs/remotes/origin/*' ]
[ "$(git -C {checkout} rev-parse refs/remotes/origin/work)" = '{member['branch_oid']}' ]
'''
            script += '[ -f /work/app/untracked ]\n[ -n "$(git -C /work/app status --porcelain)" ]\n' if dirty else '[ -z "$(git -C /work/app status --porcelain)" ]\n'
            if dirty:
                script += '[ ! -e /work/app/deleted ]\n[ -x /work/app/executable ]\n[ "$(readlink /work/app/link)" = file ]\n'
            # Guest writes outside the checked repositories grow the sparse disk.
            script += 'head -c 268435456 /dev/urandom > /work/g5-sparse-probe\nsync\nsleep 1\n'
            script += f'echo SEED_GUEST_RESTORE_OK_dirty_{dirty}\n'
            # Sample the host file backing the guest's private disk while it runs.
            samples, stop = [], threading.Event()
            def watch(disk=owned / 'workload.ext4'):
                while not stop.is_set():
                    try:
                        samples.append(disk.stat().st_blocks * 512)
                    except FileNotFoundError:
                        pass
                    stop.wait(0.05)
            watcher = threading.Thread(target=watch, daemon=True)
            watcher.start()
            began=time.monotonic()
            result=subprocess.run([g5d,'vm-exec','--rootfs',str(rootfs),'--cpus','2','--mem-mib','2048','--scratch-disk-mib',size,
                '--block-workload','-V',f'{pin}:/work','-V',os.environ['G5_TEST_RUNTIME']+':/opt/g5:ro','--','/bin/bash','-c',script],
                env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120)
            stop.set()
            watcher.join()
            (owned / 'console.log').write_text(result.stdout)
            print(result.stdout,flush=True)
            pressure.unlink(missing_ok=True)
            assert result.returncode==0, result.returncode
            assert f'SEED_GUEST_RESTORE_OK_dirty_{dirty}' in result.stdout
            # Virtual capacity is V (checked in the guest); physical allocation
            # starts as a sparse clone and grows only with the guest's writes.
            v = int(size) << 20
            assert samples, 'the private disk was never observed'
            assert min(samples) < v // 2, (min(samples), v)
            assert max(samples) - min(samples) >= 200 << 20, (min(samples), max(samples))
            assert max(samples) < v - (256 << 20), (max(samples), v)
            print(f'sparse_disk_bytes initial={min(samples)} peak={max(samples)} v={v} seeded={seeded} dirty={dirty}', flush=True)
            print(f'guest_start_and_restore_seconds={time.monotonic()-began:.3f} seeded={seeded} dirty={dirty}',flush=True)
        success=True
        if incident:
            print('QA_1049_80_GIB_COLD_IMAGE_GUEST_RESTORE_OK', flush=True)
        print('CH_SEED_RESTORE_AND_IMAGE_BOUNDARY_OK',flush=True)
    finally:
        if server:
            server.shutdown()
        if success:
            for owned in guests:
                shutil.rmtree(owned)
            shutil.rmtree(base)
        else:
            print(f'Failed fixture retained: {base}',flush=True)


if __name__=='__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--incident', action='store_true', help='literal V=80 GiB / 156.8 GiB free cold-image reproduction')
    main(incident=parser.parse_args().incident)
