from pathlib import Path
import sys,tempfile,shutil,re,json
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];VV=ROOT/'.g5/Verus_verification/consolidated'
sys.path.insert(0,str(HERE.parent/'coverage'));import count
# Retain both old measurement universes under their original all-fields family withdrawal rule.
raw=(ROOT.parent/'results/experiments/ipm-uplift-comparison-20261113/consolidation-probe.log').read_text()
probe={}
for case in ['OBJECT','PRIMITIVE']:
 m=re.search(r'^'+case+r' (\{.*\})',raw,re.M);assert m,case;probe[case]=m[1]
orig_parse=count.framework.parse_obs
def extended_parse(run,tags=None):
 d=orig_parse(run,tags)
 if run=='20261114T033831Z-r5-env':
  for c,v in probe.items():d[(c,'json')]=v
 return d
summary={}
with tempfile.TemporaryDirectory(prefix='ipm-count-') as td:
 d=Path(td);md=d/'models';shutil.copytree(VV/'models',md);(d/'native').symlink_to(VV/'native')
 old=(HERE/'before-models/env_config.py').read_text();(md/'env_config.py').write_text(old)
 before=count.compute(md,extra=False);summary['original_session']={'inherited':before['B_size'],'additional':len(before['new']),'total':before['B_size']+len(before['new']),'denominator':before['denominator']}
 (md/'env_config.py').write_text(old+'''\n_base_predict=predict\ndef predict(variant):\n out=_base_predict(variant)\n for c,a,b in [('OBJECT',{'a':[{'x':1}]},{'a':[{'y':2}]}),('PRIMITIVE',{'a':[1,2]},{'a':[3]})]:\n  merge(a,b);out[(c,'json')]=json.dumps(a,separators=(',',':'))\n return out\n''')
 count.framework.parse_obs=extended_parse
 after=count.compute(md,extra=False)
 lost=sorted(set(before['new'])-set(after['new']))
 summary['original_extended']={'inherited':after['B_size'],'additional':len(after['new']),'total':after['B_size']+len(after['new']),'denominator':after['denominator'],'withheld':lost,'probe':probe}
 count.framework.parse_obs=orig_parse
assert summary['original_session']['total']==149,summary
assert summary['original_extended']['total']==141,summary
(HERE/'before-measurements.json').write_text(json.dumps(summary,indent=2))
print(json.dumps(summary,indent=1))
