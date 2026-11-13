# Workspace VM API Surface

> Status: proposed architecture
>
> Purpose: define the narrow, debuggable interface between the G5 control plane and a workspace VM. This note refines the workspace-dispatcher surface derived from the [G5-Lean backend design, draft 6](https://github.com/G5Labs/G5-Lean/blob/main/docs/lean-backend-design-v6.md).

## 1. Boundary

The workspace VM is an ephemeral execution environment and working-tree cache. It is not the authority for Git refs, identity, approvals, provider credentials, or durable platform metadata.

| Place | Owns |
|---|---|
| Git service | Refs, receive-pack policy, workspace-branch ownership, protected branches, provider publication, and merges. |
| Workspace VM | The live working tree, declared modules, provider/agent processes, interactive terminals, and VM-local observations. |
| Workspace Runtime facade | The only control-plane client allowed to open the VM channel. Other platform systems call this facade rather than the host tunnel or dispatcher directly. |
| Agent or terminal inside the VM | Native shell and Git operations. These are intentionally local capabilities, not control-plane extension APIs. |

Every VM request is bound to a `workspace_id`, current `vm_id`, actor or capability grant, deadline, and idempotency key. A stale incarnation is rejected at every boundary.

## 2. Workspace Runtime API

These are the externally consumable VM operations. They are exposed through the Workspace Runtime facade, never as a direct host or dispatcher connection.

| Surface | Methods | Caller | Contract |
|---|---|---|---|
| Files | `fs.list`, `fs.read`, `fs.stat`, `fs.write(path, content, expected_hash)`, `fs.delete(path, expected_hash?)`, `fs.rename(from, to, expected_hash?)`, `fs.watch` | Authorized editor/file UI | Explicit workspace editing and tree observation. `fs.watch` emits `file.changed`; mutation preconditions prevent a silent editor overwrite. |
| Terminal | `terminal.open(cols, rows)`, `terminal.write`, `terminal.resize`, `terminal.close` | Authorized workspace user | Attaches a browser terminal. It is an interactive user surface and never a platform-extension mechanism. |
| Modules | `modules.list`, `modules.describe`, `modules.invoke(exec_id, module_id, version, operation, input, grant_id)`, `modules.status`, `modules.cancel` | Authorized platform feature through the facade | The sole platform execution/extension path. Callers cannot supply arbitrary `argv`, environment, cwd, or signal targets. |
| Ports | `ports.list`; `port.opened`, `port.closed` events | Ports UI through the facade | Observes declared or detected VM services. It does not start processes. |
| Workspace status | `workspace.status.get` | Workspace UI, support, operators | Reports lifecycle state, current or last `vm_id`, host placement, lease freshness, and the latest sync receipt with an explicit freshness label. |
| Runtime inspection | `runtime.inspect.summary` | Workspace owner or support grant | Reports dispatcher/image/catalog identity, boot time, uptime, resource pressure, mounted workspace roots, port summary, and fixed health-probe results. |
| Module diagnostics | `modules.logs(exec_id, cursor?)`, `modules.events(exec_id, cursor?)`, `modules.history(filter)` | Workspace owner or support grant | Returns bounded, redacted module logs and structured events, recent invocations, grants, limits, outcomes, and failure classification. It does not reveal raw host processes. |
| Diagnostic bundle | `diagnostics.capture(profile)`, `diagnostics.probe(name)` | Support grant; workspace owners for safe profiles | Produces a versioned, redacted support bundle or runs a declared read-only probe such as `workspace-integrity`, `runtime-health`, `port-connectivity`, or `module-catalog-integrity`. No caller-supplied shell command is accepted. |

When the VM is stopped, `workspace.status.get` returns only durable control-plane and last-reported state, labeled `stopped` or `last_reported`. It must not represent that data as a live inspection or wake the VM merely to serve a read.

## 3. Declared module runtime

`proc.spawn` is a private dispatcher implementation detail. Every platform-initiated executable unit is a declared module invocation.

| Module declaration field | Requirement |
|---|---|
| Identity | Stable `module_id`, semantic version, and immutable manifest hash. |
| Contract | Named operations plus typed input, output, and event schemas. |
| Execution | A pinned entrypoint and a lifecycle class: bounded `run` or desired-state service. |
| Limits | Time, CPU, memory, disk, concurrency, and output/log limits. |
| Capabilities | Minimal requested workspace read/write scope, bounded port allocation, network-egress policy, Git operation scope, provider-turn access, and related privileges. |
| Observability | Declared health probes, diagnostic profile, event kinds, and redaction policy. |

The dispatcher resolves the immutable catalog entry, validates input, and intersects requested capabilities with the control-plane-issued `grant_id` before using its private runner. A manifest requests capabilities; it never grants them. The runtime image and module catalog are signed or hash-pinned. A workspace configuration may select or configure an installed module but cannot add code or privilege.

Each invocation is auditable as:

```text
(workspace_id, vm_id, exec_id, actor, module_id, version, manifest_hash,
 grant_id, input_digest, capability_grant, outcome)
```

`modules.invoke` is idempotent on `exec_id = (task_id, attempt, vm_id)`. Its disposition is `started`, `already_running`, or `already_finished(outcome)`. Module events and terminal outcomes use the dispatcher journal, ordered replay, acknowledgement, and incarnation-fence rules.

### Built-in modules

| Module | Operations | Notes |
|---|---|---|
| `workspace.preview` | `start(profile)`, `stop`, `status` | Canonical desired-state service module. It declares its port and relay contract. |
| `workspace.changes` | `get`, optionally `commit` | Returns a normalized dirty/ahead/behind/conflict projection rather than exposing generic Git commands. |
| `agent.turn` | `run` | Runs one Claude or Codex turn through the [Agent API Surface](agent-api-surface.md), including its provider/session/question contract. |
| `workspace.bootstrap` | `run` | Performs deterministic workspace setup. |
| `workspace.diagnostics` | `capture(profile)`, `probe(name)` | Supplies the fixed, read-only diagnostic collectors behind the diagnostics surface. |

## 4. Private control-plane protocol

The following operations are required for reliability, but are not product or platform-extension APIs. Only the Workspace Runtime facade, dispatcher, host, and provider runner may use them.

| Protocol | Responsibility |
|---|---|
| `runtime.ready`, `lease.renew`, `health` | Establish VM custody, renew the current incarnation, and report liveness. |
| `events.replay`, `events.ack` | Replay and acknowledge journaled module events and outcomes. |
| `session_files.restore`, `session_files.harvest` | Restore and persist provider session files outside the working tree. |
| Admission barrier | Close admission, drain in-flight mutation calls, and reopen it on a failed lifecycle operation. |
| `workspace.sync` | Establish one durable, complete workspace cut in internal Git for a lifecycle consumer. |

## 5. Workspace sync

There is no asynchronous checkpoint task. Normal durability is agent-driven: agents use native Git to commit and push their work to internal Git. Uncommitted edits and unpushed local commits can be lost in an unplanned VM failure.

`workspace.sync` is the sole platform-triggered flush boundary. It is a privileged lifecycle operation, not a general dispatcher method.

```text
workspace.sync({ reason, materialization }) -> SyncReceipt

reason          = shutdown | fork | external_pr | explicit
materialization = snapshot | publishable
```

The operation closes mutation admission, drains in-flight writes, captures the exact state of every workspace member repository, pushes the required Git objects and refs to internal Git, persists a complete multi-repository receipt, and then reopens admission unless shutdown continues.

```text
SyncReceipt {
  sync_id,
  workspace_id,
  vm_id,
  workspace_epoch,
  repos: [{ repo_id, branch_oid, local_head_oid, snapshot_oid }],
  complete_at
}
```

| Consumer | Required materialization | Follow-up |
|---|---|---|
| VM shutdown | `snapshot` | Stop only after the receipt is durable; resume restores the snapshot. |
| Workspace fork | `snapshot` | Fork from the receipt's exact multi-repository vector, including remaining dirty work. |
| External pull request | `publishable` | Materialize remaining dirty state onto a real source-branch commit if needed; the Git service publishes that pinned source OID. |

The caller consumes the `SyncReceipt`, never a live VM directory or an ad-hoc internal ref. A failed sync fails the enclosing shutdown, fork, or PR preparation operation without offering a partial cut to its consumer.

## 6. Explicitly absent surfaces

| Absent surface | Replacement |
|---|---|
| `proc.spawn`, `proc.signal`, `proc.wait`, `proc.list` | Private dispatcher process runner used only after module resolution. |
| `git.*` | Native Git inside the VM for agents; ref and policy operations in the Git service; `workspace.changes` for a normalized UI projection. |
| `agent.run_turn` | `modules.invoke(..., "agent.turn", "run", ...)`. |
| `preview.start`, `preview.stop`, `preview.status` | The `workspace.preview` module. |
| `checkpoint.run`, `checkpoint.last`, periodic checkpoint events | Agent-driven Git persistence plus explicit `workspace.sync` at durability boundaries. |
| Arbitrary diagnostic shell execution | Fixed diagnostic probes and versioned diagnostic bundles. |

## 7. Design invariants

1. A platform feature that needs to execute workspace code uses a declared module; it never reaches the host, tunnel, or private runner directly.
2. A platform feature that needs workspace state obtains a successful `SyncReceipt` when it requires a durable cut; it never inspects a VM directory directly.
3. Observability is read-only, bounded, redacted, and freshness-labeled; it does not regain raw process control as a troubleshooting escape hatch.
4. Native agent shell and Git behavior remain available inside the workspace VM, but are not a substitute for the module contract.
