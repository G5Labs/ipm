# Running package commands

## Managing packages in a namespace

<a id="ipm"></a>
[I · ipm]

IPM installs, updates and removes modules in an IRIS namespace, together with their dependencies. It gets them from local folders, archives, git repositories or package registries.

## Running a command from a script

<a id="ipm.shell"></a>
[I · ipm.shell]

Users run IPM commands in the `zpm` shell or as `zpm "<command>"`. Programs call `%IPM.Main:Shell()`, which returns a status that shows whether the command succeeded.

<a id="ipm.shell.forms"></a>
[D · ipm.shell.forms]

Only `%IPM.Main:Shell()` lets a script see that a command failed. `Shell(command)` returns the status, and `Shell(command, 1)` also ends the process with exit code 1 on failure.

- The `zpm "<command>"` command form discards the status. Adding `,1` does not pass a flag; the process keeps running and exits 0.
- As built, the `$zpm("<command>")` function form does not return the status. It runs the command, then fails with `<COMMAND> Function must return a value` whether the command passed or failed (C:D24).

## Metadata

### ipm

- human-provided
- user-facing-behavior
- Children: [shell](#ipm.shell).

Sources: README L1-6; vendor manual AIPM. Inventory: A1 section 1.

### ipm.shell

- human-provided
- user-facing-behavior
- Expands: [IPM](#ipm).

Sources: `wiki/01` L109-167; CHANGELOG 0.10.9 (`#1186`); CONTRIBUTING L115-156. Inventory: CAT-41; A2 1.1 (`%IPM.Main:Shell`, `ShellInternal`), 1.3 (`ShellScript`, `zpm` command routine). Observed: `Shell` runs the daily [history purge](../concepts/data-policy.g5.md#history.retention) before each command and returns its error without running the command if it fails. The `zpm "<command>"` form discards the status. The function form was meant to return it, but see [entry forms](#ipm.shell.forms): as built it raises an error (corrected in iteration 5).
- Children: [entry forms](#ipm.shell.forms).

### ipm.shell.forms

- design-verified

- agent
- user-facing-behavior
- Expands: [Shell](#ipm.shell).

Direct decision: `Main.cls::Shell` (`pTerminateOnError`), `Main.cls::LanguageExtensions` (the `ZPM` body, lowercase `do ##class(%IPM.Main).Shell(pArgs...) quit` since commit `8eb520fd`, 2025-07-02), `Main.cls::UpdateLanguageExtensions` (builds `%ZLANGF00` by rewriting only lines containing the exact text `Do ##class`, unchanged by that commit). Before `8eb520fd` the same rewrite produced `Quit ##class(...)`. Verus [entry_forms.rs](../../Verus_verification/consolidated/iter5/entry_forms.rs) `{function_returns_value,pinned_body,outcome,entry_neighbors}`; controls `matcher-case-insensitive`, `pinned-body-capitalized`, `command-flag-terminates`, `plain-shell-terminates` rejected. Native Z1-Z5 through stdin sessions on a fresh container, including the installed `%ZLANGF00` text. Why the command form ignores `,1` was not traced; only its outcome is claimed.

State machines: [entry forms · iter5](../../Verus_verification/models/iter5-entry-forms/manifest.json)

Finding: [C:D24](../../Verus_verification/bugs_issues.md#c-d24).
