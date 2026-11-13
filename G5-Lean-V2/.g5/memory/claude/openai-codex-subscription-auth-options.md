---
name: openai-codex-subscription-auth-options
description: "OpenAI's documented ways to reuse a ChatGPT-plan Codex login across machines (seeded auth.json, Enterprise access tokens, Sign in with ChatGPT plan usage) and the conditions attached, researched 2026-10-28."
metadata:
  node_type: memory
  type: reference
  originSessionId: 21b1d0df-bfc4-4ef8-8aec-7d63e7c40d28
  modified: 2026-10-28T01:01:04.803Z
---

Read 2026-10-28 from learn.chatgpt.com/docs/auth, /docs/auth/ci-cd-auth, /docs/non-interactive-mode and developers.openai.com/siwc/*:

- Codex CLI 0.164 login options: browser, `--device-auth` (beta), `--with-api-key`, `--with-access-token` (ChatGPT Enterprise "Codex access tokens", admin-granted, for trusted private runners).
- Copying/seeding `auth.json` (`CODEX_HOME`) to a headless machine or VM is documented and allowed, "advanced": one machine or serialized job stream per copy, never shared across concurrent jobs, private infra only, Codex refreshes when `last_refresh` is older than ~8 days and the refreshed file must be persisted (not the seed). Refresh tokens: 30 days, replaced on each refresh; access tokens 1 h. The SIWC "self-hosted VMs" page describes sign-in on the laptop, then transferring the credential file to the VM, which owns later refreshes with its own `ext_agent_host_id`.
- "Sign in with ChatGPT" plan usage (DevDay 2026-09-29): open-source/locally hosted tools self-register with `client_id=dynamic_agent_client` and a `127.0.0.1` loopback redirect; paid or remotely hosted apps must file the interest form ("select group of commercial partners"). Plan-backed Codex runs via `codex app-server` with `env_key="ACCESS_TOKEN"`, `requires_openai_auth=false`, HTTP only, preview limitations (no tool_search, hosted tools).
- openai.com/policies/terms-of-use returned 403 to WebFetch; no third-party-login prohibition like Anthropic's was found in OpenAI's Codex docs.

**Why:** G5 retired `CODEX_AUTH_JSON` on 2026-09-23 (commit c6f3a3428) for per-VM device login with "tokens never go to the control plane"; the owner now finds per-VM login worse than the Claude token. OpenAI's own docs make an owner-scoped seeded credential defensible if copies are serialized.

**How to apply:** for "log in once for Codex", design an owner- or workspace-scoped encrypted `auth.json` lease (one live VM per copy, write back the refreshed file on release, re-login after 30 idle days); offer `--with-access-token` as a paste-once kind for Enterprise; treat SIWC plan usage as the long-term button pending OpenAI's interest form. See [[claude-subscription-login-terms-and-flow]] and [[codex-has-no-credential-in-vm]].
