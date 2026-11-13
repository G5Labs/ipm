---
child_spec_ids: []
complexity: medium
component_id: tinydb/utils
dependencies: []
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/utils.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Utility Functions

## Overview
This module provides utility classes and functions for the TinyDB database, specifically implementing an LRU (Least Recently Used) cache, immutable dictionary wrapper, and object freezing utilities. These utilities support core database operations like query caching and stable hashing of dictionary-based queries.

## Behavior

### LRUCache Class
A least-recently used cache with a fixed capacity. Acts as a dictionary but automatically evicts the least-recently accessed entry when capacity is exceeded. Implemented using `OrderedDict` for O(1) access and eviction.

**Key behaviors:**
- On access (get): Moves accessed entry to front of OrderedDict
- On insert: Adds new entry, evicts last entry if capacity exceeded
- Unlimited capacity when capacity is None
- Implements MutableMapping ABC for dict-like interface

### FrozenDict Class
An immutable dictionary subclass that implements `__hash__` for use as dictionary keys or set members. Used to create stable hashes for queries containing dictionaries.

**Key behaviors:**
- All mutating methods raise TypeError
- Hash computed from sorted tuple of items
- Recursively freezes nested dicts, lists, sets

### freeze() Function
Recursively converts mutable objects to immutable equivalents:
- dict → FrozenDict
- list → tuple
- set → frozenset
- Other objects passed through unchanged

### with_typehint() Function
Metaclass-like utility for adding type hints from a base class to a derived class. Used for type checking while maintaining runtime compatibility.

## Dependencies
- collections.OrderedDict: For LRU cache implementation
- collections.abc.MutableMapping: ABC for cache interface
- typing.TypeVar: For generic type parameters
- typing.Generic: For generic class support
- typing.TYPE_CHECKING: For conditional type hints

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `LRUCache(capacity: Optional[int] = None)` - LRU cache with optional capacity limit
- [ ] `LRUCache.capacity: Optional[int]` - Property storing max cache size
- [ ] `LRUCache.lru: List[K]` - Property returning list of keys in LRU order (most recent first)
- [ ] `LRUCache.length: int` - Property returning current cache size
- [ ] `LRUCache.clear() -> None` - Clear all cache entries
- [ ] `LRUCache.get(key: K, default: Optional[D] = None) -> Optional[Union[V, D]]` - Get value, moving to front if found
- [ ] `LRUCache.set(key: K, value: V) -> None` - Set value, evicting if capacity exceeded
- [ ] `LRUCache.__getitem__(key: K) -> V` - Raises KeyError if not found
- [ ] `LRUCache.__setitem__(key: K, value: V) -> None` - Alias for set()
- [ ] `LRUCache.__delitem__(key: K) -> None` - Delete entry
- [ ] `LRUCache.__contains__(key: object) -> bool` - Membership test
- [ ] `LRUCache.__iter__() -> Iterator[K]` - Iterate over keys
- [ ] `LRUCache.__len__() -> int` - Cache size
- [ ] `FrozenDict` - Immutable dict subclass
- [ ] `FrozenDict.__hash__() -> int` - Hash from sorted items tuple
- [ ] `freeze(obj: Any) -> Any` - Recursively freeze mutable objects
- [ ] `with_typehint(baseclass: Type[T]) -> Type` - Add type hints from base class

#### Required Behavior (NON-NEGOTIABLE)
- [ ] LRUCache MUST use OrderedDict internally for O(1) operations
- [ ] LRUCache.get() MUST call move_to_end(key, last=True) on cache hit
- [ ] LRUCache.set() MUST evict least-recently-used item (last=False) when capacity exceeded
- [ ] LRUCache.set() MUST NOT evict when capacity is None (unlimited)
- [ ] LRUCache MUST implement all MutableMapping abstract methods
- [ ] FrozenDict MUST raise TypeError on any mutating operation (__setitem__, __delitem__, clear, etc.)
- [ ] FrozenDict.__hash__() MUST sort items before hashing for consistent hash
- [ ] freeze() MUST recursively freeze nested dicts, lists, sets
- [ ] freeze() MUST return original object for non-mutable types

#### Performance Requirements
- [ ] LRUCache get/set operations: O(1) time complexity
- [ ] LRUCache eviction: O(1) via popitem(last=False)
- [ ] FrozenDict hashing: O(n log n) due to sorting

## Architectural Rationale

### LRU Cache Design
**Pattern**: MutableMapping implementation with automatic eviction
**Rationale**: Provides dict-like interface while enforcing size limits
**Constraints**: Must maintain insertion order for LRU semantics

### FrozenDict for Hashing
**Pattern**: Immutable wrapper enabling hashability
**Rationale**: Queries containing dicts need stable hashes for caching
**Trade-off**: Recursive freezing has overhead, but necessary for nested structures

### Object Freeze Pattern
**Pattern**: Recursive type transformation
**Rationale**: Enables using complex objects as dict keys or set members
**Why not in-place**: Preserves original objects for non-hashing uses