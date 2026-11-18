import json
V='.g5/Verus_verification/coverage-20261117/iter75/'
REG='.g5/Verus_verification/consolidated/iter2/registry.rs::'
OI=V+'oras_integrity.rs::'
UN='playbook-uplift/models/verus/uninstall.rs::'
UNN=V+'uninstall_neighbours.rs::'
EV=['.g5/uplift/coverage-20261117/iter75/mutations.jsonl','.g5/uplift/coverage-20261117/iter75/receipts/index.jsonl','.g5/uplift/coverage-20261117/iter75/ledger/R12.json']
OEV=EV+['.g5/uplift/coverage-20261117/iter75/receipts/coverage-20261117__iter75__oras_integrity.rs.log']
UEV=EV+['.g5/uplift/coverage-20261117/iter75/receipts/coverage-20261117__iter75__uninstall_neighbours.rs.log']
NAT='.g5/uplift/consolidated/iter68/native/observations.json'
PS='src/cls/IPM/Repo/Oras/PackageService.cls'
ml=lambda page,block,model:{'page':page,'block':block,'model':model}
PSP='.g5/ontology/architecture/package-sources.g5.md'
UNP='.g5/ontology/features/uninstall.g5.md'
entries=[
 {'id':'repo.oras.integrity#3','old_grade':'partial','new_grade':'full','check':'proved',
  'models':[REG+'verify_layers',REG+'first_tar_controls_integrity',OI+'size_and_digest_each_decide'],
  'evidence_add':OEV,'support':'mutation-checked','mutations':['R1-m01','R1-m02','R1-m03','R1-m04'],
  'receipts':['receipts/coverage-20261117__iter75__oras_integrity.rs.log'],
  'product':PS+':1014-1021 (skip non-tar mediaType; first tar layer returns manifest_size == size and manifest_hash == expected_hash; no tar layer returns False)',
  'neighbour':'size_and_digest_each_decide: size-only and digest-only mismatches of the first tar layer are refused, a non-tar layer carrying the pulled size and digest does not stand in, a later matching tar layer does not rescue either single mismatch',
  'basis_note':'iter2 verify_layers is a faithful transcription of VerifyModuleHashAndSize; the size-only/digest-only neighbours kill G4-m67/m68 (re-run as R1-m01/m02), the mediaType skip (R1-m03) and the later-layer rescue (R1-m04).',
  'instance_scoped':True,
  'metadata_link':ml(PSP,'repo.oras.integrity',V+'oras_integrity.rs')},
 {'id':'repo.oras.integrity#7','old_grade':'partial','new_grade':'partial','check':'proved',
  'models':[REG+'verify_layers',OI+'size_and_digest_each_decide',REG+'last_tag',OI+'pull_tag'],
  'evidence_add':OEV,'mutations':['R1-m01','R1-m02'],
  'receipts':['receipts/coverage-20261117__iter75__oras_integrity.rs.log'],
  'product':PS+':1014-1021',
  'neighbour':'size_and_digest_each_decide (retaining both checks is now pinned: dropping either comparison fails)',
  'basis_note':'the "retain size and digest checking" half is now mutation-checked (R1-m01, R1-m02 killed).',
  'gap':'"Expose the tag-order and file/layer mismatches" is a documentation policy with no formal observation, and the alternatives in the bullet stay undecided (open question repo.oras.integrity.policy). Retaining both checks is pinned (R1-m01/m02 killed).',
  'metadata_link':ml(PSP,'repo.oras.integrity',V+'oras_integrity.rs')},
 {'id':'repo.oras.integrity#1','old_grade':'partial','new_grade':'full','check':'proved',
  'models':[REG+'last_tag',REG+'tag_order_is_a_real_input',OI+'pull_tag',OI+'tag_selection_neighbours'],
  'evidence_add':OEV+['.g5/uplift/consolidated/iter2/native/receipt.json'],'support':'mutation-checked','mutations':['R1-m09','R1-m10','R1-m11'],
  'receipts':['receipts/coverage-20261117__iter75__oras_integrity.rs.log'],
  'product':PS+':785-787 (empty tag replaced by GetLatestTag), 920-923 (tags[-1] if tags else "")',
  'neighbour':'tag_selection_neighbours: no tag picks the last listed tag ([2,1] -> 1, [1,2] -> 2, [] -> none); an explicit tag is used as given whatever the provider lists',
  'basis_note':'pull_tag adds the explicit-tag input that iter2 last_tag lacked (Pull lines 785-787); mutants that ignore the explicit tag (R1-m09), pick the first tag (R1-m10) or ignore the tag list (R1-m11) are killed. The provider tag order is an input (ORAS get_tags), observed in the iteration-2 native receipt (raw.log TAG-g5iter2order value=1.0.0, TAG-g5iter2sorted value=2.0.0).',
  'instance_scoped':True,
  'metadata_link':ml(PSP,'repo.oras.integrity',V+'oras_integrity.rs')},
 {'id':'repo.oras.archive-boundary#2','old_grade':'partial','new_grade':'full','check':'proved',
  'models':[OI+'pull_effects',OI+'failed_check_keeps_installed_root',OI+'refused_archive_never_replaces_root',REG+'verify_layers',REG+'pull'],
  'evidence_add':OEV+[NAT],'support':'mutation-checked','mutations':['R1-m01','R1-m02','R1-m05','R1-m06','R1-m07','R1-m08'],
  'receipts':['receipts/coverage-20261117__iter75__oras_integrity.rs.log'],
  'product':PS+':826-829 (Pull throws when not verified), 239-246 (GetModule throws the failed status before returning the stream); src/cls/IPM/Utils/Module.cls:142-172 (only a returned stream reaches LoadModuleFromArchive)',
  'neighbour':'failed_check_keeps_installed_root: on a writable root a matching archive reaches the loader and replaces the root; a size-only or digest-only mismatch releases nothing, never enters the loader and leaves the root; refused_archive_never_replaces_root states it for every manifest',
  'basis_note':'successor pull_effects computes the status from the manifest layers and pulled file through verify_layers instead of the iter68 fixture bool size_matches. G4-m115/m116/m117 re-run as R1-m06/m02/m05 are killed, as are root-replaced-regardless (R1-m07) and a Pull status that ignores the verdict (R1-m08). Native correspondence: the iteration-68 receipt observed the size-mismatch refusal keeping the installed-root sentinel and the valid replacement removing it. The digest-only case is not observed natively; it is decided by the same product line 1020 and the bundled oras 0.1.30 download_blob (provider.py:446-474) writes the blob without checking its digest, so the case reaches IPM\'s check.',
  'instance_scoped':False,
  'metadata_link':ml(PSP,'repo.oras.archive-boundary',V+'oras_integrity.rs')},
 {'id':'uninstall.cleanup#1','old_grade':'partial','new_grade':'full','check':'refuted',
  'models':[UN+'clean_code',UN+'code_gone',UN+'unin_refines_complete',UNN+'mapped_code_needs_global','.g5/Verus_verification/consolidated/iter63/cleanup.rs::code_removed'],
  'evidence_add':UEV,'support':'mutation-checked','mutations':['R2-m01','R2-m02','R2-m05','R2-m06'],
  'receipts':['receipts/coverage-20261117__iter75__uninstall_neighbours.rs.log'],
  'product':'src/cls/IPM/Lifecycle/Base.cls:314 (tRemoveMapped = Clean.GlobalScope), 375-378 and 414-417 (IsMapped items skipped with a WARNING unless tRemoveMapped); src/cls/IPM/Main.cls:500 (-global sets Clean.GlobalScope)',
  'neighbour':'mapped_code_needs_global: uninstall without -global keeps a mapped class and succeeds; with -global it is deleted; unmapped code is deleted either way',
  'basis_note':'whole-command neighbours over the faithful playbook clean_code/code_gone; M-m08 and M-m09 re-run as R2-m01/m02 are killed, as are -global not reaching Clean.GlobalScope (R2-m05) and tRemoveMapped fixed to 1 (R2-m06). Check refuted for the unqualified block wording: as built, mapped code survives uninstall without -global (proved for the as-built claim in the recorded cases). IsMapped is an environment input (namespace mapping); no native receipt observes the skip.',
  'instance_scoped':True,
  'refutation':'The block says uninstall deletes the module\'s classes, packages, includes and routines; as built, mapped items are skipped with a WARNING unless -global is given (Lifecycle/Base.cls:375-378, 414-417).',
  'ontology_repair':'uninstall.cleanup P1: state that mapped code is skipped unless -global (HANDOFF-72 §5 item 10, first omission).',
  'metadata_link':ml(UNP,'uninstall.cleanup',V+'uninstall_neighbours.rs')},
 {'id':'uninstall.cleanup#2','old_grade':'partial','new_grade':'full','check':'proved',
  'models':['.g5/Verus_verification/consolidated/iter63/cleanup.rs::globals_by_declaration',UN+'clean_step',UN+'unin_witness_global_kept',UNN+'preserve_keeps_global_data'],
  'evidence_add':UEV,'support':'mutation-checked','mutations':['R2-m03','R2-m04'],
  'receipts':['receipts/coverage-20261117__iter75__uninstall_neighbours.rs.log'],
  'product':'src/cls/IPM/ResourceProcessor/Default/Global.cls:76-80 (Clean: kill @..Global only when Global is non-empty and Preserve is off)',
  'neighbour':'preserve_keeps_global_data: a Global-attribute resource with Preserve keeps its data with and without -global; without Preserve it is killed; unin_witness_global_kept (inherited) keeps a name-only global',
  'basis_note':'M-m12 re-run as R2-m03 (kill even with Preserve) and M-m13 re-run as R2-m04 (kill name-only data) are killed. "Only when named" holds; the block does not mention Preserve, which narrows deletion further (omission, not a contradiction).',
  'instance_scoped':True,
  'ontology_repair':'uninstall.cleanup P1: state that Global data is kept when Preserve is set (HANDOFF-72 §5 item 10, second omission).',
  'metadata_link':ml(UNP,'uninstall.cleanup',V+'uninstall_neighbours.rs')},
]
b=lambda f,file,member,lines,role,note:{'formal':f,'file':file,'member':member,'lines':lines,'role':role,'note':note}
bind=[
 b(REG+'verify_layers',PS,'%IPM.Repo.Oras.PackageService::VerifyModuleHashAndSize','1014-1021','decide','first application/vnd.oci.image.layer.v1.tar layer: size and digest both equal'),
 b(OI+'pull_effects',PS,'%IPM.Repo.Oras.PackageService::Pull','822-829','decide','hash and size of the linked tgz checked; failure throws "Module size and/or hash failed validation!"'),
 b(OI+'pull_effects',PS,'%IPM.Repo.Oras.PackageService::GetModule','239-246','guard','$$$ThrowOnError(status) before returning the stream'),
 b(OI+'pull_effects','src/cls/IPM/Utils/Module.cls','%IPM.Utils.Module::LoadModuleReference','142-172','read','only a returned payload is copied and passed to LoadModuleFromArchive'),
 b(OI+'pull_tag',PS,'%IPM.Repo.Oras.PackageService::Pull','785-787','decide','empty tag replaced by GetLatestTag'),
 b(REG+'last_tag',PS,'%IPM.Repo.Oras.PackageService::GetLatestTag','920-923','decide','tags[-1] if tags else ""'),
 b(OI+'size_and_digest_each_decide',PS,'%IPM.Repo.Oras.PackageService::VerifyModuleHashAndSize','1014-1021','neighbour','size-only and digest-only mismatches'),
 b(OI+'failed_check_keeps_installed_root',PS,'%IPM.Repo.Oras.PackageService::GetModule','239-246','neighbour','match on a writable root vs single mismatches'),
 b(OI+'tag_selection_neighbours',PS,'%IPM.Repo.Oras.PackageService::Pull','785-787','neighbour','explicit tag vs no tag'),
 b(UN+'code_gone','src/cls/IPM/Lifecycle/Base.cls','%IPM.Lifecycle.Base::%Clean','314,375-378,414-417','decide','IsMapped items skipped unless Clean.GlobalScope'),
 b(UN+'storage_uninstall','src/cls/IPM/Main.cls','%IPM.Main (Commands XDATA, uninstall)','500','write','-global sets Clean.GlobalScope=1'),
 b(UN+'clean_step','src/cls/IPM/ResourceProcessor/Default/Global.cls','%IPM.ResourceProcessor.Default.Global::OnPhase','76-80','decide','kill @..Global when Global non-empty and not Preserve'),
 b(UNN+'mapped_code_needs_global','src/cls/IPM/Lifecycle/Base.cls','%IPM.Lifecycle.Base::%Clean','375-378,414-417','neighbour','mapped class with and without -global'),
 b(UNN+'preserve_keeps_global_data','src/cls/IPM/ResourceProcessor/Default/Global.cls','%IPM.ResourceProcessor.Default.Global::OnPhase','76-80','neighbour','Global with and without Preserve'),
]
json.dump({'group':'R12','items':'HANDOFF-72 §6 items 1 and 2','entries':entries,'bindings':bind,
 'not_done':[{'id':'repo.oras.archive-boundary#4','why':'outside item 1: needs declared_blob derived from the pull manifest versus the metadata manifest (a larger successor of iter68 changed_tag)'}],
 'cost':'builder session; about 25 minutes for items 1-2'},open('.g5/uplift/coverage-20261117/iter75/ledger/R12.json','w'),indent=1)
