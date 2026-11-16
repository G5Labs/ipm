"""Round 1 fixtures: temporary .modules repository lifecycle, tie-breaking across repositories, post-commit failure."""
import shutil
import sys; sys.path.insert(0, __import__("os").path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c1'
shutil.rmtree(R, ignore_errors=True)
module(R/'tr1', 'tr1-main', '1.0.0')                                           # no .modules
module(R/'tr2', 'tr2-main', '1.0.0', [('tr2-dep', '1.0.0')])
module(R/'tr2/.modules/tr2-dep', 'tr2-dep', '1.0.0', desc='from-dotmodules')
module(R/'tr3', 'tr3-main', '1.0.0', [('tr3-dep', '1.0.0')])                     # leftover name ipm-temp-modules-1
module(R/'tr3/.modules/tr3-dep', 'tr3-dep', '1.0.0', desc='from-dotmodules')
(R/'elsewhere').mkdir(parents=True, exist_ok=True)
module(R/'tr4', 'tr4-main', '1.0.0', [('tr4-dep', '1.0.0')])                     # existing repo at the same root
module(R/'tr4/.modules/tr4-dep', 'tr4-dep', '1.0.0', desc='from-dotmodules')
module(R/'tr5', 'tr5-main', '1.0.0', [('tr5-dep', '1.0.0')])                     # same version in an older registry
module(R/'tr5/.modules/tr5-dep', 'tr5-dep', '1.0.0', desc='from-dotmodules')
module(R/'reg5/tr5-dep', 'tr5-dep', '1.0.0', desc='from-registry')
module(R/'tr5b', 'tr5b-main', '1.0.0', [('tr5b-dep', '>=1.0.0')])                # higher version in .modules
module(R/'tr5b/.modules/tr5b-dep', 'tr5b-dep', '1.1.0', desc='from-dotmodules')
module(R/'reg5/tr5b-dep', 'tr5b-dep', '1.0.0', desc='from-registry')
module(R/'tr5c', 'tr5c-main', '1.0.0', [('tr5c-dep', '>=1.0.0')])                # higher version in registry
module(R/'tr5c/.modules/tr5c-dep', 'tr5c-dep', '1.0.0', desc='from-dotmodules')
module(R/'reg5/tr5c-dep', 'tr5c-dep', '1.1.0', desc='from-registry')
module(R/'tr7', 'tr7-main', '1.0.0')                                           # Create fails: invalid module name in .modules
module(R/'tr7/.modules/broken', 'bad name', '1.0.0')
module(R/'tr7b', 'tr7b-main', '1.0.0')                                         # neighbor: valid unused module in .modules
module(R/'tr7b/.modules/fine', 'tr7b-unused', '1.0.0')
print('fixtures written to', R)
