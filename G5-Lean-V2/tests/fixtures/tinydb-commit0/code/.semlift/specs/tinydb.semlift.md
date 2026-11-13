---
child_spec_ids: []
complexity: high
component_id: tinydb
dependencies: []
granularity_score: 1.0
hierarchy_level: 0
is_root: true
language: python
parent_spec_id: null
source_files: []
type: root
version: 1.0.0
---

# TinyDB

## Intent

<human origin="chat_instruction">
[I1. Deliver a complete embedded document database]

Implement a complete, pure-Python TinyDB-compatible package from this
top-level specification and the committed acceptance tests. Sync is complete
only when the implementation, recursively refined SemLift ontology, and docs
agree and the full acceptance suite passes without accessing remote source
implementations.
</human>

[I2. Preserve the public package surface]

The `tinydb` package exposes `TinyDB`, `Query`, `where`, `Storage`,
`JSONStorage`, and the version value expected by its callers. Public modules
include database/table behavior, queries, storage backends, middleware,
operations, and utilities. The committed tests are the executable contract for
exact names, signatures, edge cases, and compatibility behavior.

[I3. Provide document-oriented CRUD and table isolation]

TinyDB manages named tables of mapping-like documents. The default table is
available through the database facade. Inserts assign stable document IDs;
reads, searches, updates, removals, containment, iteration, length, truncation,
and table deletion preserve the behaviors exercised by the acceptance suite.
Named tables remain isolated while sharing the selected storage backend.

[I4. Provide composable query predicates]

Queries support nested field paths, comparison operators, existence and type
checks, regular-expression and search predicates, collection predicates,
fragments, custom tests, and boolean composition. Query objects used as cache
keys must have deterministic equality and hashing semantics. Non-cacheable
queries must not enter the table query cache.

[I5. Separate storage from database semantics]

Storage is an extensible read/write/close abstraction. `MemoryStorage` keeps
state in-process. `JSONStorage` persists the complete database state to a file,
honors caller-provided JSON options and access modes, handles empty files, and
keeps repeated writes structurally valid. Database and middleware code must
work against the storage contract rather than a concrete backend.

[I6. Support middleware and reusable update operations]

Middleware wraps storage classes or instances without changing the database
API. Caching middleware buffers writes according to its configured threshold,
flushes explicitly and on close, and does not lose pending state. Reusable
operations update or delete fields and perform arithmetic or assignment in the
forms covered by the acceptance tests.

[I7. Preserve utility value semantics]

The utility layer provides bounded least-recently-used caching and recursive
freezing of mutable structures into stable hashable values. Frozen mappings
must behave consistently as immutable mapping values and cache eviction must
follow access recency.

## Quality Constraints

[Q1. Standard-library runtime]

The package has no required runtime dependency outside Python's standard
library. Test-only dependencies are allowed.

[Q2. Acceptance-driven completeness]

Do not weaken, skip, replace, or special-case the committed tests. Diagnose
failures at their owning abstraction, preserve general behavior beyond the
single assertion, and run the whole suite before declaring the sync complete.

[Q3. Recursive ontology refinement]

Before broad implementation fanout, create a valid ontology skeleton. Complex
areas may be recursively decomposed into cluster and leaf specs, with parent
stubs created first when needed so the ontology remains valid after every task.
Leaf specs must capture concrete public interfaces and integration contracts
before or alongside their implementations.

[Q4. Offline synthesis]

Use only the committed workspace state: this root spec, the docs design, and
the acceptance tests. Do not fetch, install, or inspect TinyDB source code from
GitHub, package indexes, caches, neighboring workspaces, or other external
locations.
