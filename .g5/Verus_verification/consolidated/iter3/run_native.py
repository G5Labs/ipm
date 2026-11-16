#!/usr/bin/env python3
"""Use only the owned ipm-iter3-iris instance; retain commands, failures and raw observations."""
from pathlib import Path
import datetime,hashlib,json,subprocess,time,sys,os
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=ROOT/'.g5/uplift/consolidated/iter3/native';OUT.mkdir(parents=True,exist_ok=True)
C=os.environ.get('IPM_ITER3_CONTAINER','ipm-iter3-iris')
COMMANDS=[]
def run(args,**kw):
    start=time.monotonic();p=subprocess.run(args,capture_output=True,text=True,timeout=180,**kw)
    row={'command':args,'exit':p.returncode,'seconds':time.monotonic()-start}
    if 'input' in kw:
        data=kw['input'];path=OUT/'stdin'/('command-'+str(len(COMMANDS)+1)+'.txt');path.parent.mkdir(exist_ok=True)
        path.write_text(data);row['stdin']=str(path.relative_to(ROOT));row['stdin_sha256']=hashlib.sha256(data.encode()).hexdigest()
    COMMANDS.append(row)
    if p.returncode:raise RuntimeError((args,p.stdout,p.stderr))
    return p
def main():
    probe=subprocess.run(['docker','inspect',C],capture_output=True,text=True)
    if probe.returncode:run(['docker','run','-d','--name',C,'ipm-consolidated-iris:571e04a1-fresh'])
    elif not json.loads(probe.stdout)[0]['State']['Running']:run(['docker','start',C])
    for attempt in range(40):
        ready=subprocess.run(['docker','exec','-i',C,'iris','session','IRIS','-U','USER'],input='write "I3-READY",!\nhalt\n',capture_output=True,text=True)
        if ready.returncode==0 and 'I3-READY' in ready.stdout:break
        time.sleep(0.5)
    else:raise RuntimeError('Owned IRIS instance did not become ready within the bounded startup wait')
    run(['docker','exec',C,'mkdir','-p','/tmp/g5-iter3'])
    for p in HERE.glob('*.cls'):run(['docker','cp',str(p),C+':/tmp/g5-iter3/'+p.name])
    provider=HERE/'provider.py'
    desired=hashlib.sha256(provider.read_bytes()).hexdigest()
    prior=subprocess.run(['docker','exec',C,'cat','/tmp/g5-iter3/provider-running.sha256'],capture_output=True,text=True)
    live=subprocess.run(['docker','exec',C,'pgrep','-f','^python3 /tmp/g5-iter3/provider.py$'],capture_output=True,text=True)
    if prior.stdout.strip()!=desired or live.returncode!=0:
        old=subprocess.run(['docker','exec',C,'pgrep','-f','^python3 /tmp/g5-iter3/provider.py$'],capture_output=True,text=True)
        for pid in old.stdout.split():run(['docker','exec',C,'kill',pid])
        run(['docker','cp',str(provider),C+':/tmp/g5-iter3/provider.py'])
        exists=subprocess.run(['docker','exec',C,'test','-f','/tmp/g5-iter3/cert.pem'],capture_output=True)
        if exists.returncode:
            run(['docker','exec',C,'openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout','/tmp/g5-iter3/key.pem','-out','/tmp/g5-iter3/cert.pem','-days','1','-subj','/CN=localhost'])
        run(['docker','exec','-d',C,'python3','/tmp/g5-iter3/provider.py'])
    pin_file=OUT/'source-pin.json'
    if pin_file.exists():expected=json.loads(pin_file.read_text())['source_sha256']
    else:
        pin='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
        paths=subprocess.check_output(['git','ls-tree','-r','--name-only',pin,'--','src','preload','modules'],cwd=ROOT,text=True).splitlines()
        expected={p:hashlib.sha256(subprocess.check_output(['git','show',pin+':'+p],cwd=ROOT)).hexdigest() for p in paths}
    source_script='import hashlib,json; from pathlib import Path; root=Path("/home/irisowner/zpm"); wanted='+repr(list(expected))+'; print(json.dumps({p:hashlib.sha256((root/p).read_bytes()).hexdigest() if (root/p).is_file() else None for p in wanted}))'
    actual=json.loads(run(['docker','exec',C,'python3','-c',source_script]).stdout)
    mismatches=[p for p,v in expected.items() if actual[p]!=v]
    pin_file.write_text(json.dumps({'product_pin':'571e04a19aba9f9b4bc0ea23fbe09182038f4e55','container':C,'paths':len(expected),'source_sha256':expected,'container_sha256':actual,'mismatches':mismatches,'scope':'On-disk product source identity, not a proof of compiled ObjectScript correspondence.'},indent=2)+'\n')
    assert not mismatches,mismatches
    before_requests=subprocess.run(['docker','exec',C,'cat','/tmp/g5-iter3/requests.jsonl'],capture_output=True,text=True).stdout.splitlines()
    fixtures='''from pathlib import Path
import gzip
p=Path('/tmp/g5-iter3')
(p/'not-gzip.tgz').write_bytes(b'invalid archive')
(p/'bad-checksum.tgz').write_bytes(gzip.compress(b'a'+b'\\x00'*511+b'\\x00'*1024))
for sub in ['module/assets','activated','export/assets','cache','doc','repo/pkg','project']: (p/sub).mkdir(parents=True,exist_ok=True)
(p/'repo/pkg/module.xml').write_text('<Export generator="IRIS" version="26"><Document name="g5-iter3-reposearch.ZPM"><Module><Name>g5-iter3-reposearch</Name><Version>1.0.0</Version><Packaging>module</Packaging></Module></Document></Export>')
for sub,content in {'module/assets/new.txt':'new','activated/old.txt':'old','export/assets/old.txt':'old','export/sibling.txt':'keep','cache/payload.tgz':'old'}.items(): (p/sub).write_text(content)
'''
    run(['docker','exec','-i',C,'python3','-'],input=fixtures)
    lines=['write "ZV=",$zversion,!',
        'set doc=##class(%Dictionary.CompiledMethod).%OpenId("%Library.File||CopyDir") write !,"PROVIDER|CopyDir|",doc.FormalSpec,!,doc.Description,!',
        'set doc=##class(%Dictionary.CompiledMethod).%OpenId("%Studio.Project||Check") write !,"PROVIDER|ProjectCheck|",doc.Description,!']
    for name in ['Defaults','Requirements','FailDocument','Probe']:lines.append(f'set loadsc=$system.OBJ.Load("/tmp/g5-iter3/{name}.cls","ck") write !,"LOAD|{name}|",$system.Status.IsOK(loadsc),!')
    for name in ['ArtifactSelection','ArtifactHTTP','ArtifactLifecycle','Defaults','Requirements','Public','ExportFailure','DeploymentAndSearch','ProjectReaders','ArchiveBoundary']:
        lines.append(f'do ##class(Consolidated.Iter3.Probe).{name}()')
    lines+=['write !,"ITER3-NATIVE-END",!','halt'];script='\n'.join(lines)+'\n'
    (OUT/'script.cos').write_text(script)
    p=run(['docker','exec','-i',C,'iris','session','IRIS','-U','USER'],input=script)
    raw=p.stdout+p.stderr
    branch_specs=[('missing-dev',1,{},'1','0'),('missing-normal',0,{},'0','0'),
        ('auth-failed-dev',1,{'ARTI_SERVER':'localhost','ARTI_PORT':'18433','ARTI_AQL_URL':'/aql-secure','ARTI_TOKEN':'fixture-token'},'0','0'),
        ('auth-success',0,{'ARTI_SERVER':'localhost','ARTI_PORT':'18433','ARTI_AQL_URL':'/aql-public','ARTI_TOKEN':'fixture-token'},'1','1'),
        ('anonymous-extract-dev',1,{'ARTI_SERVER':'localhost','ARTI_PORT':'18433'},'1','1'),('anonymous-extract-normal',0,{'ARTI_SERVER':'localhost','ARTI_PORT':'18433'},'1','1')]
    branch_expected={}
    for label,dev,env,ok,directory in branch_specs:
        unique=label+'-'+str(time.time_ns())
        call=f'do ##class(Consolidated.Iter3.Probe).ArtifactBranch("{unique}",{dev})\nhalt\n'
        cmd=['docker','exec']
        for key,value in env.items():cmd+=['-e',key+'='+value]
        cmd+=['-i',C,'iris','session','IRIS','-U','USER']
        b=run(cmd,input=call);raw+=b.stdout+b.stderr
        branch_expected['branch-'+unique]=ok;branch_expected['branch-dir-'+unique]=directory
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    (OUT/(stamp+'.log')).write_text(raw);(OUT/'raw.log').write_text(raw)
    obs={};compiled={}
    for line in raw.splitlines():
        if line.startswith('LOAD|'):
            _,key,val=line.split('|',2);compiled[key]=val
        if line.startswith('OBS|'):
            _,key,val=line.split('|',2);assert key not in obs,key;obs[key]=val
    expected={'filter-1':'1','filter-2':'0','filter-3':'0','filter-4':'0','selection':'artifactory/r/a3','tie':'artifactory/r/a1','wild-1':'1','wild-2':'0','wild-3':'1','wild-4':'0',
        'metadata-path':'artifactory/secure/./payload.tgz','secure-bytes':'0','aql-first':'artifactory/public/./older.tgz','aql-second':'artifactory/public/./newer.tgz','cached-bytes':'old',
        'activate-local':'1','activate-old':'1','activate-new':'1','export-local':'1','export-handled':'1','export-old':'1','export-sibling':'1',
        'default-mode':'2','default-other':'3','default-explicit-empty':'1','default-unmatched-other':'0',
        'health-unset':'1','health-false':'0','health-implies-interop':'0','disabled-allows-enabled':'1','requires-sys':'0','requires-sys-in-sys':'1',
        'invoke-default':'0','invoke-unchecked':'1','invoke-checked':'0','setting-no-overwrite':'first','setting-empty-allows-write':'third','setting-first':'first','format-case':'JsOn','format-invalid':'0','format-retained':'JsOn',
        'test-prefix':'1','owns-sibling':'0','owns-child':'1','suite-empty-shadows':'1','case-alone':'0','class-ignores-case':'1',
        'deployed-hook-error':'0','export-overwrites-error':'1','export-source-file':'1','deploy-parameter-false':'0','deploy-resource-true':'1','deploy-module-default':'0','deploy-deployedmodule-default':'1','fs-save':'1','fs-cache':'1','fs-name-count':'1','fs-keyword-count':'0','project-empty-check':'1','project-invalid-check':'1','project-before-check':'1','project-after-check':'0','project-module-save':'1','project-install-error':'0','project-deployed-on-error':'1','project-deployed-stored-on-error':'1','project-failed-in-installer':'1','project-duplicate-save-error':'1','project-duplicate-object':'1','project-duplicate-store':'0','project-marker-on-error':'0','gzip-bad-read-length':'0','gzip-bad-read-status':'0','archive-bad-gzip':'1','archive-bad-checksum':'0'}
    expected.update(branch_expected)
    checks=[{'key':k,'expected':v,'actual':obs.get(k),'pass':obs.get(k)==v} for k,v in expected.items()]
    requests='\n'.join(run(['docker','exec',C,'cat','/tmp/g5-iter3/requests.jsonl']).stdout.splitlines()[len(before_requests):])+'\n'
    (OUT/'requests.jsonl').write_text(requests)
    r={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'container':C,'raw_sha256':hashlib.sha256(raw.encode()).hexdigest(),'commands':COMMANDS,'observations':obs,'checks':checks,'fixture_compilation':compiled,'end_marker':'ITER3-NATIVE-END' in raw,
       'input_sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__),HERE/'provider.py',*sorted(HERE.glob('*.cls'))]},
       'scope':'Unmodified pinned methods with loopback TLS provider, native resource objects, controlled environment subclass and deployed-hook error override. Final packaged archive and production provider behavior are not inferred.'}
    (OUT/'receipt.json').write_text(json.dumps(r,indent=2)+'\n')
    (OUT/(stamp+'.receipt.json')).write_text(json.dumps(r,indent=2)+'\n')
    print(json.dumps({'passed':sum(x['pass'] for x in checks),'total':len(checks),'missing_or_different':[x for x in checks if not x['pass']]}))
    assert r['end_marker'] and all(x['pass'] for x in checks) and len(compiled)==4 and all(v=='1' for v in compiled.values()),OUT/'raw.log'
if __name__=='__main__':main()
