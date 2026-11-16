"""Round 1b fixtures: after-install message evaluated after the final commit; dependency-level .modules failure."""
import shutil, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c1e'
shutil.rmtree(R, ignore_errors=True)
def msg(m): return f'      <AfterInstallMessage>{m}</AfterInstallMessage>\n'
module(R/'p1', 'pc-ok', '1.0.0', extra=msg('Done {$namespace} {name}'))
for mode in ('txn', 'notxn', 'dev'):
    module(R/f'p2-{mode}', f'pc-bad-{mode}', '1.0.0', extra=msg('Done {x} $$$ThrowOnError'))
module(R/'p3', 'pc-neighbor', '1.0.0', extra=msg('Done {x} $$$OK'))
module(R/'p4', 'pc-nobrace', '1.0.0', extra=msg('Done $$$OK'))
for mode in ('txn', 'dev'):
    module(R/f'd-{mode}', f'dd-parent-{mode}', '1.0.0', [(f'dd-dep-{mode}', '1.0.0')])
    module(R/f'regd/dd-dep-{mode}', f'dd-dep-{mode}', '1.0.0')
    module(R/f'regd/dd-dep-{mode}/.modules/broken', 'bad name', '1.0.0')
module(R/'d-neighbor', 'dd-parent-ok', '1.0.0', [('dd-dep-ok', '1.0.0')])
module(R/'regd/dd-dep-ok', 'dd-dep-ok', '1.0.0')
print('fixtures written to', R)
