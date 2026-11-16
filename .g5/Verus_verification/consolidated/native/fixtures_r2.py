"""Round 2 fixtures: the version rules decide which dependency version is installed."""
import shutil, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c2'
shutil.rmtree(R, ignore_errors=True)
# registry content
for v in ('1.5.0', '2.5.0'): module(R/f'reg/sv-lt-dep/{v}', 'sv-lt-dep', v, desc=v)
module(R/'reg/sv-ge-dep/1.3.0', 'sv-ge-dep', '1.3.0', desc='1.3.0')
for v in ('1.5.0', '1.2.0'): module(R/f'reg/sv-ge2-dep/{v}', 'sv-ge2-dep', v, desc=v)
for tag in ('ab', 'ba'):
    for v in ('1.5.0', '2.0.0-beta.1'): module(R/f'reg/shared-{tag}/{v}', f'shared-{tag}', v, desc=v)
    module(R/f'reg/dep-a-{tag}', f'dep-a-{tag}', '1.0.0', [(f'shared-{tag}', '^1.0.0')])
    module(R/f'reg/dep-b-{tag}', f'dep-b-{tag}', '1.0.0', [(f'shared-{tag}', '<2.0.0')])
# roots
module(R/'lt', 'sv-lt-main', '1.0.0', [('sv-lt-dep', '<2')])            # operator with a partial version
module(R/'ge', 'sv-ge-main', '1.0.0', [('sv-ge-dep', '>=1.2')])         # only 1.3.0 offered
module(R/'ge2', 'sv-ge2-main', '1.0.0', [('sv-ge2-dep', '>=1.2')])      # 1.2.0 and 1.5.0 offered
module(R/'ab', 'root-ab', '1.0.0', [('dep-a-ab', '1.0.0'), ('dep-b-ab', '1.0.0')])
module(R/'ba', 'root-ba', '1.0.0', [('dep-b-ba', '1.0.0'), ('dep-a-ba', '1.0.0')])
print('fixtures written to', R)
# Same diamond, but the parent with <2.0.0 sorts first by name (dep-0-zz), so its requirement is combined second.
R2 = HERE / 'fixtures' / 'c2b'
shutil.rmtree(R2, ignore_errors=True)
for v in ('1.5.0', '2.0.0-beta.1'): module(R2/f'reg/shared-zz/{v}', 'shared-zz', v, desc=v)
module(R2/'reg/dep-0-zz', 'dep-0-zz', '1.0.0', [('shared-zz', '<2.0.0')])
module(R2/'reg/dep-1-zz', 'dep-1-zz', '1.0.0', [('shared-zz', '^1.0.0')])
module(R2/'zz', 'root-zz', '1.0.0', [('dep-1-zz', '1.0.0'), ('dep-0-zz', '1.0.0')])
print('fixtures written to', R2)
