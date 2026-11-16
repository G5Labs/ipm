"""Round 3 fixtures: install, uninstall and a refused load whose history records are read back."""
import shutil, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c3'
shutil.rmtree(R, ignore_errors=True)
module(R/'reg/h-inst', 'h-inst', '1.0.0')
module(R/'h-load-2', 'h-load', '2.0.0')
module(R/'h-load-1', 'h-load', '1.0.0')
print('fixtures written to', R)
