# Trust inventory: settings.rs (module 06)

What a "proved" (P) or "model-checked" (M) claim from `models/verus/settings.rs` rests on without itself being proved.
Written for review/REVIEW.md finding 15; the S items were first listed in `uninstall.TRUST.md`, which now points here.

The file uses no `assume`, `admit`, `external_body` or `external` (`scripts/record_proofs.py` refuses them and records
`trusted_items: []`). Premises are not trusted items. Each one is a `premise_*` spec function, declared at the top of
the file as `// PREMISE <fn> = <LABEL>`, required by the lemmas that need it, and dropped once by a control in
`controls/settings.json`; the proof must then fail. `scripts/ontology_index.py` checks that every P tag's statement
names each premise its lemma requires.

## Abstractions (trusted, with what checks each)

| # | Trusted item | Why it is trusted | What checks it |
|---|---|---|---|
| S1 | Config is transcribed as SetValue / GetValue / ResetToDefault over two nodes per key (`val`, `dflt`) in one store (`^IPM.settings` in %SYS). ModuleRoot values are taken as already normalized and writable. The seeded defaults are data (`seed`, preload/cls/IPM/Installer.cls#L114-L122). The shell drops the helpers' statuses (Main.cls#L2063-L2093). | Hand transcription with line anchors; ObjectScript has no verifier. Directory checks for ModuleRoot are not modelled. | Twin `models/explorer/repo/settings_twin.py` (same spec fn names, `repo-transcription-parity-settings`). Replays of `nc-repo-6-config-delete`, `nc-repo-8-config-errors` and `nc-repo-model-config-delete` (a prediction made before the run) with 0 mismatches. Twin mutants `default-overwritten` and `reset-to-value` are each contradicted by IRIS (`repo-twin-mutants-rejected`). |
| S2 | IRIS keeps a `^%` global in one database for every namespace (`pct_db` is a constant). | IRIS environment fact. | `nc-repo-env-percent-global-shared` and `nc-repo-7-default-modifiers-namespaces` replayed; twin mutant `percent-per-namespace` rejected by IRIS. |
| S3 | A SQL query with no ORDER BY returns rows in ID order, so each kind's `PublishingServer` row is its first enabled registry. | IRIS environment fact, used by the publish choice. | Probed before use: `nc-repo-model-publish-row-order` (four cases) replayed; twin mutant `first-row-best-order` rejected. Not a guarantee of IRIS; a different plan could change it. |
| S4 | Registries are a sequence in ID order with unique names. `offer` stands for the newest version of the wanted module that a registry returns. | Package services, version parsing and HTTP are not modelled. | `nc-repo-1`, `nc-repo-2`, `nc-inst-10b`/`10c` replayed. |
| S5 | Default modifiers are a flat map; `-set` merges the command's data array, `-delete` kills the global. | The real data array also carries a `zpm` node that the shell adds. | Only the `Nc*` options are compared in `nc-repo-model-default-modifiers-merge`; the extra node is a gap. |
| S6 | Command status is two-valued (`St::Ok`, `St::Err`); publish "fails" is `publish_target = None`. | The text of messages is not modelled. | Status values are compared in the replays of `nc-repo-1`, `nc-repo-2` and `nc-repo-8`. |
| S7 | Namespaces in the default-modifier L1 are a set `nss`; the L1 keeps one list per namespace. | Needed to state REPO-B106. | Premise PNS makes `repo_refines_dm_set` vacuous (with one namespace the L1 and the shared store cannot differ). It is kept only to show the model agrees with that L1 in that case and to locate control `impl-dm-set-wrong-db`; it is cited as P nowhere. REPO-B106 is F (F-REPO-5), witness `repo_witness_dm_shared`. |

## Premises (named, each dropped once by a control)

| Label | Spec fn | Meaning | Required by | Excludes | Control that drops it |
|---|---|---|---|---|---|
| PS | `premise_ps` | The key's default node holds the installer's seeded value (or the key resets to a computed default) | `repo_refines_config_delete` | F-REPO-4 (REPO-B99), so that lemma is cited on no statement | `premise-drop-ps` |
| PNS | `premise_pns` | One namespace uses IPM on the instance | `repo_refines_dm_set` | F-REPO-5; vacuous for REPO-B106 (S7) | `premise-drop-pns` |
| PH | `premise_ph` | Some enabled remote or ORAS registry holds the publish flag | `repo_refines_publish_default` | F-REPO-3 (REPO-B56) | `premise-drop-ph` |
| PU | `premise_pu` | At most one enabled registry holds the publish flag | `repo_refines_publish_default` | the case REPO-B59 forbids and the code allows (one per kind; F-REPO-3) | `premise-drop-pu` |
| PK | `premise_pk` | The flag holder is the first enabled registry of its kind | `repo_refines_publish_default` | F-REPO-10 (REPO-B55) | `premise-drop-pk` |
| PE | `premise_pe_named`, `premise_pe_all` | The registry publish -r names (or every registry search lists) is enabled | `repo_refines_publish_named`, `repo_refines_search` | F-REPO-7 (REPO-B54, REPO-B82) | `premise-drop-pe-named`, `premise-drop-pe-search` |
| PT | `premise_pt` | Equal top versions are offered in sort order | `repo_refines_install_pick` | F-INST-4 | `premise-drop-pt` |

Lemmas with no premise, and the statements that cite them: `repo_install_enabled_only` (REPO-B12),
`repo_publish_named_missing` (REPO-B53), `repo_publish_default_enabled` (REPO-B57, B58), `repo_settings_shared`
(REPO-B97), `repo_refines_config_set` (REPO-B98), `repo_dm_laws` (REPO-B103..B105).

## Both directions of the status (review finding 1)

- Worked, so success: `repo_refines_config_set` ensures `l1_config_status` (a known key with a valid value reports
  success); `repo_refines_config_delete` ensures `St::Ok` for a known key; `l1_publish` with no -repo requires a target
  whenever a holder exists. Controls `impl-valid-set-reports-err`, `impl-delete-reports-err` and
  `impl-publish-both-kinds-none` break each one and are killed in that lemma.
- Failed, so no success: for config this is contradicted by the code (F-REPO-8, DEC-REPO-4 open; witness
  `repo_witness_config_errors_report_ok`); there is no lemma for it. For publish, a missing named registry fails
  (`repo_publish_named_missing`, control `impl-missing-name-falls-back`) and no -repo never picks a disabled or folder
  registry (`repo_publish_default_enabled`).
- No command here has a crash or transaction step: settings and registry writes are single global sets.

## Bounds

- The Verus lemmas have no size bound: any number of keys, registries and namespaces.
- The model checks (part C of `models/explorer/repo/REPORT.md`) are bounded: config sequences of up to two operations
  over five keys and five values from two start states; default-modifier sequences of up to three operations over two
  namespaces; every list of up to three registries of each kind, enabled, flag and token; every list of up to three
  registries with offers and sort orders.
- The IRIS replays cover the observed cases only (part B of the same report).
