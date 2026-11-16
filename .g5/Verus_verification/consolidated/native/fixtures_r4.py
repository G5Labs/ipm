"""Round 4 fixtures: module-version; a filesystem repository refreshed under a lock held by another process."""
import shutil, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c4'
shutil.rmtree(R, ignore_errors=True)
module(R/'mv-a', 'mv-a', '1.2.3')
module(R/'reglock/lk-a', 'lk-a', '1.0.0')
module(R/'reglock/lk-b', 'lk-b', '1.0.0')
(R/'reglock/lk-garbage').mkdir(parents=True, exist_ok=True)
(R/'reglock/lk-garbage/module.xml').write_text('<Export><Document name="lk-garbage.ZPM"><Module><Name>lk-garbage</Name><Version>1.0.0</Version><Packaging>module</Packaging></Module></Document>')
print('fixtures written to', R)
