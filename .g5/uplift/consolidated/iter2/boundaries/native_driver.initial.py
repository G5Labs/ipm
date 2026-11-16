import subprocess,time,re,json,hashlib,pathlib,datetime
out=pathlib.Path('/tmp/iter2-boundary-receipts');out.mkdir(exist_ok=True)
container='ipm-iter2-boundaries'; commands=[]
def run(args, input=None, timeout=60):
 t=time.monotonic();p=subprocess.run(args,input=input,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=timeout);commands.append({'command':args,'input':input,'returncode':p.returncode,'wall_seconds':time.monotonic()-t});return p.stdout
def iris(s): return run(['docker','exec','-i',container,'iris','session','IRIS','-U','USER'],s+'\nhalt\n')
run(['docker','cp','/tmp/Iter2BoundaryProbe.cls',container+':/tmp/BoundaryProbe.cls'])
raw=iris('set ^Iter2Boundary("seq")=$get(^Iter2Boundary("seq"))+1000\nset sc=$system.OBJ.Load("/tmp/BoundaryProbe.cls","ck")\nwrite "LOADOK=",$system.Status.IsOK(sc),!')
for rep in range(2):
 raw+=iris('\n'.join(f'do ##class(Iter2.BoundaryProbe).{x}({v})' for x in ['DeletePair','LockPair','CachePair'] for v in [0,1]))
(out/'paired.log').write_text(raw)
assert 'LOADOK=1' in raw and '<' not in raw, raw
holders=[]
for tag in ['processloss2','restart2']:
 p=subprocess.Popen(['docker','exec','-i',container,'iris','session','IRIS','-U','USER'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 p.stdin.write(f'do ##class(Iter2.BoundaryProbe).Hold("{tag}")\n');p.stdin.flush();holders.append(p)
 pid=None
 for _ in range(40):
  q=iris(f'write "PID=",$get(^Iter2Boundary("{tag}","job")),!')
  m=re.search(r'PID=(\d+)',q)
  if m:pid=m[1];break
  time.sleep(.1)
 assert pid, q
 raw=iris(f'do ##class(Iter2.BoundaryProbe).Read("{tag}")')
 if tag.startswith('processloss'):
  raw+=run(['docker','exec','--user','root',container,'kill','-9',pid]);time.sleep(.4)
 else:
  raw+=run(['docker','restart','--time','30',container])
  for _ in range(90):
   try:q=iris('write "READY=",##class(%Dictionary.ClassDefinition).%ExistsId("Iter2.BoundaryProbe"),!')
   except Exception: q=''
   if 'READY=1' in q:break
   time.sleep(.5)
  assert 'READY=1' in q,q
 raw+=iris(f'do ##class(Iter2.BoundaryProbe).Read("{tag}")')
 (out/(tag+'.log')).write_text(raw)
commands.append({'note':'Native crash/restart only disposable container; no shared service touched.'})
(out/'commands.json').write_text(json.dumps(commands,indent=2))
print(raw)
print((out/'paired.log').read_text())
