#!/usr/bin/env python3
"""Replay inherited proofs/controls in an extraction, leaving every inherited live receipt unchanged."""
from pathlib import Path
import hashlib,json,resource,shutil,subprocess,sys,tarfile,tempfile,time,argparse
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3]
OUT=HERE/'inherited-replay';TOOL=Path.home()/'.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus'
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--resume',action='store_true');args=parser.parse_args()
    OUT.mkdir(exist_ok=True)
    archive=ROOT.parent/'results/experiments/uplift-measurement-20261114/ipm-iter3/baseline/actual-before.tar.gz'
    previous=json.loads((OUT/'receipt.json').read_text()) if args.resume and (OUT/'receipt.json').exists() else {}
    attempts=previous.get('failed_attempts',[])+[x for x in previous.get('checks',[]) if not x['pass']]
    rows=[x for x in previous.get('checks',[]) if x['pass']];skipped=previous.get('skipped_links',[])
    if previous:assert previous['archive_sha256']==hashlib.sha256(archive.read_bytes()).hexdigest()
    def run(name,cmd,cwd,expected='pass'):
        if args.resume and any(x['name']==name and x['pass'] for x in rows):
            print(name,'REUSED unchanged archive',flush=True);return
        before=resource.getrusage(resource.RUSAGE_CHILDREN);start=time.monotonic()
        p=subprocess.run(list(map(str,cmd)),cwd=cwd,capture_output=True,text=True,timeout=600)
        after=resource.getrusage(resource.RUSAGE_CHILDREN);raw=p.stdout+p.stderr
        (OUT/(name+'.log')).write_text(raw)
        passed=p.returncode==0 if expected=='pass' else p.returncode!=0 and ('postcondition not satisfied' in raw or 'assertion failed' in raw or 'expression simplifies to false' in raw) and 'resource limit' not in raw
        row={'name':name,'command':list(map(str,cmd)),'exit':p.returncode,'expected':expected,'pass':passed,'wall_seconds':time.monotonic()-start,'child_user_seconds':after.ru_utime-before.ru_utime,'child_system_seconds':after.ru_stime-before.ru_stime,'peak_child_rss_bytes_cumulative_max':after.ru_maxrss}
        rows.append(row);(OUT/'receipt.json').write_text(json.dumps({'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'checks':rows,'failed_attempts':attempts,'skipped_links':skipped,'optional_incomplete':'optional-full-playbook-timeout.json' if (OUT/'optional-full-playbook-timeout.json').exists() else None,'scope':'Unchanged prior models, complete consolidated controls, original P baselines and G kernel controls; G wrapper omits only the Rust explorer harness. Full unchanged P mutant campaign is not claimed; a bounded attempt is separately incomplete. No full ObjectScript or production native suite replay.'},indent=2)+'\n')
        print(name,'PASS' if passed else 'FAIL',round(row['wall_seconds'],2),flush=True)
        if not passed:raise AssertionError((name,raw[-2500:]))
    with tempfile.TemporaryDirectory(prefix='ipm-iter3-inherited-') as td:
        temp=Path(td).resolve()
        def filt(m,dest):
            if m.issym() or m.islnk():skipped.append(m.name);return None
            return tarfile.data_filter(m,dest)
        with tarfile.open(archive) as tf:tf.extractall(temp,filter=filt)
        work=temp/'ipm';v=work/'.g5/Verus_verification/consolidated/verus'
        for name in ['check_env_attr.py','semver_check.py','owners_check.py','check_parent.py','check_thin_surfaces.py','check_flag_overrides.py']:
            run(name,[sys.executable,v/name],work)
        run('iter2-models',[sys.executable,work/'.g5/Verus_verification/consolidated/iter2/run_models.py'],work)
        run('iter2-boundaries',[sys.executable,work/'.g5/uplift/consolidated/iter2/boundaries/check.py'],work)
        for p in sorted((work/'playbook-uplift/models/verus/controls').glob('*.json')):
            run('P-baseline-'+p.stem,[TOOL,p.parent.parent/(p.stem+'.rs'),'--rlimit','30','--triggers-mode','silent'],work)
        g=work/'.g5/Verus_verification/crates/ipm-load-history/src'
        wrapper=g/'iter3_kernel.rs'
        wrapper.write_text('pub mod model;\npub mod contract;\npub mod cases;\npub mod proofs;\nfn main() {}\n')
        run('G-kernel-baseline',[TOOL,wrapper,'--num-threads','4','--triggers-mode','silent'],work)
        for i in range(1,14):
            feature=f'control-{i:02d}'
            run('G-'+feature,[TOOL,wrapper,'--cfg',f'feature="{feature}"','--num-threads','4','--triggers-mode','silent'],work,'pass' if i==5 else 'assertion-rejected')
        run('G-sanity-false',[TOOL,wrapper,'--cfg','feature="sanity-false"','--num-threads','4','--triggers-mode','silent'],work,'assertion-rejected')
        prior=work/'playbook-uplift/models/verus/results'
        shutil.copytree(prior,OUT/'preserved-prior-playbook-results',dirs_exist_ok=True)
        (OUT/'preserved-prior-playbook-results/PROVENANCE.txt').write_text('Unchanged prior control receipts preserved from the actual before archive. The eight P baseline commands do not regenerate these control receipts. The optional full-control replay is incomplete.\n')
        for rel in ['.g5/uplift/consolidated/iter2/proofs','.g5/uplift/consolidated/iter2/boundaries/proofs','.g5/uplift/consolidated/extension/parent-checks','.g5/Verus_verification/consolidated/verus/results']:
            src=work/rel
            if src.exists():shutil.copytree(src,OUT/'generated'/rel,dirs_exist_ok=True)
    print('Inherited checks',len(rows),'passed',flush=True)
if __name__=='__main__':main()
