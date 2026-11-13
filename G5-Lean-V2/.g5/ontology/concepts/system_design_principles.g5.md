# System design principles

<a id="g5.design"></a>
[D · g5.design]

Design principles guide how G5 builds features: what to favor when more than one design would work.

<a id="g5.design.minimal-mechanism"></a>
[D · g5.design.minimal-mechanism]

Choose a clear design with as little state and machinery as the requirements allow. Justify new state, migrations or mechanisms against simpler use of existing operations. Keep the implementation, integration, risk controls and tests complete; fewest lines is not the goal.

<a id="g5.design.abstraction"></a>
[D · g5.design.abstraction]

Let callers ask for meaningful work without knowing how it is done. Hide repeated complexity behind stable concepts, so replacing a provider, transport or store does not change unrelated workspace or task rules.

<a id="g5.design.durability"></a>
[D · g5.design.durability]

Decide whether data must survive a process crash before choosing its cache, journal or store. Respect existing durability rules; asynchronous work or a progress display does not by itself require persistence or stronger recovery.

## Metadata

### g5.design

- agent
- Defines the `g5.design` level.
- Children: [Minimal mechanism](#g5.design.minimal-mechanism), [Abstraction](#g5.design.abstraction), [Durability](#g5.design.durability).

### g5.design.minimal-mechanism

- human-provided
- Expands: [Design principles](#g5.design).
- Source: [Ben, line 333](../../../docs/ben_new_ontology.md#L333).

### g5.design.abstraction

- human-provided
- Expands: [Design principles](#g5.design).
- Source: [Ben, line 334](../../../docs/ben_new_ontology.md#L334).

### g5.design.durability

- human-provided
- Expands: [Design principles](#g5.design).
- Source: [Ben, line 335](../../../docs/ben_new_ontology.md#L335).
