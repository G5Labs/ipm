"""Validate ontology structure, generated local links and source comparison coverage."""
from pathlib import Path
import json,re
R=Path(__file__).resolve().parents[3]
source=json.loads((Path(__file__).with_name('source-map.json')).read_text())
source_lines=set((R/source['source']).read_text().splitlines())
ids={}; checks=0
for p in (R/'.g5/ontology').rglob('*.md'):
 s=p.read_text(); body,meta=s.split('\n## Metadata\n',1) if '\n## Metadata\n' in s else (s,'')
 blocks=re.findall(r'^\[([IDQ]) · ([^\]]+)\]$',body,re.M)
 footers=re.findall(r'^### (\S+)$',meta,re.M)
 assert [id for kind,id in blocks]==footers, f'Body/footer mismatch {p}'
 for kind,id in blocks:
  assert id not in ids,f'Duplicate {id}'
  ids[id]=p
  assert body.count(f'<a id="{id}"></a>')==1,id
  entry=meta.split(f'### {id}\n',1)[1].split('\n### ',1)[0]
  assert any(tag in entry for tag in ['- human-provided','- human-verified','- agent']),id
  if kind=='Q':assert 'uncertain' in entry,id
 assert not re.search(r'^#{3,} ',body,re.M),p
# Only validate generated links, never rewrite the user's verbatim source links.
link_files=[p for base in ['.g5/ontology','.g5/uplift/ben-ontology'] for p in (R/base).rglob('*.md')]
link_files.extend(R/p for p in ['docs/ben_ontology_delta.md','docs/code_gaps.md'])
for p in link_files:
  for line in p.read_text().splitlines():
   if line in source_lines:continue
   for url in re.findall(r'\]\(([^)]+)\)',line):
    if '://' in url or url.startswith('mailto:'):continue
    path,_,anchor=url.partition('#')
    target=(p.parent/path).resolve() if path else p
    assert target.exists(),f'Missing {url} in {p}'
    line_range=re.fullmatch(r'L(\d+)(?:-L(\d+))?',anchor)
    if line_range:
     first=int(line_range[1]);last=int(line_range[2] or first)
     assert 1<=first<=last<=len(target.read_text().splitlines()),f'Invalid line range {url} in {p}'
    elif anchor:
     text=target.read_text()
     if target.is_relative_to(R/'.g5/ontology'):
      assert f'<a id="{anchor}"></a>' in text,f'Missing anchor {url} in {p}'
    checks+=1
# Every recovered commitment and unresolved question must be disclosed relative to Ben.
delta=(R/'docs/ben_ontology_delta.md').read_text()
human_ids={entry['id'] for entry in source['blocks']}
for id in ids.keys()-human_ids:
 assert delta.count(f'#{id})')==1,f'Addition must appear exactly once in comparison: {id}'
print(f'PASS: {len(ids)} blocks, {checks} generated links, complete source comparison coverage.')
