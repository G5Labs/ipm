#!/usr/bin/env python3
"""Run paired boundary observations in a newly created disposable IRIS container.
No shared container is restarted or mutated. Each run writes a new receipt directory.
"""
from pathlib import Path
import subprocess,time,re,json,hashlib,datetime,argparse
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[4]
FIX=ROOT/'.g5/Verus_verification/consolidated/iter2/boundaries'
parser=argparse.ArgumentParser();parser.add_argument('--keep',action='store_true');args=parser.parse_args()
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
container='ipm-iter2-boundary-'+stamp.lower();out=HERE/('native-replay-'+stamp);out.mkdir()
commands=[];events=[];created=False
frozen=out/'inputs';frozen.mkdir()
for f in FIX.iterdir():
 if f.is_file():(frozen/f.name).write_bytes(f.read_bytes())
fixture_hashes={f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in frozen.iterdir()}

def run(argv,input=None,timeout=90):
 start=time.monotonic();p=subprocess.run(argv,input=input,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=timeout)
 commands.append({'argv':argv,'input':input,'returncode':p.returncode,'wall_seconds':time.monotonic()-start,'stdout':p.stdout})
 return p.stdout

def iris(s):return run(['docker','exec','-i',container,'iris','session','IRIS','-U','USER'],s+'\nhalt\n',timeout=20)
def ready():
 for _ in range(100):
  try:q=iris('write "READY=",##class(%Dictionary.ClassDefinition).%ExistsId("%IPM.General.History"),!')
  except subprocess.TimeoutExpired:q=''
  if 'READY=1' in q:return
  time.sleep(.4)
 raise RuntimeError('IRIS not ready')

def require(raw,expected):
 for s in expected:
  if s not in raw:raise AssertionError(s+' missing from '+raw)

try:
 run(['docker','run','-d','--name',container,'ipm-consolidated-iris:571e04a1-fresh','--check-caps','false']);created=True;ready()
 run(['docker','cp',str(frozen/'BoundaryProbe.cls'),container+':/tmp/BoundaryProbe.cls'])
 run(['docker','exec',container,'mkdir','-p','/tmp/iter2-cpf'])
 for f in ['valid.cpf','invalid.cpf']:run(['docker','cp',str(frozen/f),container+':/tmp/iter2-cpf/'+f])
 raw=iris('set sc=$system.OBJ.Load("/tmp/BoundaryProbe.cls","ck")\nwrite "LOADOK=",$system.Status.IsOK(sc),!');require(raw,['LOADOK=1'])
 for _ in range(2):raw+=iris('\n'.join(f'do ##class(Iter2.BoundaryProbe).{x}({v})' for x in ['DeletePair','LockPair','CachePair'] for v in [0,1]))
 require(raw,['OBS delete-0 done_before_finalize=1 row_before_finalize=0 finalize=1 persist=1 done=1 deleted=1 finalrow=0 error=', 'OBS delete-0 afterclose=0','OBS delete-1 done_before_finalize=0 row_before_finalize=1 finalize=1 persist=1 done=1 deleted=1 finalrow=0 error=','OBS delete-1 afterclose=1','OBS lock-0 saved=1 acquired_before_close=0 acquired_after_close=0 acquired_after_commit=1','OBS lock-1 saved=1 acquired_before_close=0 acquired_after_close=1 acquired_after_commit=1','OBS cache-0 status=1 acquired_after_return=1 acquired_after_commit=1','OBS cache-1 status=1 acquired_after_return=1 acquired_after_commit=1'])
 (out/'paired.log').write_text(raw);events.append({'family':'delete-lock-cache','repeats':2,'pass':True})
 for tag in ['processloss','restart']:
  p=subprocess.Popen(['docker','exec','-i',container,'iris','session','IRIS','-U','USER'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  p.stdin.write(f'do ##class(Iter2.BoundaryProbe).Hold("{tag}")\n');p.stdin.flush();pid=None
  for _ in range(60):
   q=iris(f'write "PID=",$get(^Iter2Boundary("{tag}","job")),!');m=re.search(r'PID=(\d+)',q)
   if m:pid=m[1];break
   time.sleep(.1)
  assert pid,q
  raw=iris(f'do ##class(Iter2.BoundaryProbe).Read("{tag}")')
  if tag=='processloss':run(['docker','exec','--user','root',container,'kill','-9',pid]);time.sleep(.5)
  else:run(['docker','restart','--timeout','30',container]);ready()
  raw+=iris(f'do ##class(Iter2.BoundaryProbe).Read("{tag}")')
  pairs=re.findall(r'temp=(\d) permanent=(\d)',raw)
  assert pairs==([('1','1'),('1','1')] if tag=='processloss' else [('1','1'),('0','1')]),raw
  (out/(tag+'.log')).write_text(raw);events.append({'family':tag,'observations':pairs,'pass':True})
 raw=iris('do ##class(Iter2.BoundaryProbe).CPFPair("invalid")\ndo ##class(Iter2.BoundaryProbe).CPFPair("valid")')
 require(raw,['OBS cpf-invalid status=0 before=0 during=0 afterrollback=0 dbsentinel=0','exited with 7','OBS cpf-valid status=1 before=0 during=1 afterrollback=1 dbsentinel=0'])
 (out/'cpf.log').write_text(raw);events.append({'family':'cpf-config-reader','pass':True})
 meta={'image':run(['docker','inspect','--format','{{.Image}}',container]).strip(),'runtime':iris('write $zversion,!'),'container':container,'fixture_hashes':fixture_hashes,'events':events,'pass':True,'scope':'paired native observations only, finite provider/sample; no ObjectScript proof'}
 (out/'receipt.json').write_text(json.dumps(meta,indent=2)+'\n');print(json.dumps(meta,indent=2))
finally:
 if created and not args.keep:run(['docker','rm','-f',container])
 (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
