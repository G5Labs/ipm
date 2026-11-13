# Human

<a id="u.provided"></a>
[I · u.provided]

Provided.

<a id="u.verified"></a>
[D · u.verified]

Verified.

<a id="u.agent"></a>
[I · u.agent]

Agent.

<a id="u.mixed"></a>
[I · u.mixed]

Mixed bullet.

<a id="u.uncertain"></a>
[I · u.uncertain]

Uncertain human.

<a id="u.qa"></a>
[Q · u.qa]

Which option?

```g5-question
schema: 1
id: u-qa
prompt: Which option?
options:
  - id: a
    label: A
  - id: b
    label: B
status: answered
answer:
  answered_by:
    kind: user
  selected: [a]
```

<a id="u.qb"></a>
[Q · u.qb]

Which option?

```g5-question
schema: 1
id: u-qb
prompt: Which option?
options:
  - id: a
    label: A
  - id: b
    label: B
status: answered
answer:
  answered_by:
    kind: agent
  selected: [a]
```

<a id="u.qc"></a>
[Q · u.qc]

Which option?

```g5-question
schema: 1
id: u-qc
prompt: Which option?
options:
  - id: a
    label: A
  - id: b
    label: B
status: open
```

## Metadata

### u.provided

- human-provided

### u.verified

- human-verified
- Source: chat.

### u.agent

- agent

### u.mixed

- human-provided via chat

### u.uncertain

- human-verified, uncertain

### u.qa

- agent, uncertain

### u.qb

- agent, uncertain

### u.qc

- agent, uncertain

