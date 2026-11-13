---
name: full-stack-chat-e2e-in-vm
description: "Running real chat-turn Playwright specs in a KVM-less MicroVM — process-isolation dev-up, fake providers, skip the reseed, seed an org Anthropic key, hold turns with a gate file."
metadata:
  node_type: memory
  type: reference
  originSessionId: b76220f4-9843-40b2-9869-72aea99e8155
  modified: 2026-10-26T05:01:45.044Z
---

Verified 2026-10-26 (issue #823): full-stack chat E2E with real dispatch runs in this VM.

- Start: `G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 G5_CLAUDE_CODE_FAKE=1
  G5_CODEX_FAKE=1 G5_PORT_POSTGRES=15432 just dev-up` (no /dev/kvm: the default microvm
  host fails on the Cloud Hypervisor probe). Cold build ≈ 25 min; app :3000, API :7500.
- Run: `LLM_ENABLED=1 G5_DEV_E2E_SKIP_SEED=true just dev-playwright e2e/flows/<spec> --project=chromium`.
  Without SKIP_SEED the fixture refresh fails deleting the dev-up-seeded e2e-test (403).
- Claude launches need a credential even with the fake: specs call
  `seedWorkspaceClaudeCredential`, or seed the org like CI's `seed-ci-org.sh`:
  `POST /api/v1/auth/test-token {"email":"demo@g5labs.ai","orgId":<g5labs id>}` then
  `PUT /api/v1/orgs/<id>/config {"setSecrets":{"ANTHROPIC_API_KEY":"sk-ant-ci-test-key"}}`.
  `chat.spec` and `personal-workspace` fail with `missing_llm_credential` until then.
- Hold a turn: `G5_WAIT_FOR_FILE_E2E:<member root_path>/<gate>`; Codex only waits when
  the prompt also has a `G5_FS_WRITE_SET_E2E` write. Release with
  `PUT /api/v1/workspaces/:id/files/<gate>?repoId=…`.
- A new tab opens on the no-chat landing: its picker trigger is `chat-center-history`,
  not `chat-active-history`.

Related: [[native-stack-browser-e2e]], [[in-vm-preview-runs-dev-up]], [[rust-and-integration-test-setup]].
