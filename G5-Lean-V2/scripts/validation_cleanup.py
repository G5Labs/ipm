"""Remove disposable validation image tags with explicit worktree ownership."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

WORKTREE_LABEL = 'io.g5.validation.worktree'
PROJECT_LABEL = 'io.g5.validation.project'


def capture(*args, **kwargs):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE, timeout=60, **kwargs).strip()


def scoped_tag(tag, project):
    """Accept generated service tags and the two explicitly scoped runner tags."""
    return tag == f'{project}/api:test' or tag == f'{project}-playwright:ci' or bool(
        re.fullmatch(re.escape(project) + r'-[a-z0-9_-]+:latest', tag))


def remove_tags(tags):
    """Delete tags without force; Docker protects images still used by containers."""
    status = 0
    for tag in sorted(set(tags)):
        result = subprocess.run(['docker', 'image', 'rm', tag], check=False)
        status = result.returncode or status
    return status


def down(root, compose, project):
    """Tear down this project's runtime, then remove only its generated build tags."""
    env = dict(os.environ, COMPOSE_PROJECT_NAME=project, G5_TEST_API_IMAGE=f'{project}/api:test')
    command = ['docker', 'compose', '--profile', '*', '-p', project, '-f', compose]
    config = json.loads(capture(*command, 'config', '--format', 'json', cwd=root, env=env))
    tags = {service.get('image', f'{project}-{name}:latest') for name, service in config['services'].items() if service.get('build')}
    tags.add(f'{project}-playwright:ci')
    # Include only existing scoped tags, never shared base/registry images or a
    # custom browser image supplied by a caller.
    existing = set(capture('docker', 'image', 'ls', '--format', '{{.Repository}}:{{.Tag}}').splitlines())
    status = subprocess.call([*command, 'down', '-v', '--remove-orphans'], cwd=root, env=env)
    if status:
        return status
    return remove_tags(tag for tag in tags & existing if scoped_tag(tag, project))


def collect_orphan_images(prune=False):
    """Preview or remove labelled tags for deleted worktrees with no containers."""
    ids = capture('docker', 'image', 'ls', '-q', '--filter', f'label={WORKTREE_LABEL}').split()
    if not ids:
        print('No labelled validation images to collect.')
        return 0
    images = json.loads(capture('docker', 'image', 'inspect', *sorted(set(ids))))
    tags = set()
    for image in images:
        labels = image.get('Config', {}).get('Labels') or {}
        root = labels.get(WORKTREE_LABEL, '')
        project = labels.get(PROJECT_LABEL, '')
        if not root or not Path(root).is_absolute() or Path(root).exists() or not re.fullmatch(r'g5-local-[0-9a-f]{10}', project):
            continue
        # Labels survive worktree deletion; the generated project hash provides
        # a second check that this is a managed local project, not a custom one.
        expected = 'g5-local-' + hashlib.sha256(json.dumps(str(Path(root).resolve())).encode()).hexdigest()[:10]
        if project != expected:
            continue
        if capture('docker', 'ps', '-aq', '--filter', f'label=com.docker.compose.project={project}'):
            print(f'Keeping {project}: containers still exist; inspect its retained diagnostics before teardown.')
            continue
        tags.update(tag for tag in image.get('RepoTags') or [] if scoped_tag(tag, project))
    for tag in sorted(tags):
        print(f'{"Removing" if prune else "Would remove"}: {tag}', flush=True)
    if not prune:
        print('Preview only. Run just ci-docker-gc true to remove these tags.')
        return 0
    return remove_tags(tags)
