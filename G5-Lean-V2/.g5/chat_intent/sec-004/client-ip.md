# SEC-004 client identity

The human requested fixing SEC-002, SEC-003 and SEC-004 from the 2026-10-09 review, explicitly invoked build-to-ship, authorized `/Users/zliu/Workspace/G5-Lean` as the starting checkout, and required one finding per PR, sequentially.

Agent interpretation: SEC-004 requires that caller-controlled forwarding headers cannot create throttle identities and that request attribution agrees with throttling. Preserve distinct clients through the shipped Google ingress and self-hosted Caddy paths. A trusted proxy must authenticate its transport peer and guarantee the configured right-hand X-Forwarded-For suffix. CIDRs and suffix length are deployment authority, never request authority. The separate suggestion to add device-code quotas is not needed to repair this cause: CLI start has no device code yet, and poll already uses an unguessable one-time device credential; changing its quota policy is outside this bounded repair.
