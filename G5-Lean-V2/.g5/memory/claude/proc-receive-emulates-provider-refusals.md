---
name: proc-receive-emulates-provider-refusals
description: "To make a local bare remote refuse a push with a custom \"[remote rejected] (reason)\", use a proc-receive hook; pre-receive only yields \"pre-receive hook declined\"."
metadata:
  node_type: memory
  type: reference
  originSessionId: cf1cebff-6242-46ae-be8b-f44e43b7690d
  modified: 2026-10-20T06:13:41.915Z
---

A pre-receive hook's refusal reaches the pusher only as `[remote rejected] (pre-receive hook declined)`.
To reproduce a provider's exact wording (e.g. GitHub's "refusing to allow a GitHub App to create or
update workflow … without `workflows` permission"), set `receive.procReceiveRefs = refs/heads` in the
bare repo and install `hooks/proc-receive`: it speaks pkt-line (version=1 handshake, commands, then
`ok <ref>` / `ng <ref> <reason>`) and must `git update-ref` accepted refs itself. Perl is always present
on Ubuntu images (perl-base is essential), so the hook is written in Perl.

`g5_scm::native::emulate_github_workflow_permission_for_tests` (added for #612) is a reusable example;
`MockedGithubApp::enforce_workflow_permission` in `backend/g5-api/tests/helpers/github_app.rs` ties it to
minted installation-token permissions. Related: [[g5-api-test-binaries-fill-disk]].
