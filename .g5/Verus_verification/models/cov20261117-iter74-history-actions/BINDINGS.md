# history actions · coverage iter74

Which commands write History records and what install, update and uninstall write into them: row creation before or after the argument checks, Success, Committed (LoadNewModule marks; Finalize Committed || ISOK for uninstall), the logged phase lists, and dependency records written by <module> clean and <module> test. Shares history.rs and playbook uninstall.rs; native i1-i7, u1-u5, n1-n5, c1, c2, t1, 363 fields.

- install writes its record before any check, so every install that reaches Main.Install is recorded; the command parser refuses an empty name before that (`install_always_recorded`, `api_install`, `cli_install`). Native i1-i5, i4api.
- update writes a record exactly when it passes the -PermitDowngrade and installed checks; reinstall of a module that is not installed and uninstall with no name write none (`update_recorded_iff`, `reinstall_not_installed`, `uninstall_no_name`). Native u1-u5, r1, n3.
- uninstall writes a record for every name, including one that is not installed; its Committed equals its success because nothing marks it early (`uninstall_committed_is_success`, `uninstall_one_record`, `n5_facts`). Native n1, n2, n4, n5.
- For install and update, Committed equals "changes kept" except the early no-transaction marker (`install_committed_is_kept`). Native u2, u3, i5-i7.
- Logged phases: install five, update six ending in ApplyUpdateSteps, uninstall Clean; a failure lists the phases up to the failing one (`logged_lists`, `run_shape`, `install_phases`). Native i1, i5, u1, u2, n1.
- `<module> clean` writes no record of its own but one uninstall record per dependency its recursion visits; `<module> test` writes a load record per scoped dependency it installs; list, sync, reload and compile write none (`cli_clean`, `c1_facts`, `clean_without_deps_writes_nothing`, `other_commands_no_record`). Native c1, c2, t1.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declarations](../../coverage-20261117/iter74/actions.rs)

[Shared row model](../../consolidated/verus/history.rs)

[Shared uninstall model](../../../../playbook-uplift/models/verus/uninstall.rs)

[Product correspondence](../../../uplift/coverage-20261117/iter74/bindings.json)

[Proof and controls](../../../uplift/coverage-20261117/iter74/proofs/models.json)

[Iteration handoff](../../../uplift/coverage-20261117/iter74/HANDOFF.md)
