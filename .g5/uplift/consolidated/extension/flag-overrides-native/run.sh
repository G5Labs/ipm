#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
/usr/bin/python3 "$here/run.py"
result=$(/usr/bin/python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["result"])' "$here/latest.json")
docker exec -i ipm-consolidated-iris iris session IRIS -U USER < "$here/cleanup-helper.cos" > "$here/$result/cleanup-helper.log" 2>&1
