# Issue #1013: attachments for issues and pull requests

## Human instructions

- 2026-10-31: "Work on G5Labs/G5-Lean#1013: G5 workspace agents cannot inspect private GitHub issue mocks for visual UI work."
- 2026-11-01, after the first increment (`g5d issue images`): "Would this work on other file types? Is there a reason to restrict it to issues?"
- 2026-11-01: "My goal here: a solid abstraction for pulling attachments for remote issues or PRs. I don't want to build 'images' and then have to invent another name to support video. I want to reuse the same attachment fetch mechanism for PRs. Research what you need about github's attachment mechanism. You can also look at jira cloud to see what it does with attachments."
- 2026-11-01: "there is concurrent PR work in branch g5/issue-830-improve-g5-pr-support-in-g5d-pr-cross-workspace. Align the design but defer the work (g5d pr attachments) until that branch lands. The system architecture for issues (and coming soon, PRs) is for a tight adapter abstraction, a github adapter implementation and a local adapter implementation. Align the attachment design here with this model. We should also have a local adapter for attachments (there is a filed feature request for supporting image attachments for the local adapter). Recommend an approach for the local adapter."
- 2026-11-01: "link 726 to this workspace. I accept the local adapter approach but I want it built in this PR, not a separate 726 - we're adding support for attachments in general. Private GitHub file uploads are still reported as unavailable. implement the plan. along the way, update the ontology to no longer say that local adapter doesn't support attachments because it will by the time you are done."

## Interpretation

- One provider-neutral attachment operation pair (list, fetch) serves issues and pull requests; the item's existing owner keeps admission.
- "Attachments" is the noun, covering images, videos and other files, so new kinds need no new command.
- Jira Cloud informs the shape (issue-level attachments with stated type and size) but is not built.
- Listing and fetching attachments are adapter content operations of the issue service and, after #830, the pull-request service; GitHub's adapter is shared by both families.
- The filed request is [G5-Lean#726](https://github.com/G5Labs/G5-Lean/issues/726) ("No images in issues"). The owner accepted the recommended local adapter and asked for it in this PR (2026-11-01), which reverses the native tracker's retirement of attachments for images.

## Attachment store (2026-11-03)

### Human instructions

- "G5 currently uses filestore to persist git repos. Is a design where the attachments are stored there practical? It would not require bloating the database. Presumably we'd need to assign UUIDs to the files and name them appropriately. Investigate and offer pros/cons."
- "How hard would it be to get a second mount for filestore? This would resolve some isolation issues. We could also put the org id in the path, making file path guard possible for better enforcement of tenancy boundaries. I would ignore the lack of backups - we're fixing this anyways (and the loss of all repos is a much bigger catastrophe)."
- "Do we support org deletion now? If not, no reason to do anything here yet (solution is obvious, no need for dead complexity). With the dual mount (option a) does this set us up well for an object store implementation eventually if we wanted it?"
- After the agent proposed doing it in PR #1053 with a byte-level store interface: "go ahead there".

### Interpretation

- Option A: one Filestore share with two subdirectory mounts, the repository mount narrowed so it cannot reach attachments.
- "File path guard": names built from the organization's id and checked against the caller's tenant scope, independent of row-level security.
- No organization erasure and no sweep of unreferenced bytes until organization deletion exists; the per-organization directory makes it a single removal.

## Pull-request attachments and agent uploads (2026-11-03)

### Human instructions

- "Add support for 'g5d pr attachments'. Also command line support for --attach"

### Interpretation

- `g5d pr attachments` is the deferred half of the plan's CLI, now that #830 has landed.
- `--attach` is the plan's "agent uploads (`--attach`)": images from local files added to G5 issues by `g5d issue create`, `comment` and `edit`. GitHub issues stay without uploads.

## Where an attached image goes (2026-11-04)

### Human instructions

- After an image chosen with Attach image in the issue page's edit panel landed at the top of the description: "is it required to have the image in the description box? If so, bottom is better. If not, then leave it out of the description entirely if I never clicked"

### Interpretation

- It is not required: an attachment no text shows is listed at the issue. So in a description (the issue page's editor and the creation form) an image chosen with Attach image before the person has clicked or typed in the box is only attached.
- "Never clicked" is a click or keystroke in the box; focus alone (Tab, autofocus) does not place the caret.
- Comments, where an image belongs to the text, and images dropped on an unplaced box take the "bottom is better" branch: the end of the text.
