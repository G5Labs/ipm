# Iteration 5 trust and claim boundary

Input-driven Verus hand translations, not verified ObjectScript. `implementation_proofs = 0`. No `assume`, `admit`, `external_body` or trusted theorem. A control counts only when Verus rejects an assertion or postcondition. Native replay compares model results computed from fixture inputs with observed fields; one field per model is flipped and must be rejected.

| Boundary | Representation and scope | Evidence and residual |
|---|---|---|
| Web application properties | Abstract key/value maps per application name in `%SYS`. Declared keys replace, undeclared keys stay. `Modify` only when a declared value differs. | Native Description (declared) and Timeout (undeclared) through `Security.Applications.Get`. `Security.Applications` internals, CSP gateway notification, JWT/CORS keys and `CSPApplication` file purging are not modeled. |
| Ownership | No creator record; duplicate names across installed modules are refused at manifest import. | Native administrator-created app and a second module. Applications created by other tools with module-like names are covered by the same rule but not separately run. |
| Failure and rollback | `transaction` is a boolean input; with it, the `%SYS` write is rolled back with the load. | Native failing invoke after Activate with and without `-DNoTransaction=1`. Developer-mode commit points, `-DNoJournal=1` and failures between Configure and Activate are open. |
| Uninstall | Dependents count, force and Clean level are inputs; processors act before the check. | Native one dependent, with and without `-force`. Global scope, `-recurse`, `clean` at level 0 and CSP purge are P's evidence or source-only. |
| Upgrade | Modeled as identity on the application set. | Native `load` of a higher version without the resource. The `update` command, update steps and registry upgrades are not run. |
| Validation | `validate_dispatches()` is false, derived from `%Validate` not dispatching; no lifecycle subclass overrides it in the pinned source. | Native unknown attribute accepted. Custom lifecycles that override `%Validate` are outside this claim. |
| Entry forms | Exit code, continuation and command error by form; the function form's return comes from the extension body's spelling. | Native stdin sessions and the installed `%ZLANGF00`. Why the command form ignores `,1` is not modeled; only the outcome. Terminal and `iris terminal` launchers other than `iris session` are not covered. |
| Redirects | A hop sequence of origins; credentials per hop as built, under C:S14 and under C:S14b. | Native 302 from the repository host to another host on the download. Redirects on metadata, listing and `_ping`, HTTPS, and cross-scheme redirects are not exercised. |

Functions prefixed `repaired_` and the `s14`/`s14b` rules are proposals; their proofs do not mean the product was repaired. Iteration 3 and 4 files are unchanged; iteration 4's whole command was replayed in a scratch extraction.
