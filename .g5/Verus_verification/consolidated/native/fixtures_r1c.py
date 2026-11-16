"""Round 1c fixtures: two class resources, the second fails to compile (partial compile), per mode; valid neighbor."""
import shutil, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from fx import HERE, module
R = HERE / 'fixtures' / 'c1r'
shutil.rmtree(R, ignore_errors=True)
def two(dirname, name, pkg, bad):
    extra = f'      <SourcesRoot>src</SourcesRoot>\n      <Resource Name="{pkg}.A.CLS"/>\n      <Resource Name="{pkg}.B.CLS"/>\n'
    p = module(R/dirname, name, '1.0.0', extra=extra)
    src = p / 'src' / pkg; src.mkdir(parents=True, exist_ok=True)
    (src/'A.cls').write_text(f'Class {pkg}.A\n{{\n\nClassMethod Ok() As %Integer\n{{\n    quit 1\n}}\n\n}}\n')
    body = '    set x = \n' if bad else '    quit 2\n'
    (src/'B.cls').write_text(f'Class {pkg}.B\n{{\n\nClassMethod Bad() As %Integer\n{{\n{body}}}\n\n}}\n')
for mode in ('txn', 'notxn', 'dev'):
    two(f'two-{mode}', f'tworec-{mode}', f'TwoRes{mode.capitalize()}', True)
two('two-ok', 'tworec-ok', 'TwoResOk', False)
print('fixtures written to', R)
