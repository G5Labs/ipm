# TinyDB Commit0 Design Seed

## Product Goal

TinyDB is a small embedded document database for Python applications that need
local persistence without operating a database server. The finished workspace
must contain a usable `tinydb` package, a recursively refined SemLift ontology,
and passing acceptance tests. This is a synthesis benchmark: no implementation
is provided in the initial code repository.

## User Surface

Callers create a `TinyDB` instance with either JSON-file persistence or an
in-memory storage backend. They use the database itself as the default table or
request named tables, insert mapping-like documents, query them, update or
remove matches, inspect document IDs, and drop individual or all tables.

Queries are built through `Query` and `where`. They can follow nested paths,
compare values, combine predicates with boolean operators, inspect collections,
match regular expressions, and run caller-provided tests. Query values need
stable hashing where safe because tables cache repeated searches and invalidate
that cache after writes.

Storage is a replaceable abstraction. `MemoryStorage` is ephemeral;
`JSONStorage` persists the database's full table map. Middleware can wrap a
storage implementation, including a write cache that flushes at a configurable
threshold and during shutdown.

## Expected Package Areas

- Package facade and database/table management
- Documents, IDs, CRUD operations, and query caching
- Fluent query construction and predicate evaluation
- Abstract, memory, and JSON storage backends
- Storage middleware and write buffering
- Reusable update operations
- Immutable/freezing and LRU cache utilities

The acceptance tests in the code repository are authoritative for exact public
names, signatures, return values, error behavior, and edge cases. The design is
deliberately top-level: sync should inspect those tests, refine this intent into
dependency-aware child specs, implement the package, integrate child work, and
review the complete system.

## Benchmark Rules

The initial code repository contains only the root SemLift spec, SemLift
configuration, and the untouched acceptance suite. The initial docs repository
contains this design seed. Agents must not obtain TinyDB implementation source
from the network, package caches, adjacent repositories, or host paths. The
result is complete only when the generated implementation passes the committed
suite and the final ontology describes the implemented public contracts.

Complete this benchmark in an ordinary workspace conversation. Validate the integrated implementation against the committed suite and review the result before completing. Exercise real Git, CRDT, provider execution, and task lifecycle boundaries.
