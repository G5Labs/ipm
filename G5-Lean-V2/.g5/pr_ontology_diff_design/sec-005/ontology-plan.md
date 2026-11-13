# SEC-005: deployment-owned stack secrets

## Ontology changes

The [security diff](../../ontology/concepts/security.g5.md#keep-public-fixture-keys-out-of-runnable-deployments) adds two commitments: reject public fixture identities at deployment startup, and retain credentials for the lifetime of their data. The [request record](../../chat_intent/sec-005/stack-secrets.md) separates the authorized hardening from the unproved possibility of production reuse.

## Governing ontology

- [Reserved platform identity](../../ontology/concepts/security.g5.md#protect-platform-owned-environment-keys) keeps these credentials outside user task settings.
- [Local validation receipts](../../../docs/local-validation.md#developer-loop) bind effective configuration before the freshness snapshot. [Exact candidate qualification](../../ontology/concepts/deployment.g5.md#qualify-the-exact-release-candidate) separately requires fresh Rust/E2E CI after the change.
- [Deployment lanes](../../ontology/concepts/deployment.g5.md#keep-the-retained-deployment-lanes-clear) retain SaaS and self-hosted operation; existing production secret stores/installers remain the configuration owners.

## High-level design

| Owner | Responsibility |
|---|---|
| Local stack bootstrap | Load complete explicit credentials or a shared host-local bundle outside checkouts. Serialize first creation, validate before publishing, and refuse first generation over existing G5 data. Existing Compose service/project/validation labels and G5 network identity distinguish retained G5 containers from unrelated projects sharing generic Compose filenames. All local API/indexer launch and restart paths consume the same bundle. |
| CI job bootstrap | Generate once in a private job volume outside the source/build context; provide the same bundle to the three API replicas and container restarts. The self-hosted smoke run also generates one matching JWT pair and a fresh SCM key for its complete install/upgrade/restart lifecycle. |
| API startup | Use the existing authentication crypto implementation to reject the known fixture's cryptographic identity in either signer or verifier configuration, plus the fixed SCM key, before database access. The E2E flag grants no exception. |
| Validation runner | Bind effective configuration before recording candidate/reuse receipts; provisioning failures leave prior state intact. |

The existing `.env`/environment interface remains the explicit operator configuration. Automatic local credentials need a shared owner because `g5-lean_pgdata_dev` is shared across checkouts; per-checkout `.env` generation could replace the key over the same database after a worktree switch. The shared bundle is the only additional durable local fact. No database marker or secret-rotation service is added. Per-container generation is rejected because restart would invalidate JWTs and make retained ciphertext unreadable.

The existing ES256 generation primitives remain the source for new P-256 PKCS#8 keys. Self-hosted install-time `.env` rewriting is not reused as the local lifecycle owner: it is customer-install-specific, overwrites nonatomically, and does not bind the shared dev database. Missing or damaged retained key material requires restoration or deliberate operator migration, never automatic regeneration. Startup diagnostics and operator documentation must explain that existing fixture-encrypted data cannot be preserved by merely substituting a new key; this change does not migrate or delete that data.

## Decision notes

The binary keeps its existing optional-key configuration policy; the startup guard validates configured keys. Standard local and CI bootstraps supply the complete credential set. Independent review exposed an implicit fixture verifier in the HTTP “reject-all” fallback. This shares the public-fixture root cause and must be closed: an absent verifier is represented by no decoding key in the existing validator, so every JWT acceptance method rejects it. The HTTP composition root installs the same validator in AppState and outer middleware before router construction, so Git Basic/Bearer and both query-token WebSocket handlers reject unauthorized credentials consistently. With no decoding key, outbound preview classification retains the prior fail-closed behavior and strips every bearer value; installing Some(reject_all) must not relax the prior None policy. No dummy or newly generated fallback identity is introduced.

The same review identified direct native E2E recipes as additional signer consumers. Local loaders export the E2E signer alias with the other aliases; the lifecycle-managed recipe loads it before launching its stack, while the already-running-stack recipe honors an explicitly supplied signer for an external stack. The already-running dev wrapper and live-LLM recipe are also signer consumers; load after their effective .env/.env.test inputs and before invoking the E2E binary. Fixed scenario runners that cannot reach host-loss recovery do not gain a signer dependency. The bundle remains the sole retained fact.

Pure test harness keys stay deterministic where they cannot become stack defaults. Startup checks use cryptographic identity so PEM whitespace or an equivalent accepted encoding does not bypass the fixture rejection. Provisioning must distinguish absent credentials from partial overrides and must not write a managed bundle before validating an explicit configuration or existing-data guard. Publication flushes file content and the containing directory chain through its ancestors before credentials can be consumed, including directories created concurrently or by a failed earlier initializer. This avoids a separate durable initialization marker. A directory-flush failure retains the valid file and refuses export; retries revalidate and flush that same bundle instead of rotating it. Repeated startup, competing initializers, partial files, failed generation and retained validation stacks are the discriminating lifecycle cases.

The common design review compared per-container and per-checkout generation with the actual fixed-volume owner and rejected both. The chosen shared bundle preserves the existing restart/data lifetime without introducing migration state. No model checking was requested. Implementation follows these owners. Local tests exercise real key generation, competing initialization, refusal/recovery cases, actual CI launch scripts, Compose interpolation, actual API/indexer startup and SCM ciphertext decryption after reload. Removing either the startup guard or the main CI replica’s secret loader causes the corresponding regression to fail. The scoped security metadata records executed evidence and the unobserved deployment/migration boundaries.
