---
child_spec_ids: []
complexity: medium
component_id: tinydb/middlewares
dependencies:
- tinydb
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/middlewares.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Middleware System

## Overview

The TinyDB Middleware system provides a plugin architecture for extending storage behavior in TinyDB document database. Middlewares hook into the read/write process, enabling caching, logging, encryption, and other cross-cutting concerns without modifying core storage implementations.

## Behavior

The middleware system uses a decorator-like pattern where each middleware wraps a storage instance. When TinyDB creates a storage with a middleware, the middleware's `__call__` method initializes the underlying storage and returns itself as a proxy. Attribute access is forwarded to the underlying storage via `__getattr__`, making middlewares transparent to callers.

**CachingMiddleware** specifically improves performance by:
- Reading from an in-memory cache on every read operation
- Batching writes in memory and flushing to disk only after `WRITE_CACHE_SIZE` (1000) modifications
- Automatically flushing pending writes when closed

## Dependencies

- **tinydb.Storage**: Abstract base class for storage implementations that middlewares wrap

## Domain Context: Middleware Pattern for Storage Extension

### Why Middleware Pattern?

**Context**: TinyDB needs extensible storage behavior without tight coupling to specific implementations.

**Risk**: Hardcoding caching, logging, or encryption directly in storage classes creates:
- Code duplication across storage types
- Inflexible behavior (can't combine multiple concerns)
- Difficult testing (can't mock individual behaviors)

**Solution**: Middleware chain pattern where each middleware wraps the next, forwarding calls while adding behavior.

**Implementation Pattern**:
```python
# User creates: TinyDB(storage=CachingMiddleware(JSONStorage('db.json')))
# Execution flow:
# 1. CachingMiddleware.__init__(JSONStorage) - stores class reference
# 2. TinyDB calls CachingMiddleware() - invokes __call__
# 3. __call__ creates: self.storage = JSONStorage() - actual storage
# 4. Returns self - TinyDB uses CachingMiddleware instance as storage
# 5. Operations: db.insert(...) → CachingMiddleware.write(...) → self.storage.write(...)
```

### Caching Strategy Trade-offs

**Context**: Disk I/O is slow; in-memory operations are fast.

**Trade-off Analysis**:
- **Every-write-through**: Safest but slowest (writes hit disk immediately)
- **Write-through with batch**: CachingMiddleware approach - buffers writes, flushes periodically
- **Write-back**: Most aggressive - only writes on close/crash, risk of data loss

**Why CachingMiddleware uses batched write-through**:
- Guarantees data durability on crash (within 1000 writes)
- Reduces I/O by ~1000x for burst writes
- Simple to implement correctly
- Clear failure mode (max 1000 writes lost)

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `class Middleware` - Base class for all middlewares
- [ ] `Middleware.__init__(self, storage_cls) -> None` - Initialize with storage class reference
- [ ] `Middleware.__call__(self, *args, **kwargs) -> Middleware` - Create storage instance, return self as proxy
- [ ] `Middleware.__getattr__(self, name) -> Any` - Forward attribute access to underlying storage
- [ ] `class CachingMiddleware(Middleware)` - Caching implementation extending Middleware
- [ ] `CachingMiddleware.WRITE_CACHE_SIZE: int = 1000` - Class constant for cache flush threshold
- [ ] `CachingMiddleware.__init__(self, storage_cls) -> None` - Initialize cache structures
- [ ] `CachingMiddleware.read(self) -> Optional[Dict]` - Read from cache or storage
- [ ] `CachingMiddleware.write(self, data: Dict) -> None` - Store in cache, flush if threshold reached
- [ ] `CachingMiddleware.flush(self) -> None` - Force write cache to underlying storage
- [ ] `CachingMiddleware.close(self) -> None` - Flush cache and close underlying storage

#### Required Behavior (NON-NEGOTIABLE)
- [ ] MUST forward all unknown attributes to self.storage via __getattr__
- [ ] MUST initialize underlying storage in __call__, not in __init__
- [ ] MUST call super().__init__(storage_cls) in CachingMiddleware.__init__
- [ ] MUST flush cache in close() before delegating to storage.close()
- [ ] MUST increment _cache_modified_count on every write()
- [ ] MUST flush when _cache_modified_count >= WRITE_CACHE_SIZE
- [ ] MUST return cached data on read() if cache exists (skip storage I/O)
- [ ] MUST only read from storage once (lazy initialization of cache)

#### Performance Requirements
- [ ] Cache must reduce disk I/O by batching writes (up to 1000x improvement for burst writes)
- [ ] Read operations must return cached data without disk access after first read
- [ ] Cache flush must use single write operation, not per-document writes

#### Error Handling Requirements
- [ ] MUST handle None return from underlying storage.read() (empty database)
- [ ] MUST preserve None semantics (don't convert to empty dict)
- [ ] MUST propagate exceptions from underlying storage unchanged

## Architectural Rationale

### Design Pattern: Proxy/Decorator

The Middleware class implements the Proxy pattern:
- Maintains reference to real storage object
- Forwards attribute access transparently via __getattr__
- Can intercept and modify operations (read, write, close)

**Critical Constraint**: All attribute forwarding must use self.__dict__['storage'] to avoid infinite recursion through __getattr__.

### Data Flow

```
User Code: db.insert({...})
    ↓
TinyDB: storage.write(document)
    ↓
CachingMiddleware.write(data)
    ↓
[Cache hit] → Return cached data (no I/O)
[Cache miss] → Read from storage once, cache result
    ↓
On write: cache = data, _cache_modified_count++
    ↓
If count >= 1000: flush() → storage.write(cache)
    ↓
On close: flush() → storage.close()
```

### Atypical Design Choice: __getattr__ for Transparency

**Pattern**: Using __getattr__ to forward ALL unknown attributes
**Looks Like**: Could cause confusion or unexpected behavior
**Actually**: Intentional - makes middleware completely transparent
**Purpose**: Users can access storage-specific methods without knowing middleware exists
**Contributes to**: Simple API where middlewares are optional wrappers

### Why Not Cache self._table?

The caching is at the storage I/O level, not the document table level. This is intentional:
- Storage-level caching works with any storage implementation
- Document-level caching would require invalidation logic tied to Table class
- Separation of concerns: middlewares handle I/O, Table handles documents