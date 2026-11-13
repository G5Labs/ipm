# Workspace VM shell access

<a id="g5.terminal.authority"></a>
<a id="g5.terminal"></a>
[I · g5.terminal]

The terminal is an interactive shell that the workspace owner can open while the workspace VM is running.

<a id="g5.terminal.websocket"></a>
[I · g5.terminal.websocket]

The browser connects through a workspace-scoped authenticated WebSocket.  

<a id="g5.terminal.runtime"></a>
[I · g5.terminal.runtime]

The Workspace Runtime facade relays the connection through the host’s PTY channel to the guest dispatcher.  

<a id="g5.terminal.environment"></a>
[I · g5.terminal.environment]

The shell runs inside the VM with access to its live workspace files and native tools.  

<a id="g5.terminal.protocol"></a>
[I · g5.terminal.protocol]

The terminal protocol supports opening, input, resizing, closing and reattachment by terminal identity.  

<a id="g5.terminal.reattach"></a>
[I · g5.terminal.reattach]

Reattaching resumes output from an offset, as far back as output is still kept. A detached shell and whatever runs in it are kept for about five minutes. The browser keeps trying to reconnect for longer than an API restart takes, about 45 seconds in QA, so an open terminal survives an API deploy.

<a id="g5.terminal.credentials"></a>
[I · g5.terminal.credentials]

Terminal credentials resolve for the current owner; platform-owned environment keys cannot be overridden.  

<a id="g5.terminal.secret-admission"></a>
[I · g5.terminal.secret-admission]

The owner’s terminal gets the secrets that the [shared policy](chat.g5.md#g5.chat.secret-admission) admits. If the set is invalid, the API refuses before the guest is asked to open a shell, and the guest checks the same policy. Either refusal answers with a structured rejection reason, which the terminal shows so the owner can fix the configuration and retry; it never just closes.

<a id="g5.terminal.secret-admission.loader-vars"></a>
[D · g5.terminal.secret-admission.loader-vars]

That policy does not reserve loader variables such as `LD_PRELOAD`, because they are no boundary against the owner’s [guest authority](../concepts/security.g5.md#g5.security.guest-authority).

<a id="g5.terminal.image-tools"></a>
[I · g5.terminal.image-tools]

Terminal sessions use the [image environment](../software_architecture/hosts.g5.md#g5.host.environment) and [image/runtime tool precedence](../software_architecture/hosts.g5.md#g5.host.tool-path).

<a id="g5.terminal.context"></a>
[I · g5.terminal.context]

The shell receives workspace context so g5d can make authorized API and internal Git requests.  

<a id="g5.terminal.durability"></a>
[I · g5.terminal.durability]

Shell edits remain local until an explicit durability operation saves them to internal Git.  

<a id="g5.terminal.separation"></a>
[I · g5.terminal.separation]

Interactive terminal access is separate from declared module execution and does not provide platform process-control authority.


## Attachment and replay

<a id="g5.runtime.terminal.attachment"></a>
<a id="g5.terminal.attachment"></a>
[D · g5.terminal.attachment]

Only the current attachment generation may send input, resize or close a terminal, or start receiving output.

- Reattaching invalidates the previous generation, which starts no further send. A send it already started may still finish, because stopping it would need a lock on every send; its replay counts as started when it attaches.
- If an older input write cannot be aborted and is still in flight, retire the shell instead of attaching a new owner.
- When the [reattach](#g5.terminal.reattach) offset is no longer kept, reset explicitly to the oldest kept output.

<a id="g5.terminal.output-retention"></a>
[D · g5.terminal.output-retention]

A terminal session keeps output only in two bounded windows: one for reattachment replay and a larger one for polling. Streaming to the browser keeps nothing beyond them. A polling reader gets the kept bytes since its last read and is told when earlier output was dropped, as a replay reset is.

## Metadata

### g5.terminal

- human-provided
- Source: [Ben, line 273](../../../docs/ben_new_ontology.md#L273).
- Former ID: `g5.terminal.authority`, restated as the concept that defines the `g5.terminal` level ([writing principles](https://github.com/G5Labs/G5-Lean/issues/1119)).
- Children: [Websocket](#g5.terminal.websocket), [Runtime](#g5.terminal.runtime), [Environment](#g5.terminal.environment), [Protocol](#g5.terminal.protocol), [Reattach](#g5.terminal.reattach), [Credentials](#g5.terminal.credentials), [Secret admission](#g5.terminal.secret-admission), [Image tools](#g5.terminal.image-tools), [Context](#g5.terminal.context), [Durability](#g5.terminal.durability), [Separation](#g5.terminal.separation), [Attachment](#g5.terminal.attachment), [Output retention](#g5.terminal.output-retention).

### g5.terminal.websocket

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 274](../../../docs/ben_new_ontology.md#L274).

### g5.terminal.runtime

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 275](../../../docs/ben_new_ontology.md#L275).

### g5.terminal.environment

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 276](../../../docs/ben_new_ontology.md#L276).

### g5.terminal.protocol

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 277](../../../docs/ben_new_ontology.md#L277).

### g5.terminal.reattach

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 278](../../../docs/ben_new_ontology.md#L278).
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 30 (TM-Q1).

### g5.terminal.credentials

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 279](../../../docs/ben_new_ontology.md#L279).

### g5.terminal.secret-admission

- agent
- Expands: [Terminal](#g5.terminal).
- Source: [issue #695 fix design](../../pr_ontology_diff_design/issue-695/ontology-plan.md); the API open-frame producer, the guest `open` consumer and the browser hook were inspected directly. Specializes [g5.terminal.credentials](#g5.terminal.credentials).

### g5.terminal.secret-admission.loader-vars

- human-verified
- Expands: [g5.terminal.secret-admission](#g5.terminal.secret-admission).
- Source: the #535/#576 decision restated in [issue #695](https://github.com/G5Labs/G5-Lean/issues/695) ("do not reopen the rejected interpretation"); see also [runtime-config loader variables](../concepts/security.g5.md#g5.security.runtime-config-loader-env).

### g5.terminal.image-tools

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 280](../../../docs/ben_new_ontology.md#L280).

### g5.terminal.context

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 281](../../../docs/ben_new_ontology.md#L281).

### g5.terminal.durability

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 282](../../../docs/ben_new_ontology.md#L282).

### g5.terminal.separation

- human-provided
- Expands: [Terminal](#g5.terminal).
- Source: [Ben, line 283](../../../docs/ben_new_ontology.md#L283).
- The preview’s [hosted output session](../software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail) reuses the attachment and replay path under [its viewer rules](../software_architecture/workspace_vm.g5.md#g5.preview.output.live-tail.viewer) without granting shell or process-control authority: it is read-only, attachable only when the dispatcher already hosts it, and never created by the terminal protocol (`backend/g5d-dispatch/src/server.rs` `open_hosted_pty`).

### g5.terminal.attachment

- agent
- Expands: [Terminal](#g5.terminal).
- Former ID: `g5.runtime.terminal.attachment`, moved under `g5.terminal` ([writing principles](https://github.com/G5Labs/G5-Lean/issues/1119)).
- Source: [runtime reconstruction](../../uplift/ben-ontology/investigations/runtime.md) and [full review](../../uplift/ben-ontology/reviews/runtime-full.md).
- TerminalOwnership checked generation-exclusive effects and retirement after a nonabortable input write; that historical check does not establish the new allowance for output sends already begun; TerminalReplay checked suffix replay and explicit reset for unavailable offsets. OS reap, WebSocket authority and lossless transport are outside those checks.
- Amendment: [supplied proposal](../../pr_ontology_diff_design/ontology-gap-fill/ontology-plan.md#source-and-scope), item 31 (TM-Q2).

### g5.terminal.output-retention

- agent
- Expands: [Terminal](#g5.terminal).
- Source: [issue #695 fix design](../../pr_ontology_diff_design/issue-695/ontology-plan.md); the session's three output stores and every writer were inspected directly. Expands [g5.terminal.attachment](#g5.terminal.attachment) to the polling reader.
