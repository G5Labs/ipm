# Workspace sharing — v8 source clarification

Source: `/Users/zliu/Workspace/docs-Lean/workspace.share.md`, captured 2026-09-17. The clarification below is preserved verbatim; its contracts are incorporated into [design v8](lean-backend-design-v8.md) and [plan v8](lean-implementation-plan-v8.md). The source note’s named PRs are the minimum affected set; the plan also covers dependent dispatch, authorization, UI and cleanup paths.

**The model in one line.** A workspace has exactly one writer, its owner. Everyone else with access reads. The write seat moves by transferring ownership, never by adding writers.

## Access and identity

Read access comes from the org access model, as in the original G5: org admins read any workspace, and other members read according to the workspace's visibility, which is either the whole org or an invite list. Write access is not a list; it is the single `owner_user_id`.

| Capability | Reader | Owner |
|---|---|---|
| Browse files, docs, chat history, task timelines, PRs | yes | yes |
| Clone and fetch the repositories from internal git | yes | yes |
| Start a preview, cancel any preview | yes | yes |
| Send into a chat, answer a question, use the terminal | no | yes |
| Save a file, commit, push from the VM or a laptop | no | yes |
| Stop, resume, restart, delete the workspace | no | yes |
| Create a PR, record an approval decision from the workspace | no | yes |
| Transfer ownership | no | yes |
| Fork the workspace into one they own | yes | yes |

Reads are identical for both roles. While the VM runs, reads come from its file system through the API. While it is stopped, they come from internal git: the last flush snapshot when there is one, otherwise the branch head. Reads never wake the VM.

Because one human owns the VM at a time: the VM carries only the owner's provider account, so per-session grants are unnecessary; git commits made in the VM carry the owner as author; repository and task secrets need no per-user isolation. An org admin who needs to write takes ownership first, and that transfer is recorded.

## Preview

- A preview is a **task of kind `preview`** with a service lifecycle. It appears in the workspace's task list, records the user who launched it, and stays running until someone cancels it or the VM stops.
- **One active preview per workspace.** A second start replaces the running one, whoever launched it.
- **Any member with access may cancel any preview**, reader or owner. This is accepted nuisance surface, because a preview is read-only and cheap to restart.
- **Readers may create and cancel preview tasks and nothing else.** This is the only way a reader causes a VM to start.
- **Wake and idle behave like every other task.** Starting a preview wakes a stopped workspace through the normal dispatch path, preview traffic counts as activity, and the idle timeout stays an org default with a workspace override.
- The preview runs **under the workspace identity**, with the workspace's secrets, never under the launcher's credentials. The launcher's identity is for audit and for the task list.
- **Contract:** a preview must not modify tracked files. G5 does not enforce it, because the command comes from the repository's own profile. Any tracked change it makes shows up in the Changes panel and would be captured by a flush. An overlay that discards preview writes is a later hardening option.
- A preview does **not** consume the workspace's concurrency slot, has **no** wall-clock limit, and does **not** restart itself after a stop and resume.

## Ownership transfer

- The owner may transfer to any member of the org. An org admin may transfer to anyone, including themselves. Every transfer is recorded with both users and the time.
- Transfer runs the **stop sequence with reason `transfer`**: close admission, interrupt running executions, flush, release the VM, then change the owner. A workspace that is already stopped transfers with no VM work.
- If the flush fails, the transfer **aborts** and the old owner keeps the workspace, exactly like a user-initiated stop.
- The new owner resumes the workspace themselves. The VM boots with their provider account.
- Chat sessions, tasks, PRs and history stay with the workspace. An open question stays answerable by the new owner, and the successor turn bills their account.
- The previous owner keeps whatever read access the org access model gives them.

**What this deletes from draft 7:** multi-writer workspaces, any member sending into any session, the author label per message, the question of who may edit another person's queued message, session-scoped provider grants, and the proxy hole where one writer could spend another's provider account.

**Accepted limits:** two people cannot work in one workspace live, so pairing means transferring back and forth or forking. A reader can stop the owner's preview. The owner is a single point of availability, mitigated by admin transfer. This reverses the meetings' aligned decision that multiple writers are supported; the hand-off half of its rationale is covered by transfer, the review-with-an-agent half now costs a fork, and the team should be told.

**Where it lands in the documents:** the data model gains `workspaces.owner_user_id`, a transfer audit record and the `preview` task kind, and `workspace_members` loses its write role. The sections to rewrite are the workspace data model, branch identities, the receive-pack user row, the workspace actions with a sixth action, host-side grants, preview, terminal, the task model, chat sharing, reads and writes, roles, and the decision register. In the plan it touches P4.1, P4.5, P4.7, P4.10, P4.12 and P5.1.