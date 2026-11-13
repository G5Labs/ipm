# Worker Host API Surface

> Status: proposed architecture (module owner: kaihui)
>
> Purpose: define the narrow, debuggable interface of the worker host: the machine that boots workspace VMs, relays their channels, and holds every credential the VM must never see. This note is the host-side companion of [workspace-vm-api.md](workspace-vm-api.md) (the VM and dispatcher surface) and refines [G5-Lean backend design, draft 6](lean-backend-design-v6.md) §5.2, §6.1, §6.2, §6.6, §6.8 and plan task P4.4.

## 1. Boundary

A worker host is a slot provider. It runs `g5d host`, dials one outbound tunnel to the control plane, and boots, stops and relays workspace VMs on command. It has no task logic, no knowledge of branches or approvals, and no opinion about what runs inside a VM.

| Place | Owns |
|---|---|
| Control plane (g5-api) | The host registry, placement, org quotas, VM incarnations (`workspace_vms`), workspace tokens, provider credentials at rest, the stop sequence and its durable cut. |
| Workspace VM lifecycle service (control plane, `WorkspaceVmService`) | The only control-plane client of the `ctl` stream: it issues `vm.start`, `vm.stop`, `vm.status`, `vm.grants.update` and `host.drain`, and consumes `host.hello`, `host.heartbeat`, `vm.event`, `lease.renew`, `host.credential.renew` and `host.drained`. It owns `workspace_vms`, placement, the stop sequence and the incarnation fence. |
| Workspace Runtime facade | The only control-plane client that opens VM channels (`rpc`, `pty`, `http`) through a host, after the lifecycle service has made the incarnation current. Other platform systems call the facade; nothing else reaches a host tunnel. Both clients share one tunnel registry (`host_id → connection`). |
| Worker host (`g5d host`) | Equal-size VM slots, libkrun boot and stop, OCI rootfs templates, the host workspace root and bare-repo cache, passt networking and port forwards, the stream multiplexer, the per-VM lease timer, the LLM and credential proxies, host-local observations. |
| Workspace VM (`g5d dispatch`) | Everything inside the VM. See workspace-vm-api.md. |
| Git service | Refs and policy. The host caches bare repos as an optimization and never touches a ref. |

Every host command is bound to a `host_id` and a valid host credential; every VM-scoped command is additionally bound to `(workspace_id, vm_id)`, a deadline, and an idempotency key. A command naming a `vm_id` the host does not run, or that the control plane no longer holds as current, is answered `stale`, never acted on.

## 2. Host registry and lifecycle

### 2.1 Host states

```
provisioning ──registered + first heartbeat──► ready ──drain requested──► draining ──every VM released──► terminated
      │                                          │                              │
      └──never registered / bring-up abandoned───┴──heartbeat missing > 3 intervals──┴──► unreachable ──credential expired──► terminated
```

`unreachable` is new: it names the window in which the control plane has lost the tunnel but the host may still be running VMs. During it the control plane places nothing on the host, keeps every incarnation current until its lease expires (§5), and expects a reconnect with an inventory (§3.3).

### 2.2 Host record

```
worker_hosts(
  host_id PK, org_id, region,
  kind                 gce | self_hosted | macos_dev,
  status               provisioning | ready | draining | unreachable | terminated,
  slots_total          int,                -- G5D_HOST_MAX_MICROVMS
  slot_cpus, slot_mem_mib, scratch_disk_mib,
  runtime_identity     jsonb,              -- g5d version, base image digest, libkrun, libkrunfw, kernel, providers[]
  capabilities         text[],             -- isolation:libkrun-microvm, agent_provider:claude_code, agent_provider:codex, net:passt, …
  credential_expires_at, drain_requested_at, last_heartbeat_at, last_error,
  cloud_instance_id, provision_error_code, provision_retry_count  -- GCE path only, as today
)
```

Slot occupancy is not a column. It is `count(workspace_vms where host_id = ? and status in (starting, running, stopping))`, so placement and the host can never disagree about a slot that a crashed transaction left half-booked.

### 2.3 Operator API

Exposed by g5-api to org operators; the SaaS provisioning path keeps today's GCE bring-up and reconciler.

| Method | Contract |
|---|---|
| `POST /orgs/:org/worker-hosts` | Provision a host (GCE) or mint a registration token for a self-hosted or macOS host. Returns `host_id` and, for self-hosted, a one-time `g5w_*` token. Exists today. |
| `GET /orgs/:org/worker-hosts` | Registry rows plus derived occupancy and the last heartbeat summary. Exists today. |
| `POST …/:host/drain` | Request drain. The control plane runs the stop sequence for every VM on the host, sends `vm.stop` for each, and marks the host `draining → terminated`. Returns immediately with a drain id; progress is on the host record. New; today drain exists only inside the rotation loop. |
| `DELETE …/:host` | Drain, then terminate the instance (GCE) or forget the registration (self-hosted). Refused while any VM is `running` unless `force=true`, which records the loss per VM. Exists today without the drain; the drain is required because v6 VMs hold uncommitted work. |

Today's `POST …/:host/runtime-labels/refresh` is not carried over in this draft; whether the host keeps a per-host image identity at all is an open question (§13). Per-host VM listings come from `workspace_vms` by `host_id`; there is no separate route.

## 3. Host ↔ control plane protocol

### 3.0 Versioning

Hosts, VMs and the control plane deploy independently, and a VM can live across several control-plane deploys, so every message on this protocol is versioned. This is the fix for the release-channel skews seen today (G5 #1860, #1883).

- `host.hello` carries `protocol_version` (integer) and the host's `runtime_identity`. The control plane answers `host.accept {protocol_version}` with the version it will speak, which is at most the host's. If it supports no version the host offers, it answers `host.reject {reason: unsupported_version, supported: [...]}`; the host then drains itself (refuses `vm.start`, keeps running VMs, keeps lease timers) and retries the tunnel with backoff, so an old host never runs new commands it cannot parse.
- Within a version, unknown fields are ignored on both sides and required fields never change meaning. Adding a field, a `vm.event` kind, or a result variant is a minor change; removing or renaming anything, or changing a result's semantics, is a new `protocol_version`.
- `VmSpec` carries `spec_version`. A host that does not know a `spec_version` answers `spec_rejected(unsupported_spec_version)`; placement then excludes that host for that workspace instead of retrying it.
- Deploy order follows from the above: control plane first (it must accept both the old and the new host version), hosts second, and a host is only drained for a runtime upgrade, never for a protocol upgrade it already speaks.

### 3.1 Transport

One outbound WebSocket per host, `GET /api/internal/hosts/:host_id/tunnel`, authenticated with the **host credential**: a renewable token with claims `host_id`, `org_id`, `runtime_identity_hash`, TTL 24 h, renewed by the host through `host.credential.renew` from 2 h before expiry. This replaces the fixed 24-hour host JWT that forced rotation of long-lived hosts.

The socket carries the existing credit-flow stream multiplexer. Streams are tagged:

| Stream tag | Initiator | Payload | Bound to |
|---|---|---|---|
| `ctl` | host, exactly one per tunnel | JSON-RPC 2.0 both ways: commands in §3.2, messages in §3.3 | host |
| `rpc` | control plane | the dispatcher's JSON-RPC plus its pushed events, verbatim (workspace-vm-api.md §2, §4) | `vm_id` |
| `pty` | control plane | raw bytes of one terminal, after a `terminal.open` on `rpc`; one stream per terminal | `vm_id`, terminal id |
| `http` | control plane | raw TCP to a guest port, for preview and port forwarding; **one stream per TCP connection**, so a browser page opens several streams to the same port and a hot-reload WebSocket is one long-lived stream | `vm_id`, port |

Opening a VM stream carries `(workspace_id, vm_id)` and, for `http`, the target port in the stream header. The host resolves the VM's forwarded listener port and per-VM listener token, dials the guest listener through passt once per stream, and copies bytes both ways until either side closes; nothing is multiplexed inside a stream, the mux is what multiplexes streams over the one WebSocket. A stream for a `vm_id` the host does not run is refused with `stale` and closed. There is no `llm` stream: model traffic never crosses the tunnel (§6).

### 3.2 Commands, control plane → host

All commands are request and response on `ctl`, idempotent on `(command, idempotency_key)`, and answered within the caller's deadline or with `timeout`. A `timeout` is not a negative result: the host may still be executing the command, and the caller re-issues the same command with the same idempotency key until it receives a definite result. The host keeps the result of every completed VM-scoped command for at least `result_retention_secs` (default 600) so a retry after a tunnel drop gets the original result, not `not_found`.

| Command | Arguments | Result |
|---|---|---|
| `vm.start` | `workspace_id, vm_id, spec` (§4) | `started` \| `already_running` \| `slot_unavailable(reason: slots \| disk \| boot_capacity \| workspace_busy)` \| `spec_rejected(reason)` \| `boot_failed(reason, log_tail)`. `workspace_busy`: a VM of the same `workspace_id` with a different `vm_id` is still on this host (not yet exited), and the two would share one workspace root; the control plane places elsewhere or waits for that VM's `exited`. `boot_capacity`: more than `max_concurrent_boots` (default 2) boots are in progress; placement retries after backoff. |
| `vm.stop` | `workspace_id, vm_id, mode: graceful \| kill, grace_secs` | `stopped(exit)` \| `not_found` \| `stale`. `graceful` delivers SIGTERM to the guest init process, which is the dispatcher, and SIGKILLs at `grace_secs`; `kill` kills now. No rpc method is involved: by the time `vm.stop` is sent the workspace stop sequence has already run on `rpc` (admission barrier, `workspace.sync` with `reason: shutdown`), and how the dispatcher exits on SIGTERM is the workspace module's concern. The host frees the slot only after the VMM process has exited, and keeps the workspace root and scratch disk for `spec.workspace_root.retain_hours`. A `vm.stop` for a VM that already exited returns `stopped(exit)` from the retained result while it is retained, and `not_found` only after that. |
| `vm.status` | `workspace_id, vm_id` | `{state: booting \| ready \| running \| stopping \| exited, boot_at, ready_at, resources, lease_expires_at, streams_open, sessions}` (`sessions` as in §6.2) |
| `host.drain` | `deadline` | `accepted`. The host stops admitting `vm.start` and reports `drained` (§3.3) when its last VM exits. Stopping the VMs is the control plane's job; the host never stops a VM on its own except for lease expiry. |
| `vm.grants.update` | `workspace_id, vm_id, credential_grants` (the full replacement list, same shape as `VmSpec.credential_grants`) | `applied` \| `stale` \| `grant_unresolved(kind, target)`, in which case the previous table stays in force. The host resolves the new references, then swaps the proxy's grant table for that VM atomically: requests already in flight finish with the credential they started with, the next request uses the new one. Nothing inside the VM changes, no process restarts, no session is lost, because no provider process ever held the credential. This is how a user changes an API key, switches from a key to a gateway, or an org rotates a token in the middle of a chat. |

Host configuration (slot count, log level, proxy settings) is process configuration read at start, as today; changing it restarts `g5d host` after a drain. There is no inspect command: `vm.status`, `vm.event` and the host's structured logs are the observability surface (§9).

### 3.3 Messages, host → control plane

| Message | When | Payload |
|---|---|---|
| `host.hello` | first message on every tunnel connect | `host_id, protocol_version, runtime_identity, capabilities, slots_total, inventory: [(workspace_id, vm_id, state, boot_at)], retained_roots: [(workspace_id, expires_at)]`. The control plane answers `host.accept {protocol_version, current_incarnations: [(workspace_id, vm_id)], config}` or `host.reject` (§3.0). The host kills every VM in its inventory that is not in `current_incarnations`, without a stop sequence, then replays its unacknowledged `vm.event` journal. A `vm.start` the control plane sent just before a drop is resolved by the inventory: listed means it booted, absent means the same command is re-sent. |
| `host.heartbeat` | every `heartbeat_secs` (default 10) | `slots_in_use, boots_in_progress, load, memory_free_mib, disk_free_mib, retained_roots_count, proxy_upstream_errors, vms: [(vm_id, state, lease_remaining_secs, cpu_pct, mem_mib)]`. Three missed heartbeats move the host to `unreachable`. The acknowledgement carries only the current-incarnation set for the proxies (§5); it does not extend any lease. |
| `vm.event` | on every VM state change | `vm_id, seq, kind: booted \| ready \| exited{code, signal} \| oom \| lease_expired_killed \| stale_killed \| boot_failed{reason}, at, log_tail?`. `ready` is forwarded from the dispatcher's `runtime.ready` and is what flips `workspace_vms` to `running`. Events are **journaled on the host until acknowledged**: `seq` is monotonic per `vm_id`, the control plane inserts on the unique key `(vm_id, seq)` and answers `vm.event.ack {vm_id, up_to_seq}`, and after a reconnect the host replays every unacknowledged event before it sends anything else. A `vm.event exited` therefore cannot be lost to a tunnel drop, which is what keeps a slot from leaking. `log_tail` is capped at 64 KiB and has env values redacted. |
| `lease.renew` | forwarded from the dispatcher's `lease.renew` on `rpc` (workspace-vm-api.md §4, same name end to end); the dispatcher sends it at half the lease TTL | `workspace_id, vm_id`. The control plane extends `workspace_vms.lease_expires_at` and answers `{token, lease_ttl_secs}` or `stale`. The host resets its local timer to `lease_ttl_secs` from receipt (a duration, so host and control-plane clocks never need to agree) and passes the token through as an opaque blob to the dispatcher, which checks that its `vm_id` claim matches before installing it. On `stale` the host kills the VM and emits `vm.event stale_killed`. |
| `host.credential.renew` | from 2 h before the host credential expires | The control plane answers with a new host credential, or `refused` when the host is `draining`, `terminated`, or its runtime identity no longer matches. The old credential stays valid until its expiry, so renewal never interrupts a tunnel. |
| `host.drained` | after `host.drain`, when no VM remains | `drain_id` |

Disk and memory pressure are heartbeat fields (`disk_free_mib`, `memory_free_mib`), not a separate alert; the control plane alerts and stops placing on the host when they cross its thresholds.

## 4. VM start spec

The control plane sends everything the host needs to boot one incarnation. The host adds nothing from its own state except the base image files and the cache paths.

```
VmSpec {
  spec_version:  1,
  incarnation:   { workspace_id, vm_id, org_id, lease_ttl_secs, idle_timeout_secs },
  image:         { ref, digest },                       -- OCI, must be in the host cache or fetchable
  size:          { cpus, mem_mib, scratch_disk_mib },   -- must equal the host's slot size
  workspace_root:{ retain_hours },                      -- host path is host-chosen: <workspaces_root>/<workspace_id>
  repos:         [{ repo_id, root_path, git_url, branch }],   -- written to /work/.g5/layout.json for the dispatcher
  env:           { G5D_WORKSPACE_ID, G5D_VM_ID, G5D_WORKSPACE_TOKEN, G5D_API_URL, HOME, PATH, provider paths, G5D_LLM_PROXY_URL },
  network:       passt,                                 -- the only mode; see §13 for the decision
  entrypoint:    ["/opt/g5/bin/g5d", "dispatch"],
  credential_grants: [                                   -- consumed by the host proxies (§6), never mounted
    { kind: llm_provider, provider: anthropic | openai | gateway, secret_ref },
    { kind: git_https,    target_host, source: workspace_token | secret_ref },
    { kind: gh,           target_host, secret_ref }
  ]
}
```

Rules the host enforces before boot, in this order; the first failure is the answer:

- `spec_version` is one the host knows, else `spec_rejected(unsupported_spec_version)`.
- `incarnation.org_id` equals the `org_id` in the host credential, else `spec_rejected(org_mismatch)`. A host serves one org.
- `workspace_id` and `vm_id` are UUIDs and every `repos[].root_path` is a relative path with no `..` or symlink component, else `spec_rejected(invalid_path)`. The workspace root is `<workspaces_root>/<workspace_id>` and nothing in the spec can name a host path.
- `size` equals the slot size, a slot is free, no other VM of this `workspace_id` is on the host, and fewer than `max_concurrent_boots` boots are in progress, else `slot_unavailable(reason)`.
- `image.digest` matches the pulled or cached image, else `spec_rejected(image_digest_mismatch)`.
- `env` contains only keys on the host's allowlist and no value that matches a credential pattern; otherwise `spec_rejected(credential_in_env)`.
- `credential_grants` name secrets by reference. The host resolves them through the control plane on boot and keeps the values in memory only; a grant that cannot be resolved is `boot_failed(grant_unresolved)`.
- The workspace token in `env` carries `vm_id == incarnation.vm_id` and `workspace_id == incarnation.workspace_id`; a mismatch is `spec_rejected(token_mismatch)`.

Mounts are fixed: `/work` is the host workspace root over virtio-fs; `/opt/g5/bin` is the runtime directory, read only; provider assets are read only; the scratch disk is a block device for docker and build caches. The bare-repo cache is host-internal: the dispatcher's restore clones from it when present and from the Git service otherwise, and never depends on it. After `vm.stop` the host keeps the workspace root and scratch disk for `workspace_root.retain_hours` so a resume placed on the same host is warm; placement prefers such a host (§7).

## 5. Incarnation, lease and token

The host is one of the boundaries that make a superseded VM harmless. It enforces three things and nothing more.

| Mechanism | Host responsibility |
|---|---|
| Workspace token | Minted by the control plane in the spec. The dispatcher renews through `lease.renew` on `rpc`; the host forwards it on `ctl` and returns the answer. A `stale` answer kills the VM. The host never mints, reads or caches token contents. |
| Lease | A host-local timer per VM, set from `lease_ttl_secs` at boot and reset only by a successful `lease.renew` answer, never by a heartbeat acknowledgement. Expiry kills the VM and emits `lease_expired_killed`. Both sides therefore run the same clock: the control plane's `lease_expires_at` and the host's timer are extended by the same message and only by it, so a dispatcher that stops renewing is lost on both sides at the same moment, and a partitioned host has already killed the old VM by the time the control plane can place a replacement. |
| Current-incarnation set | Refreshed on `host.accept` and on every heartbeat acknowledgement. Used only by the proxies (§6) to refuse a VM whose `vm_id` is no longer current. |

A draining incarnation (`stopping`) is current. The host keeps relaying its streams and renewing its lease until `vm.stop` arrives.

## 6. Host-side proxies

Provider keys, git tokens and the workspace token stay on the host. The VM reaches the outside world through one host-side service, reachable from inside the guest at the fixed name `g5-llm`, which serves both the LLM proxy and the credential proxy in §6.4. For model traffic the proxy terminates the provider CLI's HTTP, attaches the provider credential from the VM's `credential_grants`, forwards to the provider or the org's gateway, and returns the response. It refuses with 403 when the VM's `vm_id` is not current, tags every request with `(workspace_id, vm_id, session_id)` from the per-session path in §6.1, exposes the per-session view on the local status endpoint in §6.2 and on `vm.status`, and records usage per session for accounting.

**How the guest reaches it.** The proxy is a host process with one listener per VM on a per-VM port on host loopback. The guest has a real NIC (`10.85.0.2`) served by a per-VM passt process, and reaches host loopback through the passt door `169.254.1.2`, which the host seeds into the guest's `/etc/hosts` as `g5-llm`, the way today's `g5-host` alias is seeded. The spec env carries `G5D_LLM_PROXY_URL=http://g5-llm:<port>`.

Because every VM's passt maps the door to the same host `127.0.0.1`, the host cannot tell VMs apart by source address. The listener is therefore guarded by a **per-VM proxy secret** that the host mints at boot and passes only to the dispatcher (`G5D_LLM_PROXY_SECRET`); the dispatcher includes it in the proxy configuration it gives every provider process, and the proxy refuses a wrong or missing secret with `403 wrong_vm`. The secret is useless outside the VM and grants nothing but access to that VM's own proxy, so it does not violate the no-credential rule. Whether passt can instead map the door to a per-VM host address, which would make the secret defense in depth only, is an open question (§13). The listener closes when the VM stops.

This is a change from today, where the LLM proxy runs inside the guest on a unix socket (`/run/g5/llm.sock`) and the provider credential is handed into the guest to start it. Under v6 no provider credential enters the VM, so the proxy has to live on the host.

**Changing a credential while a VM runs.** Grants are references, so the value behind one can change. The control plane pushes the new list with `vm.grants.update` (§3.2) and the host swaps the proxy's table atomically; in-flight requests complete with the old credential, the next request uses the new one. Today this works only at a task boundary, because the in-guest proxy reads its credential once at boot; on a warm sticky-runtime turn the old key stays in use until the runtime is torn down. Here it takes effect on the next model request of a resident session with no restart. The proxy also re-resolves a grant once on an upstream `401` before surfacing the error, so an org-level rotation the control plane has not pushed yet still recovers on its own.

§6.1 to §6.4 are not in v6. They are the contract the provider-layer design ("Signals, Not Silence") needs from the host: per-session tagging and the local status endpoint for its liveness rule, and the credential proxy so that no credential, including the workspace token, is present in a provider process.

### 6.1 Per-session request tagging

The dispatcher, not the host, decides which process belongs to which session. It does so by giving every provider process a base URL that carries the session id on the shared per-VM socket:

```
ANTHROPIC_BASE_URL = http://g5-llm/s/<session_id>        # Claude Code
OPENAI_BASE_URL    = http://g5-llm/s/<session_id>/v1     # Codex, and any OpenAI-compatible client
```

`g5-llm` is the host-side listener described above (`G5D_LLM_PROXY_URL`). The proxy strips the `/s/<session_id>` prefix before forwarding, records `session_id` on the request, and rejects a request with no session prefix (`400 missing_session`) so an untagged client cannot hide inside the VM's accounting. `session_id` is opaque to the host; it is the dispatcher's key for a chat session or a one-shot task.

### 6.2 Local status endpoint

The same listener serves one read-only endpoint, reachable only from inside that VM, so the dispatcher can see what the proxy sees without a round trip through the control plane:

```
GET http://g5-llm/_g5/status
→ {
    vm_id, current: true | false,
    sessions: [
      { session_id, requests_open, last_request_started_at, last_response_at,
        input_tokens, output_tokens, cost_usd }
    ]
  }
```

`requests_open` is the number of upstream model requests in flight for that session. `current` is the host's cached answer to "is this incarnation still current"; a `false` here means the proxy is already refusing traffic and the dispatcher should expect `vm.stop`. The endpoint never returns credentials or request bodies. The control plane sees the same `sessions` array on `vm.status`.

### 6.3 Refusal codes

| Situation | Proxy answer | Meaning for the client |
|---|---|---|
| `vm_id` not current | `403 {"error":"stale_incarnation"}` | The VM has been superseded; stop retrying |
| Request without `/s/<session_id>` | `400 {"error":"missing_session"}` | Misconfigured launch environment |
| Credential grant missing for the target | `403 {"error":"no_grant"}` | The spec did not grant this provider or host |
| Upstream failure | the upstream status and body, unchanged | Provider-side error; surfaced to the model as today |
| Request body over `proxy_max_body_bytes` (default 32 MiB) | `413 {"error":"body_too_large"}` | Bound on host memory per request |
| Wrong or missing proxy secret (passt) | `403 {"error":"wrong_vm"}` | A process is talking to another VM's proxy |

### 6.4 Credential proxy for git and gh

The same host service is the egress for `git` over HTTPS and for `gh`, so that git and platform credentials are never present in the VM either. It is swap-on-access:

| Behavior | Detail |
|---|---|
| Inject on access | A request to a host named in `credential_grants` leaves the VM with no `Authorization` header; the proxy attaches the real credential on the way out. `git push` to the internal Git service and to a connected GitHub or GitLab repository needs no token in the VM. |
| Placeholder for gating clients | Clients that refuse to run without a local token, `gh` above all, receive a random single-use placeholder (`g5_cred_…`) in their environment, bound to one host. The proxy swaps it for the real credential on that host only. |
| Refuse elsewhere | A placeholder presented to any other host, or a request to a host with no grant, is refused with `403 no_grant`. A request from an incarnation that is not current is refused with `403 stale_incarnation`, the same fence as the LLM proxy. |
| No clobbering | A real `Authorization` header a client set itself is forwarded untouched. |

### 6.5 How each client is steered to the proxy

One host process and one listener per VM serve model traffic, git and gh alike; what differs is how each client is pointed at it, because their configuration surfaces differ.

| Client | Steering | TLS inside the VM |
|---|---|---|
| Claude Code, Codex | `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` = `http://g5-llm:<port>/s/<session_id>` (§6.1). Plain HTTP inside the VM; the proxy re-originates over HTTPS. | none needed |
| git over HTTPS | URL rewrite in a dispatcher-managed global gitconfig: `url."http://g5-llm:<port>/git/github.com/".insteadOf "https://github.com/"`, one rule per granted host including the internal Git service. git sees a plain-http remote; the proxy recovers the real host from the path, re-originates over HTTPS and injects the `git_https` credential. | none needed |
| gh, and any client with a hard-coded API host | `HTTPS_PROXY=http://g5-llm:<port>` plus a **per-VM CA** installed in the VM's trust store; the proxy terminates TLS for `api.github.com` with a certificate issued under that CA, injects the `gh` credential, and re-originates. `GH_TOKEN` holds the placeholder so gh agrees to run. | yes: L7 termination under a CA the host minted for this VM |

The per-VM CA's private key exists only in host memory; the VM holds the public certificate. It cannot sign anything, cannot be used from another VM, and dies with the incarnation. Any future client with a hard-coded host (a GitLab CLI, a package registry client) uses the same path without new mechanism.

Grants for this proxy are the `git_https` and `gh` kinds of `VmSpec.credential_grants`. The internal Git service grant is derived from the incarnation itself (`source: workspace_token`), so the workspace token reaches the Git service only from the proxy and never from a provider process. The dispatcher keeps `G5D_WORKSPACE_TOKEN` in its own environment for its calls to the control plane and does not pass it to the processes it launches.

## 7. Placement and capacity

Placement is a control-plane decision; the host only accepts or refuses. The control plane picks, in order: a host in the org's region with a free slot that still holds this workspace's root and scratch disk (`retain_hours` not yet elapsed, from the host's `retained_roots`), then any host in the region with a free slot, then `no_capacity`, which the workspace module surfaces to the user and to a queue. Org quota is `max_running_vms` and is enforced before `vm.start` is sent. A host that answers `slot_unavailable` despite the registry's count is treated as authoritative and the control plane re-places; `workspace_busy` in particular means the previous incarnation of the same workspace has not exited on that host yet.

Retained roots are a cache, not a promise. Under disk pressure the host evicts retained roots oldest first before refusing `vm.start` with `slot_unavailable(disk)`, and `retained_roots` on the next heartbeat reflects it; a resume placed on a host that evicted the root simply restores from Git.

### 7.1 Limits

Every limit below is a host configuration value with a default; exceeding it is a definite refusal, never a hang.

| Limit | Default | Exceeded → |
|---|---|---|
| `slots_total` | `G5D_HOST_MAX_MICROVMS` | `slot_unavailable(slots)` |
| `max_concurrent_boots` | 2 | `slot_unavailable(boot_capacity)` |
| `max_pty_streams_per_vm` | 8 | stream refused with `too_many_streams` |
| `max_http_streams_per_vm` (concurrent TCP connections across all forwarded ports) | 256 | stream refused with `too_many_streams`; the browser sees a failed connection, not a hang |
| `max_inflight_commands` on `ctl` | 16 | command refused with `busy`; the caller retries with backoff |
| `max_ctl_message_bytes` | 1 MiB | message dropped, `vm.event`-style error on `ctl`; a `VmSpec` over the limit is `spec_rejected(too_large)` |
| `proxy_max_body_bytes` | 32 MiB | `413 body_too_large` |
| `result_retention_secs` | 600 | retries after this get `not_found` |
| `log_tail_bytes` | 64 KiB | tail truncated from the front |

## 8. Drain and rotation

Rotation is drain and replace. The control plane provisions the replacement first, then sends `host.drain`, then for each VM on the host runs the workspace stop sequence to its durable cut (the receipt in workspace-vm-api.md §5) and sends `vm.stop`. A VM whose stop sequence fails keeps the host `draining` and is retried with backoff; an operator force-drain records the loss per VM. `host.drained` is followed by instance termination. Host credential renewal is independent of drain and never interrupts a tunnel (§3.3).

## 9. Observability

| Surface | Caller | Returns |
|---|---|---|
| `host.hello`, `host.heartbeat` | control plane | Runtime identity, capabilities, slot occupancy, load, disk and memory free, per-VM liveness and lease freshness. What the operator surface renders. |
| `vm.status`, `vm.event` | control plane | Per-VM lifecycle, resource usage, lease freshness, open streams, per-session `requests_open`, boot log tail on `boot_failed` and `exited`. |
| Host logs | operators | Structured, per host, shipped as today. VM console output up to `ready` is captured into the boot log tail; after `ready`, VM logs belong to the dispatcher's journal. |

There is no host shell and no inspect command; anything an operator needs beyond the above is a field added to `host.heartbeat` or `vm.status`.

## 10. Failure matrix

| Failure | Detected by | Host does | Control plane does |
|---|---|---|---|
| Tunnel drops | host: socket error; control plane: missing heartbeats | reconnects with backoff, keeps VMs running, keeps lease timers running | after 3 missed heartbeats: host `unreachable`, no new placement; incarnations stay current until lease expiry |
| Host reconnects with a VM the control plane replaced | `host.hello` inventory vs `current_incarnations` | kills the stale VM, `vm.event stale_killed` | nothing; the replacement already restored from the durable cut |
| Lease expires on a partitioned host | host timer | kills the VM, `vm.event lease_expired_killed` when the tunnel returns | reconciler marks the incarnation `failed`, confirmed-running work `failed(vm_lost)`, next resume places elsewhere |
| VMM process exits unexpectedly | host wait() | `vm.event exited{code}` with log tail, frees the slot | incarnation `failed`, work `failed(vm_lost)` |
| Dispatcher dies inside a live VM | host: guest listener refuses `rpc` | reports `vm.event exited` only if the VMM exits; else `vm.status` shows `streams_open = 0` and listener failures | treats listener loss as VM loss and issues `vm.stop kill` |
| Boot fails | host | `boot_failed(reason, log_tail)` | incarnation `failed`, retry placement once, then surface to the user |
| Slot exhausted under a race | host | `slot_unavailable` | re-place on another host |
| Spec rejected | host validation | `spec_rejected(reason)` | bug: alert, do not retry the same spec |
| OOM inside the VM | host cgroup | `vm.event oom`, VM killed | as VM loss, with the reason shown to the user |
| Host disk full | host | heartbeat `disk_free_mib` below threshold, refuses `vm.start` with `slot_unavailable(disk)` | stops placing on the host, drains it |
| Control plane restarts | host: tunnel drop | reconnects; `host.hello` replays inventory, then the unacknowledged `vm.event` journal | rebuilds the tunnel registry from `host.hello`; incarnation state is in Postgres, so nothing is lost |
| `vm.start` sent, tunnel drops before the result | control plane: `timeout` | boots or not; result retained | on reconnect reads the inventory: listed → treat as `started`; absent → re-send the same idempotent command |
| Two incarnations of one workspace placed on the same host | host | second `vm.start` gets `slot_unavailable(workspace_busy)` | places elsewhere or waits for the first VM's `exited` |
| Control plane and host disagree on protocol | `host.hello` | on `host.reject`: self-drain, keep VMs and leases, retry with backoff | deploys the control plane first so both host versions are accepted |
| Proxy upstream failure | host | returns the provider's error to the CLI; upstream error count on the heartbeat | surfaces provider status; no VM action |

## 11. Explicitly absent surfaces

| Absent | Replacement |
|---|---|
| `poll`, `claim`, `ack`, `nack`, `observation` on tasks | `vm.start` and `vm.stop` on incarnations; work is dispatched inside the VM through the Workspace Runtime facade |
| Per-task assignment, handoff file, `Materializer`, repo checkout on the host | `VmSpec.repos` plus the dispatcher's own restore from the Git service |
| Golden images, golden lanes, builder and consumer | The workspace root and scratch disk retained on the host between incarnations (`VmSpec.workspace_root.retain_hours`) |
| Sticky runtimes, parked runtimes, wake and busy riders on the heartbeat | Resident VMs; provider processes are the dispatcher's concern |
| Discard fence challenge, proof and receipt | The incarnation fence: `vm_id` in the token, the lease, and the current-incarnation set |
| Fixed 24 h host JWT | Renewable host credential (§3.1) |
| Zero-slot polling, `concurrent_jobs` as a task budget | `slots_total` and occupancy derived from `workspace_vms` |
| Separate preview VM registry and chat-attach machinery | `http` streams to the workspace VM |
| In-guest LLM proxy started with a credential handed into the VM | Host-side per-VM proxy listener (`g5-llm`) with `credential_grants` and the `vm_id` fence |
| Any host shell, inspect command, or arbitrary command from the control plane | `host.heartbeat`, `vm.status`, `vm.event`, and host logs |

## 12. Invariants

1. The host runs what the control plane names and nothing else: every VM on a host corresponds to a `(workspace_id, vm_id)` the control plane issued, and every reconnect reconciles the two sets by killing the difference.
2. A secret never enters a provider process. Provider, git and platform credentials reach the host by reference in `VmSpec.credential_grants`, live in host memory, and are attached only by the host proxies for the incarnation that owns them. The dispatcher alone holds the workspace token, bound to the incarnation, for its own calls to the control plane.
3. The host never decides that a VM is done, idle, or lost. It reports; the control plane decides. The single exception is lease expiry, which the host enforces locally because it is the same value the control plane waits for before placing a replacement.
4. Slot occupancy is derived from `workspace_vms`, never stored on the host row, so a crashed start cannot leak a slot.
5. Every VM-scoped command and stream is refused with `stale` for an incarnation that is not current, including after a reconnect, and a draining incarnation is current.
6. Model traffic and git traffic egress from the host, never through the control plane tunnel, so a single control-plane replica is not on the data path of a running agent.
7. Nothing on the host is decided by an absolute timestamp from another machine: leases are durations from receipt, events are ordered by per-VM sequence numbers, and commands are matched by idempotency key.
8. A host never runs a command it cannot parse: the protocol version is negotiated at `host.hello`, the spec version is checked before any side effect, and an unknown version is a refusal that leaves running VMs untouched.
9. Every refusal is definite and named. A host answers `slot_unavailable(reason)`, `spec_rejected(reason)`, `busy` or `too_many_streams`; it never hangs on a limit.

## 13. Open questions

To be reconciled with the module-API discussion document once it is readable from this session.

| Question | Options | Default in this note |
|---|---|---|
| Retention of the workspace root and scratch disk after stop: fixed hours or org policy? | fixed / org setting | org setting with a 24 h default, carried in `VmSpec.workspace_root.retain_hours` |
| One version number or two? `protocol_version` covers the message set on the tunnel and is negotiated once per connection at `host.hello`; `spec_version` covers the shape of `VmSpec` and is checked per `vm.start`. Both describe the control plane's side, so a spec mismatch is a sub-case of "control plane newer than host". They are kept separate because the spec changes more often than the message set, and a spec a host cannot safely honor (for example a new `credential_grants` kind the host must enforce) should reject one placement, not drain the whole host. | (a) two numbers, as in §3.0 (b) one `protocol_version` that also covers `VmSpec`; every required spec change bumps it and old hosts self-drain | (a); (b) is simpler to reason about but turns every required spec change into a fleet drain |
| Can the passt door be mapped to a per-VM host address so the proxy can tell VMs apart by source address, making the proxy secret defense in depth only? Today `--map-host-loopback 169.254.1.2` maps the door to host `127.0.0.1`, which every VM on the host shares. | (a) per-VM proxy secret, as in §6 (b) door mapped to a per-VM host loopback address, if passt supports a non-`127.0.0.1` target (c) network namespace per VM | (a) until (b) is verified on the pinned passt; (a) stays as defense in depth either way |
| Does a host still own an image identity after v6? Today each host has `base_image_ref`, `base_image_source_kind` (track or pin), `worker_profile` (fat-default or slim) and registry-resolved `runtime_schema` / `runtime_labels`, refreshed on demand by `POST …/:host/runtime-labels/refresh`. v6 §2 drops customer-specific worker images and §6.5 says the VM image is the org or workspace `base_image`, so this draft moves the image to `VmSpec.image` per `vm.start` and has the host report only its own `runtime_identity` (g5d, libkrun, libkrunfw, kernel, base image digest it was built from) in `host.hello`. That removes the refresh route and the per-host image columns. | (a) image is per VM only; host reports `runtime_identity`; retire the route and columns. (b) host keeps an image track and profile that constrain which `VmSpec.image` it accepts; keep the refresh route and columns; `vm.start` answers `spec_rejected(image_not_supported)` on mismatch. (c) both: per-VM image, plus a host-level allowlist of image digests it has cached, reported on `host.hello` and used by placement. | (a), because v6 makes the image a property of the workspace, not the machine; (c) is the fallback if placement needs to know which images a host already holds |

One seam with a sibling module doc, to be closed there:

| Seam | Owner | Needed change |
|---|---|---|
| §6.1 relies on every provider process being launched with the per-session base URL (`ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` = `http://g5-llm/s/<session_id>`), otherwise §6.2 cannot attribute `requests_open` to a session. The process is launched by the `agent.turn` module. | agents module (`agent.turn` launch environment) | State in the agents API that `agent.turn run` sets these variables from its session id, and that the credential-bearing variables (`ANTHROPIC_API_KEY`, `GH_TOKEN`) are either absent or the proxy's placeholders. |

Settled 2026-09-15: `vm.stop graceful` is an OS signal to the guest init, not an rpc method. An earlier draft asked the workspace module for a `runtime.shutdown` method; that dependency was unnecessary, since the stop sequence already completes on `rpc` before `vm.stop` is sent and the host can signal the VM on its own.

Settled 2026-09-15: the VM network mode is passt only. TSI is not offered, because it impersonates only the socket layer and cannot run docker or arbitrary guest listeners, both of which v6 requires inside the workspace VM. `VmSpec.network` keeps its field for a future mode but accepts only `passt`.

Settled by design v6 §14 and §17.1: the tunnel registry is process-local in this iteration (a single control-plane replica), and moving it to Redis is an implementation-time choice for Phase 4 or later; `host.hello` replay makes a control-plane restart cheap either way.

Settled by plan P4.5: placement is a function of `WorkspaceVmService` (the `resume` step chooses the host, preferring the one that still holds the workspace's disk); it is not a separate scheduler and this document does not reopen it.

Settled during the module-API alignment: `lease.renew` is the one name for the incarnation renewal on both `rpc` and `ctl` (workspace-vm-api.md's private protocol already uses it); per-session request tagging is assigned by the dispatcher and read by the host proxy (§6.1, §6.2); and the credential proxy for git and gh is part of this surface (§6.4) because the provider-layer design requires that no credential reach a provider process. This changes v6 §5.2 and §6.3, which put the workspace token in the VM environment for every process: here only the dispatcher holds it.
