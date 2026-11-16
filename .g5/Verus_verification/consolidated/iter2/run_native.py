#!/usr/bin/env python3
"""Replay owned loopback/provider and reader fixtures; write only iter2 receipts."""
from pathlib import Path
import subprocess, json, hashlib, datetime
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter2/native'
CONTAINER='ipm-consolidated-iris'
def run(args, **kw):
    p=subprocess.run(args,text=True,capture_output=True,timeout=180,**kw)
    if p.returncode: raise RuntimeError((args,p.stdout,p.stderr))
    return p
def docker(*args, **kw): return run(['docker','exec',*args],**kw)
def main():
    OUT.mkdir(parents=True,exist_ok=True)
    docker(CONTAINER,'mkdir','-p','/tmp/g5-iter2')
    for name in ['provider.py','Probe.cls']:
        run(['docker','cp',str(HERE/name),CONTAINER+':/tmp/g5-iter2/'+name])
    subprocess.run(['docker','exec',CONTAINER,'pkill','-f','^python3 /tmp/g5-iter2/provider.py$'],capture_output=True)
    try:
        docker(CONTAINER,'python3','-c',"import urllib.request; assert urllib.request.urlopen('http://127.0.0.1:18291/v2/').status == 200")
    except RuntimeError:
        docker('-d',CONTAINER,'python3','/tmp/g5-iter2/provider.py')
    docker('-i',CONTAINER,'sh','-c','cat > /tmp/g5-iter2/requirements.txt',input='  # comment\n  package==1 # inline\n\n')
    for name,obj in {'objects-a':{'pkg':{'arr':[{'x':1}]}},'objects-b':{'pkg':{'arr':[{'y':2}]}},'primitives-a':{'pkg':{'arr':[1,2]}},'primitives-b':{'pkg':{'arr':[3]}}}.items():
        docker('-i',CONTAINER,'sh','-c','cat > /tmp/g5-iter2/'+name+'.json',input=json.dumps(obj))
    script='''write "ZV=",$zversion,!
do $system.OBJ.Load("/tmp/g5-iter2/Probe.cls","ck")
do ##class(Consolidated.Iter2.Probe).Registry()
do ##class(Consolidated.Iter2.Probe).Cache()
do ##class(Consolidated.Iter2.Probe).Readers()
do ##class(Consolidated.Iter2.Probe).Environment()
write "ITER2-NATIVE-END",!
halt
'''
    p=docker('-i',CONTAINER,'iris','session','IRIS','-U','USER',input=script)
    raw=p.stdout+p.stderr
    (OUT/'raw.log').write_text(raw); (OUT/'script.cos').write_text(script)
    expected=['OBS TAG-g5iter2order value=1.0.0','OBS TAG-g5iter2sorted value=2.0.0',
        'OBS HASH-g5iter2first ok=0','OBS HASH-g5iter2skip ok=1','OBS HASH-g5iter2match ok=1',
        'OBS CACHE-REMOTE before=1 save=1','OBS CACHE-REMOTE disabled=1 save=1',
        'OBS CACHE-ORAS before=1 save=1','OBS CACHE-ORAS disabled=0 save=1',
        'OBS AUTH-accepted rejected=0','OBS AUTH-rejected rejected=1','OBS AUTH-unknown rejected=0',
        'OBS REPORT-METHOD methods=1 failed=1 assertions=0','OBS REPORT-CASE methods=2 failed=1 assertions=0',
        'OBS REPORT-FILE status=1 suites=1 summary=0','OBS REQUIREMENTS count=1 line=package==1 # inline',
        'OBS ENV-objects value=[{"x":1,"y":2}]','OBS ENV-primitives value=[3,2]',
        'OBS CYCLE a=${a} b=${a}','OBS CHAIN a=ok b=ok','ITER2-NATIVE-END']
    checks=[{'expected':x,'observed':x in raw} for x in expected]
    receipt={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'container':CONTAINER,
        'command':['docker','exec','-i',CONTAINER,'iris','session','IRIS','-U','USER'],
        'inputs_sha256':{str(x.relative_to(ROOT)):hashlib.sha256(x.read_bytes()).hexdigest() for x in [Path(__file__),HERE/'Probe.cls',HERE/'provider.py']},
        'raw_sha256':hashlib.sha256(raw.encode()).hexdigest(),'checks':checks,
        'scope':'Actual compiled pinned source, owned loopback registry responses and seeded native reader rows; no remote registry integrity generalization.'}
    (OUT/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({'checks':len(checks),'passed':sum(x['observed'] for x in checks)}))
    assert all(x['observed'] for x in checks),OUT/'raw.log'
if __name__=='__main__':main()
