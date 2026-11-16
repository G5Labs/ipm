#!/usr/bin/env python3
"""Observe absent versus explicit-false inherited flags in the shipped load consumer."""
from pathlib import Path
import subprocess,json,hashlib,datetime,re,xml.etree.ElementTree as ET,sys
HERE=Path(__file__).resolve().parent
REPO=HERE.parents[4]
C='ipm-consolidated-iris'
REMOTE='/tmp/g5flagoverride20261114'
PIN='571e04a19aba9f9b4bc0ea23fbe09182038f4e55'
CASES=[('JA','NoJournal',False,True),('J0','NoJournal',True,True),('TA','NoTransaction',False,True),('T0','NoTransaction',True,True),('JS','NoJournal',False,False),('TS','NoTransaction',False,False)]
def cmd(args,**kw):
    p=subprocess.run(args,capture_output=True,text=True,timeout=180,**kw)
    if p.returncode:raise RuntimeError((args,p.stdout,p.stderr))
    return p

def manifest(name,inner):
    return f'<Export generator="IRIS" version="26"><Document name="{name}.ZPM"><Module><Name>{name}</Name><Version>1.0.0</Version><Packaging>module</Packaging>{inner}</Module></Document></Export>\n'
for case,flag,zero,fail in CASES:
    root=f'g5flagx20261114-{case.lower()}-root';child=f'g5flagx20261114-{case.lower()}-dep'
    folder=HERE/'fixtures'/case;dep=folder/'.modules'/child;dep.mkdir(parents=True,exist_ok=True)
    inner=f'<Defaults><Parameter Name="{flag}">1</Parameter></Defaults><Invoke Class="G5FlagOverride20261114.Probe" Method="Child" Phase="Activate" CheckStatus="1"><Arg>{case}</Arg></Invoke>'
    (dep/'module.xml').write_text(manifest(child,inner))
    inner=f'<Dependencies><ModuleReference><Name>{child}</Name><Version>1.0.0</Version></ModuleReference></Dependencies><Invoke Class="G5FlagOverride20261114.Probe" Method="Parent" Phase="Activate" CheckStatus="1"><Arg>{case}</Arg><Arg>{int(fail)}</Arg></Invoke>'
    (folder/'module.xml').write_text(manifest(root,inner))
    for p in [folder/'module.xml',dep/'module.xml']:ET.fromstring(p.read_text())
# Preflight refuses own names or helper collisions; it never removes foreign work.
pre='write "HELPER=",##class(%Dictionary.ClassDefinition).%ExistsId("G5FlagOverride20261114.Probe"),!\nwrite "GLOBAL=",$data(^G5FlagOverride20261114),!\n'
for case,*_ in CASES:
    for who in ['root','dep']:pre+=f'write "MOD=",##class(%IPM.Storage.Module).NameExists("g5flagx20261114-{case.lower()}-{who}"),!\n'
pre+='halt\n'
p=cmd(['docker','exec','-i',C,'iris','session','IRIS','-U','USER'],input=pre)
assert 'HELPER=0' in p.stdout and 'GLOBAL=0' in p.stdout and p.stdout.count('MOD=0')==12,p.stdout
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ');out=HERE/'results'/stamp;out.mkdir(parents=True)
(out/'preflight.cos').write_text(pre);(out/'preflight.log').write_text(p.stdout+p.stderr)
cmd(['docker','exec','-u','root',C,'mkdir','-p',REMOTE])
cmd(['docker','cp',str(HERE/'fixtures')+'/.',C+':'+REMOTE+'/fixtures'])
cmd(['docker','cp',str(HERE/'Probe.cls'),C+':'+REMOTE+'/Probe.cls'])
script=f'do $system.OBJ.Load("{REMOTE}/Probe.cls","ck-d")\ndo ##class(G5FlagOverride20261114.Probe).Snapshot("before")\n'
for case,flag,zero,fail in CASES:
    root=f'g5flagx20261114-{case.lower()}-root';child=f'g5flagx20261114-{case.lower()}-dep'
    script+=f'do ##class(G5FlagOverride20261114.Probe).Run("{case}","{REMOTE}/fixtures/{case}/","{flag}",{int(zero)},"{root}","{child}")\n'
script+='do ##class(G5FlagOverride20261114.Probe).Snapshot("observed")\n'
for case,*_ in CASES:script+=f'do ##class(G5FlagOverride20261114.Probe).Cleanup("g5flagx20261114-{case.lower()}-root","g5flagx20261114-{case.lower()}-dep")\n'
script+='kill ^G5FlagOverride20261114\ndo ##class(G5FlagOverride20261114.Probe).Snapshot("after")\nwrite "ZV=",$zversion,!,"NATIVE-END",!\nhalt\n'
(HERE/'script.cos').write_text(script);(out/'script.cos').write_text(script)
p=cmd(['docker','exec','-i',C,'iris','session','IRIS','-U','USER'],input=script)
raw=p.stdout+p.stderr;(out/'raw.log').write_text(raw);print(raw)
assert 'NATIVE-END' in raw
sources=['src/cls/IPM/Utils/Module.cls','src/cls/IPM/Storage/Module.cls','src/cls/IPM/Storage/ModuleSetting/Parameter.cls','src/cls/IPM/Utils/JournalManager.cls','src/cls/IPM/Storage/InvokeReference.cls','src/cls/IPM/Main.cls']
receipt={'product_pin':PIN,'command':['docker','exec','-i',C,'iris','session','IRIS','-U','USER'],'started':stamp,'container':C,'image':cmd(['docker','inspect','-f','{{.Image}}',C]).stdout.strip(),'architecture':cmd(['docker','exec',C,'uname','-m']).stdout.strip(),'iris_build':re.search(r'^ZV=(.*)$',raw,re.M)[1],'exit_code':p.returncode,'end_marker':True,'namespace':'USER','helper':'G5FlagOverride20261114.Probe, compiled before commands','journal_observer':"'($zboolean($zjob,1024,1)=1024), read only; no JournalManager object created",'python':sys.version,'inputs':{str(p.relative_to(REPO)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [HERE/'run.py',HERE/'Probe.cls',HERE/'script.cos']+list((HERE/'fixtures').rglob('module.xml'))+[REPO/n for n in sources]}}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
obs={}
for case,rest in re.findall(r'^OBS (\S+) (.*)$',raw,re.M):
    for key,val in re.findall(r'(\S+)=([^ ]+)',rest):obs[(case,key)]=val
expected={'JA':(1,0,1,1),'J0':(2,1,0,0),'TA':(1,1,0,0),'T0':(2,1,0,0),'JS':(1,0,1,1),'TS':(1,1,1,1)}
errors=[]
for case,flag,zero,fail in CASES:
    tl,j,child,write=expected[case]
    want={'entry.tlevel':'0','exit.tlevel':'0','entry.journal':'1','exit.journal':'1','before.root':'0','before.child':'0','status':'ERR' if fail else 'OK','child.tlevel':str(tl),'child.journal':str(j),'parent.tlevel':'1','parent.journal':'1','stored.root':str(int(not fail)),'stored.child':str(child),'stored.childwrite':str(write),'stored.parentwrite':str(int(not fail))}
    for k,v in want.items():
        if obs.get((case,k))!=v:errors.append({'case':case,'field':k,'expected':v,'actual':obs.get((case,k))})
before=set(re.findall(r'^SNAP before (.*)$',raw,re.M));after=set(re.findall(r'^SNAP after (.*)$',raw,re.M))
preserve={'foreign_rows_before':len(before),'foreign_rows_after':len(after),'removed':sorted(before-after),'added':sorted(after-before),'same':before==after}
checks={'cases':{case:{k:v for (c,k),v in obs.items() if c==case} for case,*_ in CASES},'unexpected':errors,'preservation':preserve}
(out/'checks.json').write_text(json.dumps(checks,indent=2)+'\n');(HERE/'latest.json').write_text(json.dumps({'result':str(out.relative_to(HERE))},indent=2)+'\n')
print(json.dumps(checks,indent=2))
assert not errors,errors
assert preserve['same'],preserve
