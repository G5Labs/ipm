#!/usr/bin/env python3
"""Residual distribution of production members without a resolved Verus locator, from a project map.
Usage: residual.py MAP.json OUT.json"""
import json, csv, sys, collections
from pathlib import Path
R = Path(__file__).resolve().parents[4]
d = json.load(open(sys.argv[1])); linked = set(d['resolved_Verus_locator_members'])
census = list(csv.DictReader(open(R / '.g5/uplift/consolidated/coverage/members.tsv'), delimiter='\t'))
excl_files = {'Utils/FileBinaryTar.cls': 'archive', 'Utils/Build.cls': 'verify/build namespace', 'Lifecycle/StudioProject/XDataArchive.cls': 'archive', 'Lifecycle/StudioProject/XDataArchive/IFile.cls': 'archive', 'General/Archive.cls': 'archive'}
excl_prefix = {'Repo/Oras/': 'external registry', 'Repo/Remote/': 'external registry', 'Repo/Http/': 'external registry', 'Repo/IPublishService.cls': 'publish'}
excl_members = {'Utils/Module.cls::LoadModuleFromArchive': 'archive', 'Lifecycle/Base.cls::%Package': 'package', 'Lifecycle/Base.cls::Packaging': 'package', 'Lifecycle/Base.cls::GetBaseClassForPackaging': 'package', 'Lifecycle/Base.cls::OnBeforeArtifact': 'package', 'Lifecycle/Base.cls::OnInstallFromPayload': 'archive', 'Lifecycle/Base.cls::DetectPipCaller': 'python/network', 'Lifecycle/Base.cls::DetectFlexiblePythonCaller': 'python/network', 'Lifecycle/Base.cls::ResolvePipCaller': 'python/network', 'Lifecycle/Base.cls::ExportPythonDependencies': 'package', 'Lifecycle/Base.cls::VersionedPythonName': 'python/network', 'Lifecycle/Base.cls::FindExecutableInDirs': 'python/network', 'Main.cls::Unpublish': 'external registry', 'Main.cls::GetPythonInstalledLibs': 'python', 'Main.cls::GetPythonLibrariesList': 'python', 'Main.cls::RunDependencyAnalyzer': 'external package', 'Main.cls::GetUpstreamPackageVersions': 'external registry', 'Storage/Module.cls::HaveToDeploy': 'deployed/package', 'Storage/Module.cls::GetKnownDependencies': 'dependency analyzer', 'Storage/Module.cls::GetNamespaceConfig': 'deployed/package'}
cnt = collections.Counter(); sub = collections.defaultdict(collections.Counter); short = collections.Counter()
for r in census:
    m = r['member']; f = m.split('::')[0]; n = int(r['end']) - int(r['start']) + 1
    if m in linked: cnt['linked'] += 1; continue
    reason = excl_members.get(m) or excl_files.get(f) or next((v for k, v in excl_prefix.items() if f.startswith(k)), None)
    if reason: cnt['excluded:' + reason] += 1; continue
    cnt['unlinked in scope'] += 1; sub[f.split('/')[0] if '/' in f else f][f] += 1
    if n <= 6: short['<=6 lines'] += 1
    elif n <= 20: short['7-20 lines'] += 1
    else: short['>20 lines'] += 1
out = {'map': sys.argv[1], 'totals': dict(cnt), 'in_scope_by_length': dict(short),
       'in_scope_by_subsystem': {k: {'total': sum(v.values()), 'files': dict(v.most_common())} for k, v in sorted(sub.items(), key=lambda x: -sum(x[1].values()))}}
Path(sys.argv[2]).write_text(json.dumps(out, indent=1) + '\n')
print(json.dumps({'totals': out['totals'], 'by_length': out['in_scope_by_length'], 'top': [(k, v['total']) for k, v in list(out['in_scope_by_subsystem'].items())[:10]]}))
