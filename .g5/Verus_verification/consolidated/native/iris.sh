#!/bin/bash
# Disposable IRIS for the consolidated uplift (ARM64, IRIS Community 2026.1).
#   iris.sh up                 recreate the container from the pinned image and install IPM 571e04a1 from git archive
#   iris.sh exec NAME SCRIPT [NS]  pipe an ObjectScript terminal script; writes results/<UTC>-NAME/{script.cos,raw.log,receipt.json}
#   iris.sh state              print instance state (namespaces, IPM version, default modifiers, repos)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../../.." && pwd)
C=${CONTAINER:-ipm-consolidated-iris}
IMG=containers.intersystems.com/intersystems/iris-community:2026.1
PIN=571e04a19aba9f9b4bc0ea23fbe09182038f4e55
case "${1:-}" in
reset)
  # Recreate from the snapshot taken right after `up` (image ipm-consolidated-iris:571e04a1-fresh, fresh install, no repos).
  docker rm -f "$C" >/dev/null 2>&1 || true
  docker run -d --name "$C" ipm-consolidated-iris:571e04a1-fresh >/dev/null
  for i in $(seq 1 90); do
    s=$(docker inspect -f '{{.State.Health.Status}}' "$C" 2>/dev/null || echo none)
    [ "$s" = healthy ] && break; sleep 2; done
  echo "reset: $C from ipm-consolidated-iris:571e04a1-fresh ($s)";;
up)
  docker rm -f "$C" >/dev/null 2>&1 || true
  docker run -d --name "$C" "$IMG" >/dev/null
  for i in $(seq 1 90); do
    s=$(docker inspect -f '{{.State.Health.Status}}' "$C" 2>/dev/null || echo none)
    [ "$s" = healthy ] && break; sleep 2; done
  tmp=$(mktemp -d); git -C "$REPO" archive "$PIN" | tar -x -C "$tmp"
  docker exec -u root "$C" mkdir -p /home/irisowner/zpm
  docker cp "$tmp/." "$C":/home/irisowner/zpm/; rm -rf "$tmp"
  docker exec -u root "$C" chown -R irisowner /home/irisowner/zpm
  docker exec -i "$C" iris session IRIS -U USER <<'COS'
do $System.OBJ.Load("/home/irisowner/zpm/preload/cls/IPM/Installer.cls","ck")
set sc=##class(IPM.Installer).setup("/home/irisowner/zpm/",3) write !,"SETUP ",$system.Status.IsOK(sc),!
write !,"IPMVER=",##class(%IPM.Storage.Module).NameOpen("zpm").VersionString,!
halt
COS
  echo "up: $C image $(docker inspect -f '{{.Image}}' "$C")";;
exec)
  name=$2; script=$3; ns=${4:-USER}
  out="$HERE/results/$(date -u +%Y%m%dT%H%M%SZ)-$name"; mkdir -p "$out"
  cp "$script" "$out/script.cos"
  docker cp "$script" "$C":/tmp/run.cos >/dev/null
  # Fixtures (fixtures/<dir> -> /tmp/<dir>) and the observation harness class.
  for fx in ${FIXTURES:-}; do docker exec -u root "$C" rm -rf "/tmp/$fx"; docker cp "$HERE/fixtures/$fx" "$C":"/tmp/$fx" >/dev/null; done
  docker cp "$HERE/harness/Obs.cls" "$C":/tmp/Obs.cls >/dev/null
  docker exec -u root "$C" chown -R irisowner /tmp
  printf '%s\n' 'do $system.OBJ.Load("/tmp/Obs.cls","ck-d")' 'halt' | docker exec -i "$C" iris session IRIS -U "$ns" >/dev/null 2>&1
  start=$(date -u +%FT%TZ)
  printf '%s\n' 'write "ZV=",$zversion,!' 'halt' | docker exec -i "$C" iris session IRIS -U "$ns" 2>&1 | grep '^ZV=' > "$out/zversion.txt" || true
  set +e; docker exec ${EXEC_ENV:-} -i "$C" iris session IRIS -U "$ns" < "$script" > "$out/raw.log" 2>&1; rc=$?; set -e
  python3 -I - "$out" "$C" "$IMG" "$PIN" "$start" "$rc" "$ns" <<'PY'
import json,sys,hashlib,subprocess
out,c,img,pin,start,rc,ns=sys.argv[1:]
raw=open(out+'/raw.log',errors='replace').read()
arch=subprocess.run(['docker','exec',c,'uname','-m'],capture_output=True,text=True).stdout.strip()
zv=[open(out+'/zversion.txt').read().strip()[3:]]
json.dump({'container':c,'image':img,'architecture':arch,'iris_build':zv[0].strip() if zv else 'see raw.log','ipm_commit':pin,
 'namespace':ns,'started':start,'exit_code':int(rc),'script_sha256':hashlib.sha256(open(out+'/script.cos','rb').read()).hexdigest(),
 'end_marker':'NATIVE-END' in raw},open(out+'/receipt.json','w'),indent=1)
PY
  echo "$out";;
state)
  docker exec -i "$C" iris session IRIS -U USER <<'COS'
write "ZV=",$zversion,!
set r=##class(%SQL.Statement).%ExecDirect(,"select Name,VersionString from %IPM_Storage.ModuleItem") while r.%Next() { write "MOD=",r.Name," ",r.VersionString,! }
set r=##class(%SQL.Statement).%ExecDirect(,"select Name,%ID,Enabled from %IPM_Repo.Definition") while r.%Next() { write "REPO=",r.Name," ",r.Enabled,! }
write "NS=" set ns="" for { set ns=$order(^%SYS("Namespace",ns)) quit:ns=""  write ns," " } write !
halt
COS
  ;;
*) echo "usage: $0 up|exec NAME SCRIPT [NS]|state"; exit 2;;
esac
