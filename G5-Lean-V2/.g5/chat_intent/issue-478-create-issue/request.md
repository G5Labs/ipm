# Support Create Issue (issue #478)

## Source

[G5Labs/G5-Lean#478](https://github.com/G5Labs/G5-Lean/issues/478), "support Create Issue":

> 1. Home page includes a "Create" button next to "Issues and Tasks"
> 2. Create issue flow focuses on basic issue properties supported by all integrations.
> 3. In addition to the "Create" button at the end of the Create dialog, also offer Create & Fork Workspace
> 4. Support create issue from the command line 'g5d issue' which matches 'gh issue' as closely as possible.

## Clarifications

The workspace's open-questions document (`docs/24f41a94-6d44-463c-8a6d-cbfde7bbfa73/open-questions.md` in the docs repository) raised 34 questions in five sets under `docs/24f41a94-6d44-463c-8a6d-cbfde7bbfa73/questions/`. On 2026-10-13 the owner answered all of them, and each answer reports `decision: true`:

| Set | Question | Decision |
| --- | --- | --- |
| home | home-create-layouts | Workbench layout only |
| home | home-create-after | Open the new issue's detail view in G5 |
| home | home-create-unavailable | Visible but disabled, with the reason |
| home | home-create-prefill | No pre-fill; start empty |
| properties | basic-fields | Title, Description (Markdown), Assignees |
| properties | which-integrations | GitHub Issues only, for now; revisit when Jira lands |
| properties | issue-templates | Ignore templates |
| properties | default-assignee | Unassigned |
| properties | parent-issue | ~~Not in this release~~ Superseded 2026-10-13: create under an optional parent (see [Parent issue](#parent-issue)) |
| fork | fork-issue-role | Primary |
| fork | fork-interaction | Open the existing fork dialog with the new issue preselected |
| fork | fork-source-workspace | "The person picks, with no default: Same as Start workspace today." |
| fork | fork-start-chat | Free text: "Since we're using the Fork dialog, shouldn't have to invent anything new here - the dialog already offers a checkbox" |
| fork | fork-partial-failure | Issue exists, workspace failed; retry with Start workspace |
| fork | fork-dialog-create-option | No, Home only for now |
| cli | cli-agent-may-create | Yes, the agent creates directly (supersedes "drafts only") |
| cli | cli-author-identity | The workspace owner's account |
| cli | cli-where-runs | Inside a G5 workspace (terminal and agent) |
| cli | cli-flag-scope | `-t/--title`, `-b/--body` and `-F/--body-file`, `-a/--assignee` (including `@me`) |
| cli | cli-interactive | Never prompt; fail with a clear message |
| cli | cli-link-workspace | No link. Free text: "But we will need some g5d command to link an issue to the workspace to meet our agent-parity goal." |
| cli | cli-parity-beyond-create | `create`, plus gh-style aliases for the existing commands (for example `view` for `show`) |
| cli | cli-create-and-fork | No, UI only |
| cli | cli-output | The issue URL (gh style), plus full details with `--json` |
| policy | supersede-deferral | Lift the deferral; reuse the guarded comment path as the first broker step |
| policy | other-entry-points | Workspace rail quick-add task; issues from delayed or out-of-scope meeting decisions |
| policy | author-identity-ui | The person's own GitHub account; creation needs a connected account |
| policy | who-may-create | Any organization member; their own GitHub permissions still apply |
| policy | destination-choice | Always the organization's default creation target |
| policy | admin-enablement | The existing "G5 comments" switch also allows creation |
| policy | public-destinations | Refuse; private and internal repositories only |
| policy | payload-confirmation | Same rule as comments: review step for untrusted domains, direct for trusted |
| policy | secret-screening | Yes, the same screen as comments |
| policy | duplicate-safety | "Outcome unknown"; G5 checks, then reports created or not created |

The owner then asked: "read question answers and continue implementation" (2026-10-13).

## Parent issue

Later the same day (2026-10-13) the owner asked whether the implementation supports sub-issues. The agent answered that it did not, by the parent-issue answer, and estimated two sizes: a small one adding a parent to the Home dialog and `g5d issue create`, and a large one adding "Create sub-issue" to the fork dialog with the design's fork follow-on record. The owner then instructed:

> go build the small surface. Update the decision in request.md.

The owner's answer in the question set file still reads "Not in this release"; this instruction supersedes it.

### Agent interpretation

- **Scope.** The small surface is an optional parent on issue creation: the Home dialog (and the rail's quick-add, which reuses it), `g5d issue create --parent`, and both create routes. The fork dialog's create option stays out, per fork-dialog-create-option, and so do the fork follow-on record and showing an issue's hierarchy.
- **Where the parent may be.** Only in the creation target, because putting an issue under a parent writes to the parent issue. That write then goes to the destination creation already admitted (approved, G5 comments on, not public).
- **No pre-fill.** home-create-prefill still holds on Home: the field starts empty there. (Quick-add was empty too until the instruction below.)

The owner then asked (2026-10-13):

> Can we wire up the "Add a Task" button in the "Issues and Tasks" sidebar to create a sub-issue if there is one

Agent interpretation: "if there is one" means the workspace has a primary issue. Quick-add's dialog then starts with that issue as the parent, so the task becomes its sub-issue. The person can change or clear it, and the task is still linked to the workspace as related. A primary issue in a repository other than the creation target cannot be a parent, so quick-add then starts without one. Home is unchanged.

## Agent interpretation

- **Source workspace for Create & Fork.** The chosen option says the person picks, but its note says "Same as Start workspace today", and the answers to fork-interaction and fork-start-chat both say to reuse the existing dialog unchanged. Today Home's Start workspace opens `CreateWorkspaceDialog`, which always forks the organization's base workspace. The agent follows the reuse intent: Create & Fork opens that same dialog. Offering a source picker would be a change to Start workspace as well.
- **Agent link command.** The note on cli-link-workspace asks for a `g5d` link command. The agent adds `g5d issue link`, which reaches the same link operation as the UI's Link existing and Import task, as the workspace owner.
- **Meeting decisions.** The review meeting's decision cards are not implemented yet (`g5.workbench.meeting` is at 0%). The contract records that those cards create issues through the shared create flow; that delivery waits for the cards themselves.
