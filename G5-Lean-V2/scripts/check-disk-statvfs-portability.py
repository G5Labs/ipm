#!/usr/bin/env python3
"""Compile the actual storage helper against Linux and Darwin libc field types.

Only libc is needed. This keeps unrelated platform dependencies (ring's C
toolchain, Linux process custody, etc.) out of this narrow portability check.
Install the requested Rust targets first; Darwin checks do not link or execute.
"""
import argparse
from pathlib import Path
import subprocess
import tempfile
import tomllib


def main():
    repo = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=repo / 'backend/g5d-foundation/src/disk.rs')
    parser.add_argument('--target', action='append', required=True)
    args = parser.parse_args()
    source = args.source.read_text()
    lock = tomllib.loads((repo / 'Cargo.lock').read_text())
    libc = next(package['version'] for package in lock['package'] if package['name'] == 'libc')
    start = source.index('pub fn free_bytes(')
    end = source.index('pub fn pressure(', start)
    with tempfile.TemporaryDirectory(prefix='g5-statvfs-') as directory:
        root = Path(directory)
        (root / 'src').mkdir()
        (root / 'Cargo.toml').write_text('''[package]
name = "g5-statvfs-portability"
version = "0.0.0"
edition = "2021"
[dependencies]
libc = "=LIBC_VERSION"
'''.replace('LIBC_VERSION', libc))
        (root / 'src/lib.rs').write_text('use std::{io, path::Path};\n' + source[start:end] + '''
#[test]
fn reads_real_filesystem() {
    assert!(free_bytes(Path::new("/")).is_ok());
}
''')
        for target in args.target:
            operation = 'test' if target == 'x86_64-unknown-linux-gnu' else 'check'
            subprocess.run(['cargo', operation, '--offline', '--manifest-path', str(root / 'Cargo.toml'),
                            '--target', target], check=True)
            print(f'STATVFS_{operation.upper()}_OK target={target} source={args.source}', flush=True)


if __name__ == '__main__':
    main()
