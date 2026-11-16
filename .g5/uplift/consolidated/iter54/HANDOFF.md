# Iteration 54: `help` rendering and `version`

Batch 11 (convergence test over the remaining untested commands). Product `571e04a1` is unchanged. [Model](../../../Verus_verification/consolidated/iter54/helpcmd.rs), [cases and controls](../../../Verus_verification/consolidated/iter54/cases.json), [bindings](bindings.json), [ledger with predictions](working-ledger.json). The owner is [inspect](../../../ontology/features/inspection.g5.md#inspect) ("`help` shows help for the shell or for one command"; "`info` and `version` show the IPM environment and version"). No product test calls `help` or `version`.

## Predictions and observations

| Case | Observed | Owner predicted | Source predicted |
|---|---|---|---|
| h1 `help` | "Available commands:", all 39 commands one per line, aliases appended, "For more detail" hint | every command | same |
| h2 `help install` | description, Arguments, Flags (incl. `-dev`), Examples | full detail | same |
| h3 `help ver` | `version` detail | alias resolves | same |
| h4 `help nosuchcmd` | warning, then the full listing | unspecified | same |
| h5 `help 1` | full listing, no warning | unspecified | same (subscript 1 is the alias table) |
| h6 `help compile` | shows `-only, -o` and `-dev` although `compile` declares no modifiers | n/a | merged from module-action |
| h7 `help -v` | 39 commands, 38 descriptions (`enable` has only a summary) | full detail | same |
| h8 `help INSTALL` | warning, full listing (case-sensitive, like the parser) | unspecified | same |
| h9 `help module-action` | Flags incl. `-only` | n/a | same |
| v1 `version`, no remote registry | `USER> zpm 0.10.11-SNAPSHOT` only | IPM version | same |
| i1-i3 `info` intro | "No registry configured" with none or with a filesystem repo; "Current registry http://127.0.0.1:9/" once a remote registry is defined | n/a | same |

**No new discrepancy was found.** One fact is recorded and not registered: a numeric name that hits a structural subscript (`help 1`) lists everything without the unknown-command warning, because subscripts 1 and -1 hold the alias table and the column width.

## Evidence

- Formal: `helpcmd.rs` 1 verified (a single conjunctive `neighbors` lemma), 0 errors. Five controls were each rejected: `structural-warns`, `unknown-detailed-nothing`, `alias-not-resolved`, `no-lifecycle-unify` and `registry-from-any-repo`.
- Native: [receipt](native/20261116T181702519002Z/observations.json), 125 fields, all gated. The exploratory run [attempts/explore1.log](attempts/explore1.log) supplied the expected values after matching every source prediction.
- Replay: 35 fields in Verus. Flips of `h5.warning`, `h6.only` and `i3.currentregistry` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter54/run.py` (17 s).

## Reach candidates

Newly reached against the iteration53 map: `CLI.cls::%Help`, `UnifyLifecycles`, `%HelpForCommand`, `%HelpForCommandDetails`, `%HelpForCommandParameters`, `%HelpForCommandModifiers`, `%HelpForCommandExamples`, `%GetCommandStructure` and `TerminalPromptColor`, plus `Main.cls::Version` and `GetVersion`. Strengthened: `Main.cls::BuildIntroMessages`. Not claimed: `DrawBorder`, `FormatName` and `GetVersionModule` (not on these paths).

## Limits

Markdown help (`-m`), column widths and the remote registry query in `version` are not assessed; the last is excluded. The command table is the pinned XDATA, and the native listing count confirms it. No ObjectScript body is verified directly.

## Proposed owner text

In `.g5/ontology/features/inspection.g5.md#inspect`, replace the `help` bullet with:

- "`help` lists every command with its aliases and summary. `help <command>` or an alias shows that command's arguments, flags and examples, and phase commands such as `compile` also show `module-action`'s flags. An unknown name, including a different-case name such as `INSTALL`, prints a warning and the full list."

Add to the `info`/`version` bullet: "`version` prints IPM's version per namespace and each remote registry's version. The shell intro names the current registry only when a remote registry is defined."

Metadata line: "Iteration54: [helpcmd.rs](../../Verus_verification/consolidated/iter54/helpcmd.rs) `help`, `shows_only`, `intro_current_registry`, `neighbors` ([handoff](../../uplift/consolidated/iter54/HANDOFF.md))." No register entry.
