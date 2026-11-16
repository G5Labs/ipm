#!/usr/bin/env python3
"""Replay isolated held-out fixtures against an existing IPM container; never reset it."""
from pathlib import Path
import datetime,hashlib,json,subprocess,sys,time
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
EXT=ROOT/'.g5/uplift/consolidated/extension';FX=EXT/'independent-fixtures'
CONTAINER=sys.argv[1] if len(sys.argv)>1 else 'ipm-consolidated-iris'
def call(args,**kw):return subprocess.run(args,text=True,capture_output=True,check=True,timeout=180,**kw)
def docker(*args,**kwargs):return call(['docker','exec',*args],**kwargs)
# The directory at a file path intentionally makes the real lockfile write fail.
(FX/'LOCKFAIL'/'module-lock.json').mkdir(exist_ok=True)
docker('-u','root',CONTAINER,'mkdir','-p','/tmp/g5-independent-thin')
for name in ['IMMEDIATE','DEFERRED','REPORT','LOCKFAIL','LOCKOK']:
 # These names and this directory are owned exclusively by these native fixtures.
 docker('-u','root',CONTAINER,'rm','-rf','/tmp/g5-independent-thin/'+name)
 call(['docker','cp',str(FX/name),CONTAINER+':/tmp/g5-independent-thin/'+name])
call(['docker','cp',str(FX/'server.py'),CONTAINER+':/tmp/g5-independent-thin/server.py'])
docker('-u','root',CONTAINER,'chown','-R','irisowner','/tmp/g5-independent-thin')
probe='''import urllib.request,urllib.error
for p in [18171,18172,18173,18174]:
 try:r=urllib.request.urlopen('http://127.0.0.1:'+str(p)+'/v2/',timeout=2)
 except urllib.error.HTTPError as e:r=e
 print(p,r.code,bool(r.headers.get('WWW-Authenticate')))
'''
expected='18171 200 False\n18172 401 True\n18173 401 False\n18174 403 False\n'
try:ready=docker(CONTAINER,'python3','-c',probe).stdout==expected
except (subprocess.CalledProcessError,subprocess.TimeoutExpired):ready=False
if not ready:
 docker('-d',CONTAINER,'python3','/tmp/g5-independent-thin/server.py')
 for _ in range(20):
  time.sleep(.1)
  try:
   if docker(CONTAINER,'python3','-c',probe).stdout==expected:break
  except subprocess.CalledProcessError:pass
 else:raise RuntimeError('Loopback fixture unavailable; inspect port conflict, do not replace unknown service')
helper=HERE/'thin_probe.cls';call(['docker','cp',str(helper),CONTAINER+':/tmp/G5IndependentThin.cls'])
script='''write "ZV=",$zversion,!
do $system.OBJ.Load("/tmp/G5IndependentThin.cls","ck")
do ##class(Consolidated.IndependentThin).Run()
do ##class(Consolidated.IndependentThin).HistoryConcurrent()
do ##class(Consolidated.IndependentThin).Inspection()
do ##class(Consolidated.IndependentThin).LockWriteConsumers()
write "INDEPENDENT-REPLAY-END",!
halt
'''
out=EXT/('independent-native-replay-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ'));out.mkdir()
r=docker('-i',CONTAINER,'iris','session','IRIS','-U','USER',input=script)
raw=r.stdout+r.stderr;(out/'script.cos').write_text(script);(out/'raw.log').write_text(raw)
inputs=[Path(__file__),helper,*FX.rglob('*')]
for p in inputs:
 if p.is_file():
  dest=out/'inputs'/p.relative_to(ROOT);dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(p.read_bytes())
receipt={'command':['docker','exec','-i',CONTAINER,'iris','session','IRIS','-U','USER'],'container':CONTAINER,'product':'571e04a1','inputs_sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs if p.is_file()},'script_sha256':hashlib.sha256(script.encode()).hexdigest(),'fixture_directory':'LOCKFAIL/module-lock.json must be directory','environment':'loopback fixtures ports18171..18174; ARM64 IRIS build expected in raw ZV','end_marker':'INDEPENDENT-REPLAY-END' in raw}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
assert 'Load finished successfully.' in raw and receipt['end_marker'],out
for observation in ['OBS F-YES status=1 target=1 db=0','OBS F-MISSING status=0 target=0 db=0','OBS IMMEDIATE status=0 module=0 copied=1','OBS DEFERRED status=0 module=0 copied=0','OBS ORAS-18172 available=1','OBS ORAS-18173 available=0','OBS REPORT-COMPOSE status=0 file=1','OBS REPORT-NEIGHBOR status=1 file=1','OBS H-CONCURRENT finalize=1 persist=1 afterclose=0','OBS INSPECT-G5-INDEPENDENT-REPORT-ALIAS status=1','OBS LOCKFAIL status=1 file=0 directory=1','OBS LOCKOK status=1 file=1 directory=0']:
 assert observation in raw,(out,observation)
assert '"methods":{"total":1,"passed":1' in raw,('actual passing test missing',out)
print(out.relative_to(ROOT))
