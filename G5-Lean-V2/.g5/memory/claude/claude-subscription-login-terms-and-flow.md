---
name: claude-subscription-login-terms-and-flow
description: "Why G5 cannot offer a \"Sign in with Claude\" OAuth button of its own, and what the in-VM `claude setup-token` flow actually looks like headless (URL + paste-back code, redirect to platform.claude.com)."
metadata:
  node_type: memory
  type: reference
  originSessionId: 21b1d0df-bfc4-4ef8-8aec-7d63e7c40d28
  modified: 2026-10-28T00:52:30.064Z
---

Anthropic's Claude Code legal page (https://code.claude.com/docs/en/legal-and-compliance, read 2026-10-28) says third-party developers may not offer Claude.ai login in their own apps nor "collect, store, or intermediate Claude.ai credentials or session tokens"; sign-in must complete through Anthropic's own flow. It explicitly allows an end user to sign in to the unmodified Claude Code binary hosted by a platform. Enforcement against third-party harnesses started 2026-01, tightened 2026-04-04.

Headless `claude setup-token` (CLI 2.1.308) under a PTY prints an authorize URL on claude.com with `client_id=9d1c250a-…`, `redirect_uri=https://platform.claude.com/oauth/code/callback`, `scope=user:inference`, PKCE, then waits at `Paste code here if prompted >`. There is no `--no-browser`/device-code option; without a PTY it prints nothing. `claude auth login` (`--claudeai` / `--console`) exists and stores the login in `$CLAUDE_CONFIG_DIR/.credentials.json`.

**Why:** the only terms-compatible "button" is the Codex-device-auth pattern (docs/codex-device-auth.md, backend/g5d-agent/src/codex_auth.rs, frontend CodexDeviceAuth.tsx): run the CLI's own login in the owner's VM, relay the URL and the paste-back code, keep the credential in the VM. A G5-owned OAuth client or central token store is what the policy forbids.

**How to apply:** when asked to "connect Claude automatically", design around the in-VM CLI login with code relay; do not propose a G5 OAuth client, and flag that the current centrally stored `CLAUDE_CODE_OAUTH_TOKEN` provider account is itself in the gray zone. See [[codex-has-no-credential-in-vm]].
