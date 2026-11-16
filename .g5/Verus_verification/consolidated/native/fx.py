"""Fixture writer for consolidated native cases. Writes module.xml trees under fixtures/<round>/ on the host;
iris.sh copies them into the container at /tmp/<round>/ before a run."""
import os
from pathlib import Path
HERE = Path(__file__).resolve().parent

def module(path, name, version, deps=(), desc='', extra=''):
    """deps: list of (name, version_expression or '')."""
    p = Path(path); p.mkdir(parents=True, exist_ok=True)
    esc = lambda t: t.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')
    d = ''
    if deps:
        refs = ''.join(f"        <ModuleReference>\n          <Name>{n}</Name>\n" + (f"          <Version>{esc(v)}</Version>\n" if v else '') + "        </ModuleReference>\n" for n, v in deps)
        d = f"      <Dependencies>\n{refs}      </Dependencies>\n"
    desc_el = f"      <Description>{desc}</Description>\n" if desc else ''
    (p / 'module.xml').write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n<Export generator="IRIS" version="26">\n'
        f'  <Document name="{name}.ZPM">\n    <Module>\n      <Name>{name}</Name>\n      <Version>{version}</Version>\n'
        f'{desc_el}      <Packaging>module</Packaging>\n{d}{extra}    </Module>\n  </Document>\n</Export>\n')
    return p
