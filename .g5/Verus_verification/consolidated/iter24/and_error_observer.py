"""Classify the original direct And exception, independently of the catch bit."""
import hashlib


def facts(stdout: bytes):
    lines = [line for line in stdout.splitlines()
             if line.startswith(b'SEMVER-AND-ERROR|')]
    assert len(lines) == 1, 'Missing or duplicate direct And exception'
    assert (b'No overlapping versions in AND operation for =1.0.0+snapshot '
            b'and >=1.0.0+snapshot') in lines[0], lines[0]
    return {
        'stdout_sha256': hashlib.sha256(stdout).hexdigest(),
        'direct_and_error_original_line_hex': lines[0].hex(),
        'kind': 'NoOverlap in AND for exact snapshot and installed lower bound',
        'scope': 'Independent original-byte check qualifies the caught exception type; a Boolean catch alone is not an error-kind oracle. No new production-member or model-field credit.',
    }
