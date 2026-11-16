#!/usr/bin/env python3
"""Rebuild the findings audit from saved starting files, never product sources."""
from pathlib import Path

if (Path(__file__).resolve().parents[4] / ".g5/Verus_verification/results/viewer-index.jsonl").exists():
    raise SystemExit("Historical register generator. Use the sealed iteration 28 revision to reproduce the audit.")

from collections import Counter
import hashlib, json, os, re

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
BASE=HERE/'sealed'
def read(p): return (BASE/p).read_text()
def anchor(s): return re.sub(r'[^a-z0-9-]','-',s.lower()).strip('-')
def rel(p,frm): return os.path.relpath(p,frm.parent)
def link(label,p,frm,frag=''): return f'[{label}]({rel(p,frm)}'+('#'+frag if frag else '')+')'
def move_links(text,source,dest):
 def sub(m):
  label,url=m.group(1),m.group(2)
  if re.match(r'[a-z]+:',url) or url.startswith('/') or ' ' in url:return m[0]
  part,mark,frag=url.partition('#')
  path=source if not part else (source.parent/part).resolve()
  return '['+label+']('+rel(path,dest)+('#'+frag if mark else '')+')'
 return re.sub(r'\[([^\]\n]+)\]\(([^)]+)\)',sub,text)
def write(p,text): p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text.rstrip()+'\n')

FILES={'C':'.g5/uplift/consolidated/BUGS.md','G':'.g5/Verus_verification/bugs_issues.md','P':'playbook-uplift/BUGS.md'}
bugs={}
for line in read(FILES['C']).splitlines():
 m=re.match(r'\| (C:D\d+)\b.*? \|',line)
 if m and m[1] not in bugs:
  fields=line.strip('|').split(' | ')
  bugs[m[1]]={'id':m[1],'arm':'C','title':fields[2].strip(),'original':line,'severity':fields[1].strip().split()[0]}
g=read(FILES['G'])
for m in re.finditer(r'^\| (D\d+|I\d+|OBS-[LH]\d+|OOS-\d+)(?: \(remainder\))? \| (supported|conditional|issue|suspected|withdrawn|refuted|out of scope) \| (.*?) \|',g,re.M):
 id='G:'+m[1]
 if id in bugs: continue
 section=re.search(r'^### '+re.escape(m[1])+r'(?:\b| \().*?(?=^### |^## |\Z)',g,re.M|re.S)
 bugs[id]={'id':id,'arm':'G','title':m[3],'original':section[0] if section else m[0],'old_status':m[2],'severity':'Medium'}
p=read(FILES['P'])
for id in sorted(set(re.findall(r'\bF-(?:LOAD|HIST|INST|DEP|UNIN|REPO|NSP|RES|SYNC)-\d+\b',p))):
 candidates=list(re.finditer(r'^\| '+re.escape(id)+r'(?: †)? \|[^\n]*',p,re.M))
 row=next((m for m in candidates if len(m[0].strip('|').split(' | '))>=5),None)
 if row:
  fields=row[0].strip('|').split(' | ');title=fields[2];severity=fields[1].replace('*','').strip().split()[0]
 else:
  section=re.search(r'(?:\*\*'+re.escape(id)+r'[^\n]*|[^\n]*\*\*'+re.escape(id)+r'[^\n]*)',p)
  title=section[0] if section else id;severity='Low'
 bugs['P:'+id]={'id':'P:'+id,'arm':'P','title':title.strip(),'original':row[0] if row else title,'severity':severity}
assert len(bugs)==165,len(bugs)

# Aliases require the same trigger and reader consequence. Related failures remain distinct.
ALIASES={
 'P:F-HIST-1':'G:D1','P:F-HIST-5':'G:D9','P:F-HIST-6':'G:D3','P:F-HIST-9':'G:D6',
 'G:D5':'P:F-REPO-4','G:D19':'P:F-REPO-5','G:I7':'P:F-DEP-1','G:OBS-L3':'C:D16',
 'G:OBS-L4':'P:F-INST-5','G:OBS-L5':'P:F-INST-10','G:OBS-L9':'P:F-INST-1',
 'G:OBS-L17':'P:F-UNIN-6','G:D30':'P:F-UNIN-2','G:D31':'P:F-INST-9',
 'P:F-NSP-12':'G:D4','G:I1':'C:D12','C:D25':'P:F-UNIN-1',
 'P:F-RES-1':'C:D19','P:F-HIST-3':'G:D25','P:F-HIST-10':'C:Q7',
}
SECURITY={
 'C:D17','C:D18','C:D20','C:D22','C:D23','P:F-REPO-1','P:F-REPO-2','P:F-REPO-7',
 'P:F-REPO-10','P:F-REPO-11','P:F-UNIN-3','P:F-UNIN-8','G:OOS-1',
}
# These ordinary witnesses would traverse the archive work explicitly excluded by the owner.
EXCLUDED={'G:D15','G:D16','G:OBS-L16','P:F-LOAD-3','P:F-RES-7','G:OOS-2'}
DOCS={'P:F-HIST-4','P:F-INST-2','P:F-INST-3','P:F-INST-11','P:F-DEP-4','P:F-DEP-5','P:F-DEP-6','P:F-REPO-3','P:F-NSP-3','P:F-RES-4','P:F-RES-5','P:F-RES-6','P:F-SYNC-2'}
FRESH={
 'C:D2':('early','TR7 missing-row cleanup warning; TR1 succeeds'),
 'C:D3':('params','cyclic Default reaches ordinary load error; acyclic neighbor succeeds'),
 'C:D4':('params','literal dollar Default makes load fail; five succeeds'),
 'C:D5':('params','a.b Default changes {$axb} in the command output; exact ordinary value neighbor'),
 'C:D6':('early','ordinary dependency load installs 2.5.0 for <2; valid dependency neighbor Dok'),
 'C:D8':('early','named diamond load refusal with source-selected dependency bounds; successful neighbor'),
 'C:D9':('params','ordinary module load stores the malformed parsed build version; ordinary 1.0.0 neighbor'),
 'C:D12':('misc','history find displays ERROR #00 for an unfinished record; completed history neighbor'),
 'C:D13':('early','module-version unchanged refusal; patch bump succeeds'),
 'C:D14':('params','ordinary -env load feeds altered dollar value to its real Invoke reader; five neighbor'),
 'C:D24':('params','$zpm(version) throws after command; Shell(version) returns success'),
 'C:D27':('error','supported error-returning resource extension; uninstall deletes metadata while class remains callable; successful extension neighbor'),
 'C:D28':('iter11','real class edit lock; uninstall succeeds with callable class retained; unlocked uninstall removes it'),
 'C:D29':('misc','ordinary FileCopy Overlay=0 retains unrelated sentinel and writes new payload; real Unix filesystem provider'),
 'C:D31':('iter17','actual outer/child Shell test runs, final file/device identities and alone neighbors'),
 'C:D32':('iter21','ordinary explicit-version install reads changed directory bytes; stable/rebuilt/refusal neighbors'),
 'C:D33':('iter24','default snapshot reinstall refuses; shallow/release/explicit-permission neighbors'),
 'G:D1':('early','dependency row absent after parent rollback; successful load neighbor'),
 'G:D2':('early','history delete removes three seeded identities despite unmatched Version_Major; Package filter preserves nonmatches'),
 'G:D3':('early','ordinary list triggers retention; old row survives with marker today; retention-off neighbor'),
 'G:D4':('nsp','SYSNamespace refusal retains developer module; ordinary-mode refusal leaves none; policy remains open'),
 'G:D9':('misc','ordinary downgrade refusal followed by history details; named successful history neighbor'),
 'G:D24':('misc','misspelled history column returns OK; valid Success query neighbor'),
 'P:F-LOAD-4':('early','ordinary after-install macro failure with durable module/history committed; successful message neighbor'),
 'P:F-INST-1':('inst','real competing update lock; command OK with old step rows; unlocked retry runs due steps'),
 'P:F-INST-4':('inst','two filesystem repositories with tied versions; creation-order selection and reordered neighbor'),
 'P:F-INST-5':('inst','ordinary update -dev from filesystem registry ends normal; local -path developer neighbor'),
 'P:F-INST-6':('inst','real V3Helper fixture suppresses V3 execution; V2/V9/V10 ordinary order neighbor'),
 'P:F-INST-7':('inst','real Validator fixture makes update fail; numbered-class neighbor'),
 'P:F-INST-8':('inst','developer fresh install leaves zero step records; later update executes old steps; normal fresh install seeds'),
 'P:F-INST-9':('inst','ordinary parent install replaces developer dependency; initial developer load verifies prestate'),
 'P:F-INST-10':('inst','new dependency installed during update runs V1; fresh normal install seeds without running'),
 'P:F-DEP-1':('inst','PermitDowngrade relaxes resolution then load refuses; normal load bound refusal neighbor'),
 'P:F-DEP-2':('inst','ordinary create-lockfile emits name order while load output is dependency first; downstream ci is unimplemented'),
 'P:F-DEP-3':('inst','list-dependents lowercase displays parent; mixed case displays none'),
 'P:F-DEP-7':('inst','ordinary solvable graph install refuses; pinned solution installs all modules'),
 'P:F-DEP-8':('inst','failed create-lockfile install leaves JSON while module/dependency rows disappear; successful lockfile neighbor'),
 'P:F-DEP-9':('inst','ordinary local-loaded dependency produces lockfile warning and no changed file; registry supplier neighbor'),
 'P:F-UNIN-1':('repo','blocked uninstall deletes actual test web application; module remains; ordinary global-resource uninstall neighbor'),
 'P:F-UNIN-2':('iter27','ordinary persistent sentinel survives -purge and schema removal/reload; real extent-delete and installer callback neighbors'),
 'P:F-UNIN-4':('repo','dependent graph error lets uninstall remove required class; resolvable dependent neighbor blocks'),
 'P:F-UNIN-5':('repo','ordinary name-only global resource retains data; explicit Global attribute deletes it'),
 'P:F-UNIN-6':('repo','blocked uninstall returns OK and logs success with module retained; successful ordinary uninstall neighbor'),
 'P:F-REPO-4':('repo','config delete restores first stored value; recorded changed-value prestate'),
 'P:F-REPO-5':('repo','real namespaces read shared default modifiers; local namespace settings neighbor'),
 'P:F-REPO-8':('repo','ordinary config invalid key/value returns OK; valid config neighbor'),
 'P:F-NSP-2':('nsp','ordinary requirement load fails in mapped namespace, succeeds in source namespace'),
 'P:F-NSP-4':('nsp','enable -map -repos skips first repo mapping; same command second run maps it'),
 'P:F-NSP-6':('nsp','unmap removes package mapping but routine mapping remains and IPM still callable; source refusal neighbor'),
 'P:F-SYNC-1':('nsp','ordinary failed compile then exact-byte revert makes sync skip stale server definition; successful initial load neighbor'),
 'P:F-SYNC-3':('nsp','ordinary reload of normal module succeeds without reloading edited file; root-bearing developer load neighbor'),
}
FRESH['P:F-UNIN-9']=('repo','ordinary recursive uninstall removes dependency code/module but retains its manifest; top manifest is removed')
FRESH.update({
 'G:D13':('early','ordinary directory load hits failed temporary repository creation after commit; module remains; valid .modules neighbor'),
 'G:D6':('returns','ordinary returned-error Activate Invoke leaves last phase OPEN; successful C01 phase is OK; intent remains Q-14'),
 'G:D11':('returns','NoJournal manifest refusal leaves the imported ZPM document at 2.0.0; successful NoJournal neighbor; broader cases remain in starting evidence'),
 'G:D12':('returns','no-module-definition refusal under NoTransaction keeps no module and reports committed=1; ordinary successful NoJournal neighbor'),
 'G:D25':('returns','supported caller transaction rollback removes the row/module; the ordinary no-caller successful return retains a finalized row; policy remains Q-05')
})
FRESH.update({
 'G:D21':('nested','ordinary parent failure leaves NoJournal middle dependency while its own child is gone; successful NESTS neighbor'),
 'G:D22':('nested','same parent failure records committed=0 while middle dependency survives; successful parent neighbor'),
 'G:D28':('nested','own NoTransaction dependency still rolls back with parent; successful Q23S neighbor'),
 'G:I3':('nested','ordinary parent load upgrades developer dependency and clears developer mode; same-version neighbor retains it')
})
FRESH.update({
 'G:D20':('readers','public GlobalRecords repeats USER history and misses seeded other namespace; local Records returns exactly one correct row'),
 'G:D29':('readers','ordinary update/reinstall/uninstall argument refusals allocate no history ID; successful/failed named load neighbors elsewhere; intent remains Q-09')
})
FRESH.update({
 'C:D16':('lock','first ordinary load -lock writes valid audit-lock JSON in process cwd instead of source root; Root is populated by return, so the final record is not evidence of its earlier availability to the writer; developer-mode neighbor writes at source'),
 'G:D18':('readers','stored NoTransaction modifier makes ordinary history find hide seeded row while returning OK; deleting default restores the same row'),
 'G:D23':('readers','ordinary global delete with unknown SQL column reports OK and retains sentinel; valid Package-filtered global delete removes it; nonpermission-error policy remains Q-27')
})
PARTIAL={
 'C:D7':('early','real range parser witness and valid caret neighbors; ordinary resolver caller with ^1.2 not re-executed'),
 'C:D10':('early','real AND helper error text; full conflicting-dependency command message path remains'),
 'C:D21':('params','real private converter exposed by subclass and valid inside-path neighbor; sync routing reachability remains untraced'),
 'C:D19':(None,'existing allowed deployed-helper override shows local status suppression; no ordinary default-helper failure/full package entry established; no new package/archive run'),
}

def owner(id):
 mapping={
 'C:D2':('features/load.g5.md','load.dependencies.local-repo'),'C:D3':('concepts/modules.g5.md','module.parameter'),
 'C:D4':('concepts/modules.g5.md','module.parameter'),'C:D5':('concepts/modules.g5.md','module.parameter'),
 'C:D6':('concepts/versions.g5.md','version.range'),'C:D7':('concepts/versions.g5.md','version.range'),
 'C:D8':('concepts/versions.g5.md','version.range'),'C:D9':('concepts/versions.g5.md','version'),
 'C:D10':('concepts/versions.g5.md','version.range'),'C:D12':('features/history.g5.md','history.record'),
 'C:D13':('features/developer-tools.g5.md','devtools.authoring.module-version'),
 'C:D14':('concepts/modules.g5.md','module.env'),'C:D15':('features/load.g5.md','load.refusal.downgrade'),
 'C:D16':('features/dependencies.g5.md','deps.lockfile'),'C:D17':('features/artifact-download.g5.md','artifact.authentication'),
 'C:D18':('features/artifact-download.g5.md','artifact.archive-admission'),'C:D19':('features/build-and-publish.g5.md','build.package.deployed-status'),
 'C:D20':('concepts/repositories.g5.md','repo.remote.download.credentials'),'C:D21':('features/developer-tools.g5.md','devtools.sync.test-paths'),
 'C:D22':('concepts/repositories.g5.md','repo.remote.download.integrity'),'C:D23':('concepts/modules.g5.md','module.webapp.validation'),
 'C:D24':('root.g5.md','ipm.shell.forms'),'C:D26':('concepts/modules.g5.md','module.webapp.upgrade'),
 'C:D27':('features/uninstall.g5.md','uninstall.child-error'),'C:D28':('features/uninstall.g5.md','uninstall.class-delete-error'),
 'C:D29':('concepts/modules.g5.md','module.processor.file-copy.unix-overlay'),'C:D30':('concepts/repositories.g5.md','repo.oras.build-readers'),
 'C:D31':('features/build-and-publish.g5.md','build.test-output.run-identity'),'C:D32':('concepts/repositories.g5.md','repo.install.version-request'),
 'C:D33':('concepts/versions.g5.md','version.range.snapshot-intersection'),
 'G:D1':('features/history.g5.md','history.survives-rollback'),'G:D2':('features/history.g5.md','history.read.filter'),
 'G:D3':('features/history.g5.md','history.retention'),'G:D4':('features/load.g5.md','load.refusal.requirements.timing'),
 'G:D6':('features/history.g5.md','history.phases.returned-errors'),'G:D9':('features/history.g5.md','history.fields'),
 'G:D24':('features/history.g5.md','history.read.filter.invalid-column'),'G:D25':('features/history.g5.md','history.committed.nested'),
 'G:D20':('features/history.g5.md','history.read.api'),
 'G:D29':('features/history.g5.md','history.record.unlogged-refusals'),
 'G:D18':('features/history.g5.md','history.read.filter.default-modifiers'),
 'G:D23':('features/history.g5.md','history.delete.global-errors'),
 'G:D21':('concepts/transactions-and-dev-mode.g5.md','transaction.flag-sources.dependency-closure'),
 'G:D22':('features/history.g5.md','history.committed.escaped-dependency'),
 'G:D28':('concepts/transactions-and-dev-mode.g5.md','transaction.flag-sources.dependency-no-transaction'),
 'G:I3':('concepts/transactions-and-dev-mode.g5.md','dev-mode.overwrite-guard.dependency-policy')}

 mapping.update({
 'P:F-REPO-4':('concepts/settings.g5.md','settings'),
 'P:F-REPO-5':('concepts/settings.g5.md','settings.default-modifiers'),
 'P:F-REPO-8':('concepts/settings.g5.md','settings.print'),
 'P:F-INST-1':('features/update.g5.md','update.steps.busy-policy'),
 'P:F-INST-4':('concepts/repositories.g5.md','repo.search-order'),
 'P:F-INST-5':('features/update.g5.md','update.dev-mode'),
 'P:F-INST-8':('features/update.g5.md','update.steps.dev-seeding-policy'),
 'P:F-INST-9':('concepts/transactions-and-dev-mode.g5.md','dev-mode.overwrite-guard.dependency-policy'),
 'P:F-INST-10':('features/update.g5.md','update.steps.fresh-seeding-policy'),
 'P:F-DEP-2':('features/dependencies.g5.md','deps.lockfile'),
 'P:F-DEP-3':('features/inspection.g5.md','inspect.dependents'),
 'P:F-DEP-8':('features/dependencies.g5.md','deps.lockfile'),
 'P:F-DEP-9':('features/dependencies.g5.md','deps.lockfile.local-supplier-policy'),
 'P:F-UNIN-1':('features/uninstall.g5.md','uninstall.dependents.as-built'),
 'P:F-UNIN-2':('features/uninstall.g5.md','uninstall.purge.as-built'),
 'P:F-UNIN-4':('features/uninstall.g5.md','uninstall.dependents'),
 'P:F-UNIN-6':('features/uninstall.g5.md','uninstall.dependents'),
 'P:F-UNIN-9':('features/uninstall.g5.md','uninstall.recurse'),
 'P:F-NSP-2':('concepts/namespaces.g5.md','namespace.version.mapped-reader'),
 'P:F-NSP-4':('concepts/namespaces.g5.md','namespace.enable.mapping-results'),
 'P:F-NSP-6':('concepts/namespaces.g5.md','namespace.unmap.routine-result'),
 'P:F-SYNC-1':('features/developer-tools.g5.md','devtools.sync.failed-import-policy'),
 'P:F-SYNC-3':('concepts/lifecycle.g5.md','lifecycle.empty-reload-policy')})
 if id in mapping:return mapping[id]
 if id.startswith('G:'):return ('features/history.g5.md','history')
 family=id.split('-')[1] if id.startswith('P:') else ''
 return {'LOAD':('features/load.g5.md','load'),'HIST':('features/history.g5.md','history'),
 'INST':('features/update.g5.md','update.steps'),'DEP':('features/dependencies.g5.md','deps'),
 'UNIN':('features/uninstall.g5.md','uninstall'),'REPO':('concepts/repositories.g5.md','repo'),
 'NSP':('concepts/namespaces.g5.md','namespace'),'RES':('concepts/lifecycle.g5.md','lifecycle'),
 'SYNC':('features/developer-tools.g5.md','devtools.sync')}.get(family,('root.g5.md','ipm'))

runmap={}
for p in sorted((HERE/'runs').glob('*/receipt.json')):
 result=json.loads(p.read_text());runmap[result['group']]=(p,result)
assert set(v[0] for v in FRESH.values()) <= set(runmap)

inactive_reasons={
 'C:D16':'Existing lock-location entry and native artifacts checked. Current lockfile cohort uses registry install, which has a Root, and cannot establish the first plain-load empty-Root caller. Do not relabel that neighbor as reproduction; existing extension witness remains historical.',
 'C:D15':'Fresh early HB2 accepts 1.0.0-rc over 2.0.0-beta; release downgrade neighbor refuses. The row is restored below as a tested contract violation.',
 'C:D26':'Observed dropped declaration is narrower than update, and the source does not settle upgrade resource ownership. Keep current facts under module.webapp.upgrade; request policy at module.webapp.upgrade.policy. No accepted cleanup obligation inferred.',
 'C:D30':'Existing ORAS service/install ordering evidence was read. A new tag-to-installed-byte witness would enter excluded archive delivery; no new ORAS install/packaging run was attempted.',
 'G:D17':'Source and earlier witness establish acceptance of relative paths. They do not establish loading an unintended module or an authoritative refusal requirement; retain fact without an active defect verdict.',
 'G:D23':'Catch-all source and history checked; ordinary nonpermission failing global-delete provider remains unexercised. A parser/component model is not an end-to-end delete failure.',
 'G:OBS-H12':'Early purge return is source-supported. Constructing a real namespace enumeration or purge-marker storage failure was not established with ordinary inputs; no monkeypatch was counted.',
 'G:OBS-H13':'Cross-namespace temporary history clearing is source-supported; complete self-uninstall/active-writer reader remains unqualified. No source determination chooses that policy.',
 'G:OBS-H15':'Self-uninstall ordering read; an interactive skipped removal and final shell/history reader was not qualified by the safe directory-only tests.',
 'P:F-NSP-10':'Existing simulation is explicitly a simulated legacy install, not real ZPM 0.7.4. A real install would replace the server-wide language extension; no baseline substitution is called end-to-end.',
 'P:F-NSP-9':'Migration source and saved scratch logs checked. No genuine legacy upgrade producer was qualified in this audit; current installation success is not that neighbor.',
 'P:F-REPO-6':'Refuted by moniker selection before Configure, as the original nc-repo-10-invalid-type evidence records; preserve F-REPO-9 as the separate prompting issue.',
 'G:OBS-H10':'Refuted historical empty-group prediction; DisplayQueryResult only prints a namespace after a row. No claim that source-only reachability proves unrelated behavior.',
}
inactive_reasons.update({'G:D10': 'Reviewed the persisted/temp history writers and the old SIGKILL/Terminate receipts. The safe return cohorts do not exercise process death; an unmodified running command plus a post-death durable-row reader remains unqualified in this audit.', 'G:D26': 'Reviewed old SIGKILL evidence and Q-08. A fresh unfinished action that kept writes, its durable Committed reader and a normal-return neighbor were not run; the existing abnormal-end requirement is preserved.', 'G:D27': 'Reviewed Terminate close/finalization code, old receipt and Q-21. A fresh termination schedule is not qualified by the return-only cohorts; its desired finalized/end-time relationship remains an owner question.', 'G:I2': 'The later-resolution-first consequence is narrowed by TR5 creation-order ties. The interrupted-load registration itself needs a process-end fixture; no fresh interruption was performed and no universal priority claim is retained.', 'G:I5': 'Reviewed Compile status dropping and inherited extension witness. A supported failing processor followed through the complete compile command/external reader is not supplied by C:D27, whose hook is ResolveChildren during uninstall.', 'G:I8': 'Reviewed the NoLock-to-compile-lock argument path. A setter/helper value does not establish a real competing compile-lock schedule; the command/caller/contention gate remains.', 'P:F-HIST-2': 'Reviewed developer manifest commit, dependency entry and durable history persistence plus the original crash receipt. New directory tests return normally, so the crash-in-dependency/durable-row gate remains open.', 'P:F-HIST-7': 'The deterministic commit-window witness uses a patched stall and cannot be promoted to an ordinary-input observation on the unmodified pin. The older graceful-stop case has an unidentified hang cause. Both records are preserved without a fresh whole-command claim.', 'P:F-HIST-8': 'Reviewed restart/IRISTEMP loss code and existing before/after restart logs. The new tests use fresh instances without restarting an in-flight command; that exact restart/outcome reader was not rerun.', 'P:F-LOAD-1': 'Reviewed unchecked clone status and the old benign failed-URL receipt. This URL-to-shell path overlaps G:OOS-1 security scope; no URL-load workflow or shell-boundary investigation was executed in this audit.', 'P:F-LOAD-2': 'Reviewed clone lifetime/root storage and the old successful URL-load receipt. No URL-to-shell workflow was executed because that boundary overlaps the explicitly excluded security investigation; temporary-root behavior remains historically evidenced.', 'P:F-NSP-1': 'The failing enable-from-registry reader requires a real registry/installer-delivery failure. Directory mapping success is not that test; no excluded package/archive traversal was used as setup.', 'P:F-NSP-11': 'Reviewed the three enable-community status calls and old step-failure log. The fresh mapping cohort checks successful first/second passes, not a failing component step plus final shell status; that exact gate remains.', 'P:F-NSP-5': 'Reviewed mapped installer behavior and the old namespace/compile-time logs. The fresh mapped-IPM tests do not run the full source installer into that mapped target; the installer/source-namespace/unmap reader remains unqualified.', 'P:F-NSP-7': 'Reviewed last-source overwrite and source selection. The fresh namespaces have one actual zpm owner, so they do not create the two-record/shared-code precondition; the ambiguous-source reader remains open under its restored Q owner.', 'P:F-NSP-8': 'Reviewed AddGlobalMapping positional arguments and earlier SDK activation counters. Fresh enable commands establish final mapping state but do not count individual configuration activations or collation effects. The narrower provider witness is retained as historical evidence.', 'P:F-REPO-12': 'Reviewed version/platform tag construction and old unpublish logs. No fresh real deployed-tag producer/unpublish consumer was exercised; successful filesystem installs do not qualify this ORAS route.', 'P:F-REPO-9': 'Reviewed moniker filtering, the unreachable Invalid type branch and earlier interactive receipt. A missing-name setup incident also prompted, but it is a different trigger and is not counted as an invalid-type reproduction. A controlled invalid-type terminal/input reader remains.', 'P:F-RES-2': 'Reviewed the documented register phase against the phase dispatcher and existing method-missing log. The new tests do not execute that phase and its multi-phase success neighbor; no current ordinary-command regression was claimed.', 'P:F-RES-3': 'Reviewed makedeployed warning and deployment/source readers plus the old native case. No fresh developer-module command/source-readability neighbor was executed; a model or warning alone does not establish source loss.', 'P:F-RES-8': 'Reviewed the shared null-device close path and old TestQuietOutput failure/pass logs. Nested report tests exercise run identity, not nested quiet device lifetime; they are not substituted for the required quiet command/neighbor.', 'P:F-UNIN-10': 'Reviewed cleanup killing CacheD but not cache indices, and old cleanup/reinstall/add-repo evidence. A fresh full IPM self-removal, source reinstall and filesystem-repo reader was not executed; ordinary module uninstall is not that gate.', 'P:F-UNIN-11': 'Reviewed self-Clean branching and old warning output. A fresh level-0 self-Clean and after-hook observer is not covered by blocked ordinary-module Clean; no root-cause equivalence is assumed.', 'P:F-UNIN-7': 'Reviewed typed/untyped orphan enumeration and inherited interoperability noise logs. The current CLS ownership evidence does not establish the full untyped interoperability/system inventory, so no whole-reader defect claim is retained.'})
# The existing early downgrade cohort is a through-product test.
FRESH['C:D15']=('early','ordinary prerelease downgrade accepted; release and mixed-release downgrades refused')

for id,b in bugs.items():
 b['anchor']=anchor(id);b['owner_path'],b['owner_id']=owner(id)
 if id in ALIASES:
  b.update(disposition='alias',canonical=ALIASES[id],repro_level='alias; see canonical owner',reason='Same trigger and consequence; retain this ID as a forwarding record. C:D25 also shares P:F-UNIN-6 for its returned-status projection.' if id=='C:D25' else 'Same trigger and consequence; retain original ID, evidence and policy qualifications at the canonical owner.')
 elif id in {'G:I4','G:OBS-H7','G:I6'}:
  b.update(disposition='policy-only/source-reviewed',repro_level='source review; historical observations preserved',reason={'G:I4':'The public ROWSPEC lacks namespace/ID. Desired API shape remains Q-28; current query duplication is separately reproduced as G:D20, not a decision to add columns.', 'G:OBS-H7':'Dependency load labels and install-only source display are source/test-supported as built. Alternative labels/display require Q-15; do not retain behavior matching the current working assumption as a defect.', 'G:I6':'Database rollback does not promise filesystem compensation. The failed-lockfile path is reproduced under P:F-DEP-8; the broader external-compensation choice remains transaction.rollback-scope.policy.'}[id])
 elif id=='G:D14':
  b.update(disposition='policy-only/as-built-observed',repro_level='e2e reproduced',group='nested',evidence=str(runmap['nested'][0].relative_to(ROOT)),reason='Ordinary nested load confirms that the dependency NoJournal default escapes parent rollback. This agrees with Q-22 working assumption A; it is a defect only under alternative B. Runtime is settled for the checked pair, policy remains open.')
 elif id in SECURITY:
  b.update(disposition='security-outside-reproduction',repro_level='defensive source/existing evidence review only',reason='Explicitly outside active reproduction scope. Existing source and receipts reviewed without constructing or executing a vulnerability workflow; previous interruptions remain excluded.')
 elif id in FRESH:
  group,why=FRESH[id];b.update(disposition='active',repro_level='e2e reproduced',reason=why,group=group,evidence=str(runmap[group][0].relative_to(ROOT)))
 elif id in PARTIAL:
  group,why=PARTIAL[id];b.update(disposition='active-component-only',repro_level='partial reproduced',reason=why,group=group)
  if group:b['evidence']=str(runmap[group][0].relative_to(ROOT))
 elif id in EXCLUDED:
  b.update(disposition='inconclusive/execution-scope-blocked',repro_level='none in this audit',reason='Good-faith source and existing evidence review only. The discriminating application entry requires excluded archive/verify traversal; no new investigation, probe or reproduction was attempted. Historical native evidence is not erased.')
 elif id in DOCS:
  b.update(disposition='documentation-or-test-gap',repro_level='source review',reason='Read the product comment/help/test and its existing resolution. This is documentation or missing test quality, rather than an independently reproduced functional defect. See the preserved starting record; assumptions are not maintainer approval.')
 elif id in {'G:D7','G:OBS-H9'}:
  b.update(disposition='withdrawn/source-settled',repro_level='historical',reason='Preserve the documented withdrawal and its original attributed source. Raw status details and unconfirmed global deletion were explicitly documented; no new contrary intent inferred.')
 elif id in {'P:F-REPO-6','G:OBS-H10'}:
  b.update(disposition=('refuted/unreachable-CLI-trigger' if id=='P:F-REPO-6' else 'refuted'),repro_level='historical contrary evidence',reason=inactive_reasons[id])
 elif id in {'G:OBS-L15','G:OOS-3'}:
  b.update(disposition='outside-functional-contract',repro_level='historical',reason='Existing API/reporting scope limitation preserved. No new product requirement was created to turn an observation into a defect.')
 else:
  b.update(disposition='inconclusive',repro_level='none newly qualified',reason=inactive_reasons.get(id,'Good-faith check: inspected the starting entry, its referenced source/previous native evidence, current owner and safe regression cohort. A fresh supported caller with a discriminating external-reader assertion and passing neighbor is not established for this exact trigger. Existing component/model or old runtime evidence is preserved; lack of reproduction is not proof of correctness.'))

bugs['G:OBS-L3']['reason']='Only the first plain-load lockfile location is aliased to C:D16. The broader reading that Root remains absent after return is refuted by the fresh stored-Root reader; later phase/root writers are preserved. Repository absence on a direct directory load is not itself made a defect.'
write(HERE/'bugs.json',json.dumps(list(bugs.values()),indent=2))
review=HERE/'BUG-REVIEW.md'
body=['# Findings review against the sealed iteration 28 register','',
 'Starting inventory: 165 primary IDs (C 31, G 54, P 80). Referenced-only retired IDs and raw builder labels remain in their inherited ID registry; they are not invented new findings. The frozen archive and all starting entries are saved in `sealed/`; the live pre-edit files are in `before/`.',
 '', 'The scope decision is distinct from truth: inconclusive entries remain possible defects, and historical observations remain evidence. No ordinary untested claim is retained as an active functional defect. A component entry states its missing caller. Security entries have only defensive source/existing-evidence review. No protected Need fix, Defer or Irrelevant entry status appeared in the nine starting registers; source-supported human determinations and withdrawals remain recorded.','']
for b in bugs.values():
 body += [f'<a id="{b["anchor"]}"></a>',f'### {b["id"]}', '',f'Disposition: {b["disposition"]} · Reproduction: {b["repro_level"]}', '',b['reason'],'',link('Canonical ontology',ROOT/'.g5/ontology'/b['owner_path'],review,b['owner_id']), '', 'Starting evidence and claim:', '',move_links(b['original'],ROOT/FILES[b['arm']],review),'']
for b in bugs.values():
 if b.get('canonical'):
  canon=b['canonical']
  dest=(ROOT/FILES[canon.split(':')[0]]) if canon in bugs else ROOT/'.g5/uplift/consolidated/QUESTIONS.md'
  marker='### '+b['id']+'\n'
  at=body.index(marker) if marker in body else -1
  # Entries in body are list items, so locate their exact heading item.
  at=next((i for i,v in enumerate(body) if v=='### '+b['id']),-1)
  if at>=0:body.insert(at+2,'Canonical finding/question: '+link(canon,dest,review,anchor(canon) if canon in bugs else '')+'.\n')
body += ['## Referenced-only inherited aliases','',
 '<a id="c-d1"></a>', '### C:D1 precedence claim', '',
 'Normative defect credit was already withdrawn. The same-version temporary-repository precedence is owned by C:Q1 / load.dependencies.local-repo.precedence. Its observed creation-order tie is preserved; no new defect is counted.', '',
 '[Inherited meaning-based aliases](../extension/reconciled-findings.json) also retain G:V6 / P:V4 empty-manager links to G:D13. The post-commit macro failure P:F-LOAD-4 remains a different trigger.', '',
 '[Exact constructor and active-history withdrawals](../iter2/boundaries/withdrawals.json) preserve OW-CONSTRUCTOR and L12: immediate default-lock release does not withdraw all BuildCache properties, and the locked close schedule does not withdraw the accessible-delete absence schedule.', '']
write(review,'\n'.join(body))

# Registers are canonical entries plus redirects. Removed claims live only in the review.
for arm,path in FILES.items():
 target=ROOT/path
 lines=[f'# IPM findings: {arm} register','', 'Product: `571e04a1`; baseline: sealed iteration 28. '+link('Full reconciliation and commands',HERE/'README.md',target)+'. Decisions and historical claims are preserved in the review. Only the heading entries below are active functional findings.','']
 active=[b for b in bugs.values() if b['arm']==arm and b['disposition'].startswith('active')]
 for b in active:
  title=re.sub(r'\[([^\]]+)\]\([^)]+\)',r'\1',b['title']).replace('|',';').replace('**','')
  if len(title)>160:title=title[:157]+'...'
  lines += [f'<a id="{b["anchor"]}"></a>',f'### {b["id"]}: {title}','',f'Severity: {b["severity"] if b["severity"] in {"High","Medium","Low","Critical","Unranked"} else "Medium"} · Decision: Open','', 'Found by: '+link('Starting entry and evidence',review,target,b['anchor']), '', 'Reproduced by: '+b['repro_level']+(' (historical helper witness; not rerun)' if b['id']=='C:D19' else '')+'. '+b['reason']+'.', '']
  if b.get('evidence'):lines += [link('Executable regression receipt',ROOT/b['evidence'],target)+'; '+link('Runner',HERE/'run_regressions.py',target)+f' `{b["group"]}`. The receipt records commands, expected observations, actual readers and provider qualifications.','']
  lines += [link('Governing owner',ROOT/'.g5/ontology'/b['owner_path'],target,b['owner_id'])+'. No product repair or maintainer approval is claimed.'+(' This is a reproduced discrepancy under an open policy assumption, not a settled defect verdict.' if b['id'] in {'G:D4','G:D6','G:D12','G:D25','G:D21','G:D22','G:D28','G:I3','G:D29','G:D18','G:D23','P:F-INST-1','P:F-INST-8','P:F-INST-9','P:F-INST-10','P:F-DEP-9','P:F-SYNC-1','P:F-SYNC-3'} else ''),'']
 lines += ['## Outside active reproduction scope','']
 for b in bugs.values():
  if b['arm']==arm and b['disposition']=='security-outside-reproduction':
   lines += [f'<a id="{b["anchor"]}"></a>',f'{b["id"]}: defensive source/existing-evidence review only; outside active reproduction scope. '+link('Preserved claim and scope',review,target,b['anchor']), '']
 lines += ['## IDs forwarded to their owning record or review','']
 for b in bugs.values():
  if b['arm']!=arm or b['disposition'].startswith('active') or b['disposition']=='security-outside-reproduction':continue
  lines += [f'<a id="{b["anchor"]}"></a>',f'{b["id"]}: {b["disposition"]}. '+link('Disposition, source and remaining gate',review,target,b['anchor']), '']
 # Preserve legacy heading fragment IDs for inbound links.
 old=read(path)
 old_anchors=re.findall(r'<a id="([^"]+)"',old)
 old_anchors += [re.sub(r'[^\w\- ]','',x.lower()).replace(' ','-') for x in re.findall(r'^### (.+)$',old,re.M)]
 for a in dict.fromkeys(old_anchors):
  if a and f'id="{a}"' not in '\n'.join(lines):lines += [f'<a id="{a}"></a>']
 write(target,'\n'.join(lines))

counts=Counter(b['disposition'] for b in bugs.values())
write(HERE/'bug-counts.json',json.dumps({'starting_ids':len(bugs),'by_arm':dict(Counter(b['arm'] for b in bugs.values())),'dispositions':dict(counts),'repro_levels':dict(Counter(b['repro_level'] for b in bugs.values()))},indent=2))
print(json.dumps(dict(counts),indent=2))
