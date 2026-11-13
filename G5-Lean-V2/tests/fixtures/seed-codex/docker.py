#!/usr/bin/env python3
"""Replace only seed Docker transport with the test app's real DB and Git files."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.error
import urllib.request


def local_path(value):
    return value.replace("/data/repos", os.environ["SEED_TEST_REPOS"]).replace(
        "/tmp/ontology_seed_codex.sql", os.environ["SEED_TEST_SQL"]
    )


args = sys.argv[1:]
operation = args.pop(0)
if operation == "cp":
    source, destination = args
    assert destination.startswith("g5-dev-postgres:"), destination
    shutil.copyfile(source, local_path(destination.split(":", 1)[1]))
    sys.exit(0)

assert operation == "exec", operation
if args[0] == "-i":
    args.pop(0)
container = args.pop(0)
if container == "g5-dev-postgres":
    assert args[:5] == ["psql", "-U", "g5", "-d", "g5_dev"], args
    args = args[5:]
    variables = {}
    sql = None
    while args:
        flag = args.pop(0)
        value = args.pop(0)
        if flag == "-v":
            key, value = value.split("=", 1)
            variables[key] = value
        elif flag == "-tAc":
            sql = value
        elif flag == "-f":
            sql = Path(local_path(value)).read_text()
        else:
            raise AssertionError(f"Unexpected psql flag: {flag}")
    assert sql is not None
    # Simulate a successful lookup with no row, as after concurrent removal.
    empty_query = os.environ.get("SEED_TEST_EMPTY_QUERY")
    if empty_query and sql.startswith(empty_query):
        sys.exit(0)
    for name, value in variables.items():
        sql = sql.replace(f":'{name}'", "'" + value.replace("'", "''") + "'")
    request = urllib.request.Request(
        os.environ["SEED_TEST_DB"], data=sql.encode(),
        headers={"Content-Type": "text/plain"},
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            print(response.read().decode())
    except urllib.error.HTTPError as error:
        sys.exit(error.read().decode())
    sys.exit(0)
else:
    assert container == "g5-dev-api", container
args = [local_path(arg) for arg in args]
# BSD tar has the same extraction semantics but no GNU ownership flag.
if sys.platform == "darwin" and args[0] == "tar":
    args = [arg for arg in args if arg != "--no-same-owner"]
sys.exit(subprocess.call(args))
