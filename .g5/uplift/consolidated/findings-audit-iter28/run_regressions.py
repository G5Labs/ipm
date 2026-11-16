#!/usr/bin/env python3
"""Pinned IPM findings regressions. Only disposable, network-isolated containers.

Exit 0 means the recorded as-built discrepancies and their neighbors were observed;
it does not mean that the product meets the intended contracts. No archive tests.
"""
from pathlib import Path
import argparse, datetime, hashlib, importlib.util, io, json, re, subprocess, sys, tarfile, time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
NATIVE = ROOT / '.g5/Verus_verification/consolidated'
IMAGE = 'ipm-consolidated-iris:571e04a1-fresh'
PIN = '571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
PINNED_IMAGE_ID = 'sha256:8db58b93cf76245f6cc9cfc16aaa9627391535eab6350134ecc1bbf8bcf8b82f'

def run(group):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    out = HERE / 'runs' / (stamp + '-' + group)
    out.mkdir(parents=True)
    container = 'ipm-findings-audit-' + group + '-' + stamp.lower()
    commands = []
    def call(args, stdin=None, timeout=180, check=True):
        p = subprocess.run(args, input=stdin, capture_output=True, timeout=timeout)
        commands.append({'argv':args,'stdin':stdin.decode() if stdin else None,'exit':p.returncode})
        if check and p.returncode:
            raise RuntimeError((args, p.stderr.decode(errors='backslashreplace')))
        return p
    try:
        call(['docker','run','-d','--network','none','--name',container,IMAGE])
        for _ in range(80):
            p=call(['docker','exec','-i',container,'iris','session','IRIS','-U','USER'],b'write "READY",!\nhalt\n',check=False)
            if p.returncode == 0 and b'READY' in p.stdout: break
            time.sleep(.5)
        else: raise RuntimeError('IRIS not ready')
        image_id=call(['docker','inspect',container,'--format','{{.Image}}']).stdout.decode().strip()
        assert image_id==PINNED_IMAGE_ID,('retagged image',image_id)
        # Compare source bytes to the product pin, independently of live edits.
        archive=call(['git','-C',str(ROOT),'archive',PIN,'src','preload','modules','tests']).stdout
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            expected={m.name:hashlib.sha256(tar.extractfile(m).read()).hexdigest() for m in tar if m.isfile()}
        script='import json,hashlib;from pathlib import Path;r=Path("/home/irisowner/zpm");ps='+repr(list(expected))+';print(json.dumps({p:hashlib.sha256((r/p).read_bytes()).hexdigest() if (r/p).is_file() else None for p in ps}))'
        actual=json.loads(call(['docker','exec',container,'python3','-c',script]).stdout)
        mismatches=[p for p in expected if expected[p]!=actual[p]]
        (out/'source-pin.json').write_text(json.dumps({'pin':PIN,'image_id':image_id,'files':len(expected),'mismatches':mismatches},indent=2)+'\n')
        assert not mismatches,mismatches[:10]
        def copy(src,dest):
            call(['docker','cp',str(src),container+':'+dest])
        cfg=None
        if group.startswith('iter'):
            home=NATIVE/group
            cfg=json.loads((home/'cases.json').read_text())
            copy(home/'native','/tmp/'+group)
            session=cfg['session']
            if group=='iter11': session=[session[1],session[3]]
        elif group=='early':
            home=NATIVE/'native'
            for name in ['c1','c1e','c2','c2b','c3','c4','c5','c5h']:
                copy(home/'fixtures'/name,'/tmp/'+name)
            copy(home/'harness/Obs.cls','/tmp/Obs.cls')
            session=['do $system.OBJ.Load("/tmp/Obs.cls","ck-d")']
            for name in ['r1-temp-repo','r1-eval-postcommit','r1-evaluator','r2-semver','r2-semver-addendum','r2-resolve','r2-resolve-order','r3-delete','r3-purge','r4-owners','r5-env','r5-env-b','r5-heldback']:
                s=(home/'scripts'/(name+'.cos')).read_text()
                session += [line for line in s.splitlines() if line.strip()!='halt']
        else:
            spec=json.loads((HERE/'jobs.json').read_text())[group]
            call(['docker','exec','-u','root',container,'mkdir','-p','/tmp/uplift'])
            if spec.get('directories'):call(['docker','exec','-u','root',container,'mkdir','-p']+spec['directories'])
            for src,dest in spec['copies']: copy(ROOT/src,dest)
            if spec.get('directories'):call(['docker','exec','-u','root',container,'chmod','-R','a+rwX']+spec['directories'])
            session=spec['session']
        call(['docker','exec','-u','root',container,'chmod','-R','a+rwX','/tmp'])
        payload=('\n'.join(session+['write !,"AUDIT-END",!','halt'])+'\n').encode()
        (out/'session.cos').write_bytes(payload)
        p=call(['docker','exec','-e','CONS_DOLLAR=a$1b','-e',r'CONS_BS=a\b','-e','CONS_DOLLAR9=a$9b','-i',container,'iris','session','IRIS','-U','USER'],payload,timeout=300)
        (out/'stdout.bin').write_bytes(p.stdout);(out/'stderr.bin').write_bytes(p.stderr)
        raw=(p.stdout+p.stderr).decode('utf-8','backslashreplace')
        (out/'session.log').write_text(raw)
        assert 'AUDIT-END' in raw
        obs={}
        for line in raw.splitlines():
            if line.startswith('OBS|'):
                _,key,val=line.split('|',2)
                if re.fullmatch(r'-?\d+\s*',val): obs[key]=int(val)
        if cfg:
            sys.path.insert(0,str(home))
            spec=importlib.util.spec_from_file_location('audit_harness',home/'harness.py')
            module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
            obs.update(module.output_observations(raw,cfg) if hasattr(module,"output_observations") else {})
            wanted=cfg['native_expected']
            if group=='iter11':
                wanted={k:v for k,v in wanted.items() if k.startswith('delete-')}
                wanted.update({'delete-direct-status':0,'delete-direct-retained':1,'delete-locked-uninstall':1,'delete-locked-module':0,'delete-locked-manifest':0,'delete-locked-code':1,'delete-unlocked-uninstall':1,'delete-unlocked-code':0})
            assert wanted
            failures={k:{'expected':v,'actual':obs.get(k)} for k,v in wanted.items() if obs.get(k)!=v}
            assert not failures,failures
            gates=len(wanted)
        else:
            checks=json.loads((HERE/'assertions.json').read_text())[group]
            failures=[x for x in checks if not re.search(x['pattern'],raw,re.M)]
            assert not failures,failures
            gates=len(checks)
        result={'group':group,'pin':PIN,'image_id':image_id,'network':'none','gates':gates,'passed':True,'observations':obs,'commands':commands,'stdout_sha256':hashlib.sha256(p.stdout).hexdigest(),'provider':'real IRIS Community 2026.1 ARM64; local filesystem repositories; extension fixtures explicitly named in review'}
        (out/'receipt.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps({'group':group,'passed':True,'gates':gates,'run':str(out.relative_to(ROOT))}),flush=True)
    except Exception as e:
        (out/'failure.json').write_text(json.dumps({'group':group,'error':repr(e),'commands':commands},indent=2)+'\n')
        raise
    finally:
        subprocess.run(['docker','rm','-f',container],capture_output=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('groups',nargs='+');args=parser.parse_args()
    for group in args.groups: run(group)
