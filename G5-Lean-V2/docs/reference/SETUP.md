# Local development setup

Run commands from the repository root. The application uses Docker for its local
services; workspace execution uses Linux/KVM or explicitly acknowledged host
processes. See [worker image setup](../../infra/packer/g5-worker/README.md) for the
production host platform and [local validation](../local-validation.md) for tests.

## Common dependencies

Install Docker, the repository's pinned Rust toolchain, Node.js, `just`, and `jq`.
Run `just setup` for frontend dependencies and Playwright browsers. Local settings
are documented in `.env.example`; provider credentials are configured in the app's
Runtime Config or provider account settings.

## macOS and local process development

Native macOS hardware-VM execution is not supported. Select and acknowledge local
process mode for both setup and startup:

```bash
G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 just worker-host-setup
G5D_ISOLATION_MODE=process G5D_ALLOW_PROCESS_ISOLATION=1 just dev-up
```

Setup builds the host binary and stages a host-native runtime from installed tools;
it reports missing tools by name. This mode shares the host filesystem and network,
with no VM isolation or resource caps. Use it only for local development.

## Linux microVM development

Cloud Hypervisor requires Linux x86_64 with accessible `/dev/kvm`. The host needs
cgroup v2 delegation, Landlock, AppArmor, the VMM/passt identities and network policy,
and an XFS reflink store. Provision those through the
[worker platform](../../infra/packer/g5-worker/README.md) or the
[self-hosted installer](../../infra/self-hosted/README.md). Building the VMM/kernel
artifacts alone does not configure those host boundaries.

Once the platform is installed:

```bash
just worker-host-setup
just dev-up
```

Setup validates the platform artifacts, builds the host binary and stages the
Linux guest runtime through Docker. Cloud Hypervisor, the pinned guest kernel and
initramfs are built by `scripts/build-cloud-hypervisor-runtime.sh`; the platform
installer supplies passt and the service configuration. Missing VM prerequisites
fail explicitly; there is no automatic switch to process mode.

## Troubleshooting

- Follow the missing-prerequisite error from `worker-host-setup`; re-run it after
  installing the named tool or platform artifact.
- A modern Docker image cannot replace an old host kernel. Landlock and cgroup-v2
  failures require a compatible host, including for CI. See the
  [CI worker requirements](../../infra/qa/README.md#ci-worker-operating-system).
- Use the same pinned compiler and `rustdoc` for builds and doctests. Rust
  compilation does not require a VMM installation; actual VM tests do.
