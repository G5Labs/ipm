# SEC-005: runnable stack secrets

The user requested fixing SEC-005, DEAD-004, DEAD-005 and DUP-001 through the previously invoked build-to-ship flow, allowing the agent to choose ordering and grouping. The starting checkout and isolated worktrees under `/Users/zliu/Workspace` were authorized.

SEC-005 reports a committed ES256 signing key reused by local/CI stacks and a fixed SCM encryption-key default. The independent audit classifies this as fixture/configuration hardening: production key reuse has not been established. Its correction notes that rejecting the fixture only when E2E login is disabled leaves copied E2E-enabled stacks exposed.

Agent interpretation: provision deployment-owned secrets for runnable stacks, reject the known public fixture identities at startup, and preserve secrets across restarts wherever encrypted data persists. This does not authorize rotating an existing user's credentials or deleting their database. Pure unit/integration fixtures may retain deterministic keys without becoming runnable deployment defaults.
