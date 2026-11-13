# Core concepts

<a id="g5.concept"></a>
[I · g5.concept]

Core concepts are the shared vocabulary the rest of the ontology builds on.

<a id="g5.concept.organization"></a>
[I · g5.concept.organization]

An organization is a group of people building enterprise software under shared policies. It is a strict boundary for data and metadata, governed by [tenant isolation](system_invariants.g5.md#g5.security.tenant).

<a id="g5.concept.repo"></a>
[I · g5.concept.repo]

A repository stores an organization’s source code and intent documents in Git.

<a id="g5.concept.workspace"></a>
[I · g5.concept.workspace]

A workspace is an isolated place to propose, discuss, implement and approve software changes. It owns repository branches and has a defined, configurable lifecycle. Resource isolation and approvals make experimentation safe; collaboration follows the workspace’s access rules.

<a id="g5.concept.workspace-vm"></a>
[I · g5.concept.workspace-vm]

A workspace has at most one VM: the compute resources that modify its repositories and provide an API for resource-intensive tasks. VM failure can lose work that has not been pushed to a repository.

<a id="g5.concept.host"></a>
[I · g5.concept.host]

A host provides compute resources for one organization and keeps its workspace VMs isolated from each other.

<a id="g5.concept.ontology"></a>
[I · g5.concept.ontology]

The ontology is a minimal description, in English, of the system’s intent and design. Agents must keep the code consistent with it.

<a id="g5.concept.issue"></a>
[I · g5.concept.issue]

An issue tracks a bug, feature, task or other planned work and connects it to the enterprise’s work-tracking process.

## Metadata

### g5.concept

- agent
- Defines the `g5.concept` level.
- Children: [Organization](#g5.concept.organization), [Repository](#g5.concept.repo), [Workspace](#g5.concept.workspace), [Workspace VM](#g5.concept.workspace-vm), [Host](#g5.concept.host), [Ontology](#g5.concept.ontology), [Issue](#g5.concept.issue).

### g5.concept.organization

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 15](../../../docs/ben_new_ontology.md#L15).

### g5.concept.repo

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 16](../../../docs/ben_new_ontology.md#L16).

### g5.concept.workspace

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 17](../../../docs/ben_new_ontology.md#L17).

### g5.concept.workspace-vm

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 18](../../../docs/ben_new_ontology.md#L18).

### g5.concept.host

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 19](../../../docs/ben_new_ontology.md#L19).

### g5.concept.ontology

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 20](../../../docs/ben_new_ontology.md#L20).

### g5.concept.issue

- human-provided
- Expands: [Core concepts](#g5.concept).
- Source: [Ben, line 21](../../../docs/ben_new_ontology.md#L21).
