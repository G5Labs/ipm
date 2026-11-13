---
child_spec_ids:
- tinydb/storages
- tinydb/table
- tinydb/database
- tinydb
- tinydb/mypy_plugin
- tinydb/queries
- tinydb/utils
- tinydb/operations
- tinydb/version
- tinydb/middlewares
complexity: medium
component_id: tinydb_aggregate
dependencies: []
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: multi
parent_spec_id: tinydb-parity_project_aggregate
source_files: []
type: aggregate
version: 1.0.0
---

# Design Doc for TinyDB Document Database

## Overview

TinyDB is a lightweight, pure Python document-oriented database designed for simplicity and ease of use. It stores documents (JSON-like dictionaries) in a single file, provides a fluent query interface, and supports customization through storage backends and middleware. The database is designed as an embedded database for applications that need simple, local storage without the complexity of full database servers.

## Behavior

### Core Architecture

TinyDB follows a layered architecture where each layer has distinct responsibilities:

1. **Database Layer** (`tinydb/database`): Entry point managing storage and tables
2. **Table Layer** (`tinydb/table`): Document CRUD operations with query caching
3. **Storage Layer** (`tinydb/storages`): Persistence abstraction (JSON, Memory)
4. **Query Layer** (`tinydb/queries`): Query building and evaluation
5. **Middleware Layer** (`tinydb/middlewares`): Extensible storage enhancement

### Data Flow

```
User Code
    ↓
TinyDB (database.py)
    ↓
Table (table.py) - CRUD operations, caching
    ↓
Middleware (middlewares.py) - Optional caching, logging
    ↓
Storage (storages.py) - JSON file or memory persistence
    ↓
File System / Memory
```

### Storage Model

TinyDB stores all data in a single structure:

```python
{
    '_default': {0: {...}, 1: {...}},
    'table_name': {0: {...}, 1: {...}}
}
```

Each table is a dictionary keyed by document ID (integers). The storage layer converts between integer keys (API) and string keys (JSON serialization).

### Query System

Queries use a fluent, chainable API:

```python
User = Query()
db.search(User.name == 'Alice')
db.search((User.age > 18) & (User.city == 'NYC'))
```

Queries are cached for performance using stable hash values.

## Dependencies

### Internal Module Dependencies

- **tinydb.database**: Depends on `tinydb.storages`, `tinydb.table`, `tinydb.utils`
- **tinydb.table**: Depends on `tinydb.storages`, `tinydb.queries`, `tinydb.utils`
- **tinydb.storages**: No external dependencies (pure Python)
- **tinydb.queries**: Depends on `tinydb.utils`
- **tinydb.middlewares**: Depends on `tinydb.storages`
- **tinydb.mypy_plugin**: Depends on `tinydb.utils`
- **tinydb.operations**: No dependencies
- **tinydb.utils**: No dependencies
- **tinydb.version**: No dependencies

### External Dependencies

None - TinyDB is a pure Python package with no external runtime dependencies.

## Domain Context: Document Database Design

### Storage Persistence Strategy

**Context**: TinyDB must survive crashes and power loss without data corruption
**Risk**: Without proper durability, writes may remain in OS buffers
**Solution**: JSONStorage uses fsync() after every write to force data to physical disk

**Implementation Pattern**:
```python
def write(self, data):
    self._handle.seek(0)
    serialized = json.dumps(data, **self.kwargs)
    self._handle.write(serialized)
    self._handle.flush()           # Python buffers → OS
    os.fsync(self._handle.fileno())  # OS → disk
    self._handle.truncate()      # Remove stale data
```

**Why This Matters**:
- fsync() ensures data survives system crashes
- truncate() prevents corruption when data shrinks
- Order matters: flush → fsync → truncate

### Query Caching Strategy

**Context**: Database queries are expensive; repeated queries should be fast
**Risk**: Re-executing same query wastes resources
**Solution**: LRU cache stores query results, cleared on any write

**Implementation Pattern**:
```python
# On search: Check cache first
if cond in self._cache:
    return copy(self._cache[cond])

# On any write: Clear entire cache
def insert(self, doc):
    self._update_table(...)
    self.clear_cache()  # Data changed
```

**Critical Design Decision**: Clear entire cache on ANY write
- Simpler than tracking affected queries
- No stale data returned
- Trade-off: Clears all cached queries even for unrelated changes

### Document ID Management

**Context**: Documents need unique identifiers
**Risk**: ID collisions or inefficient ID generation
**Solution**: Sequential IDs with caching via `_next_id`

**Implementation Pattern**:
```python
def _get_next_id(self):
    if self._next_id is not None:
        id = self._next_id
        self._next_id += 1
        return id
    # Scan table to find max ID
    max_id = max(table.keys(), default=0)
    self._next_id = max_id + 2
    return max_id + 1
```

### Middleware Extension Pattern

**Context**: Need to extend storage without modifying core classes
**Risk**: Tight coupling prevents customization
**Solution**: Middleware decorator pattern

**Implementation Pattern**:
```python
# User: TinyDB(storage=CachingMiddleware(JSONStorage('db.json')))
# Flow:
# 1. CachingMiddleware.__init__(JSONStorage) - stores class
# 2. TinyDB calls CachingMiddleware() - invokes __call__
# 3. __call__ creates actual storage, returns self as proxy
# 4. Operations go through middleware
```

**Why This Pattern**:
- Transparent to users (middleware is the storage)
- Chainable (multiple middlewares)
- Testable (easy to mock)

### Type Hinting Support

**Context**: Dynamic class creation breaks static type analysis
**Risk**: mypy cannot resolve types for dynamically created classes
**Solution**: mypy plugin intercepts `with_typehint()` calls

**Implementation Pattern**:
```python
# In tinydb/utils.py:
def with_typehint(baseclass):
    # Runtime: creates class with baseclass methods
    return type(name, (baseclass,), {})

# In tinydb/mypy_plugin.py:
class TinyDBPlugin(Plugin):
    def get_dynamic_class_hook(self, fullname):
        if fullname == 'tinydb.utils.with_typehint':
            def hook(ctx):
                # Tell mypy where to find the type
                ctx.api.add_symbol_table_node(ctx.name, qualified)
            return hook
```

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)

The tinydb package exposes the following public API:

- [ ] `TinyDB` - Main database class from `tinydb.database`
- [ ] `Storage` - Abstract storage base class from `tinydb.storages`
- [ ] `JSONStorage` - File-based JSON storage from `tinydb.storages`
- [ ] `MemoryStorage` - In-memory storage from `tinydb.storages`
- [ ] `Query` - Query builder class from `tinydb.queries`
- [ ] `where` - Shorthand query function from `tinydb.queries`
- [ ] `__version__` - Version string from `tinydb.version`

#### Module Structure (MANDATORY)

- [ ] `tinydb/__init__.py` - Package root, re-exports public API
- [ ] `tinydb/database.py` - TinyDB class implementation
- [ ] `tinydb/table.py` - Table class with CRUD operations
- [ ] `tinydb/storages.py` - Storage abstractions
- [ ] `tinydb/queries.py` - Query building and evaluation
- [ ] `tinydb/middlewares.py` - Middleware system
- [ ] `tinydb/operations.py` - Update operations
- [ ] `tinydb/utils.py` - Utility classes
- [ ] `tinydb/version.py` - Version constant
- [ ] `tinydb/mypy_plugin.py` - mypy type checker plugin

#### Required Behavior (NON-NEGOTIABLE)

- [ ] TinyDB MUST support context manager protocol (with statement)
- [ ] TinyDB MUST forward unknown attributes to default table
- [ ] Table MUST use _update_table() pattern for ALL modifications
- [ ] Table MUST clear query cache on ANY write operation
- [ ] JSONStorage MUST call os.fsync() after EVERY write
- [ ] JSONStorage MUST call truncate() after EVERY write
- [ ] Query MUST support logical operators (&, |, ~)
- [ ] Query MUST support comparison operators (==, !=, <, >, <=, >=)
- [ ] Middleware MUST forward unknown attributes to underlying storage
- [ ] CachingMiddleware MUST batch writes (flush every 1000 modifications)

#### Performance Requirements

- [ ] Query caching: O(1) lookup via LRU cache
- [ ] Persistent file handle: 10x faster than open/close per operation
- [ ] Sequential ID generation: O(1) when cached, O(n) otherwise

#### Error Handling Requirements

- [ ] MUST raise ValueError when insert receives non-Mapping document
- [ ] MUST raise ValueError when duplicate document ID provided
- [ ] MUST raise RuntimeError when get() called without condition
- [ ] MUST return None when document not found by ID
- [ ] MUST return empty list when no documents match

## Architectural Rationale

### Design Patterns

#### Facade Pattern (TinyDB class)

The TinyDB class acts as a facade, providing a unified interface to storage and table operations. Users interact with TinyDB without needing to understand underlying storage or table mechanics.

**Benefits**:
- Simple API for common operations
- Hides complexity of storage format
- Enables convenient default table access

#### Builder Pattern (Query class)

Query uses a builder pattern with fluent interface. Each operation returns a new QueryInstance, enabling chaining:

```python
User.name == 'Alice'  # Creates QueryInstance
(User.name == 'Alice') & (User.age > 18)  # Combines
```

#### Updater Pattern (_update_table)

All table modifications use `_update_table(updater)` pattern:

```python
def _update_table(self, updater):
    tables = self._storage.read()
    table = tables.get(self.name, {})
    updater(table)  # Modify in-memory
    tables[self.name] = table
    self._storage.write(tables)
    self.clear_cache()
```

**Why This Pattern**:
- Atomic read-modify-write operations
- Middleware can intercept modifications
- Cache invalidation happens automatically

#### Proxy/Decorator Pattern (Middleware)

Middleware wraps storage, forwarding calls while adding behavior:

```python
class CachingMiddleware:
    def __init__(self, storage_cls):
        self.storage_cls = storage_cls
    
    def __call__(self, *args, **kwargs):
        self.storage = self.storage_cls(*args, **kwargs)
        return self
    
    def __getattr__(self, name):
        return getattr(self.storage, name)
```

#### Strategy Pattern (Storage implementations)

Storage uses strategy pattern - abstract base defines interface, implementations provide concrete behavior:

```python
class Storage(ABC):
    @abstractmethod
    def read(self): ...
    
    @abstractmethod
    def write(self, data): ...

class JSONStorage(Storage):
    def read(self): ...
    def write(self, data): ...
```

### Why These Design Decisions

#### Single File Storage

TinyDB stores all data in a single file rather than one file per table. This was chosen for:
- Simplicity (one file to manage)
- Atomicity (single write for entire database)
- Lower filesystem overhead

The trade-off is that large databases may have slower random access, but this matches TinyDB's use case as a lightweight embedded database.

#### No Transaction Support

TinyDB does not support multi-document transactions. This was chosen for:
- Simplicity of implementation
- No locking overhead
- Matches embedded database use case

For applications needing transactions, a full database like SQLite is recommended.

#### String Keys in Storage

Storage uses string keys (for JSON compatibility), but API uses integer keys. This design:
- Preserves JSON compatibility
- Enables human-readable JSON files
- Requires conversion at API boundary

#### Query Caching Without Invalidation

The cache is cleared entirely on any write rather than tracking affected queries. This design:
- Guarantees correctness (no stale data possible)
- Simpler implementation
- Trade-off: Less efficient for some workloads

### Cross-Module Contracts

#### Storage Interface

All storage implementations must implement:
- `read() -> Optional[Dict]` - Returns None for empty storage
- `write(data: Dict) -> None` - Writes entire database state
- `close() -> None` - Cleanup (default no-op)

#### Query Interface

Query conditions must implement:
- `__call__(doc: Mapping) -> bool` - Evaluate against document
- `__hash__() -> int` - Stable hash for caching

#### Table Interface

Table provides:
- CRUD operations (insert, get, update, remove)
- Query caching
- Document ID management

## Usage Patterns

### Basic Usage

```python
from tinydb import TinyDB, where, Query

# Create database
db = TinyDB('data.json')

# Insert documents
doc_id = db.insert({'name': 'Alice', 'age': 25})
doc_id = db.insert({'name': 'Bob', 'age': 30})

# Search with condition
results = db.search(where('name') == 'Alice')

# Update documents
db.update({'age': 26}, where('name') == 'Alice')

# Remove documents
db.remove(where('age') < 25)

# Close database
db.close()
```

### With Query Builder

```python
User = Query()

# Complex queries
db.search((User.name == 'Alice') | (User.name == 'Bob'))
db.search(User.age > 18)
db.search(User.tags.any('admin'))
```

### With Custom Storage

```python
from tinydb import TinyDB
from tinydb.storages import MemoryStorage

# In-memory database
db = TinyDB(storage=MemoryStorage)
```

### With Middleware

```python
from tinydb import TinyDB
from tinydb.storages import JSONStorage
from tinydb.middlewares import CachingMiddleware

# With caching layer
db = TinyDB(storage=CachingMiddleware(JSONStorage('data.json')))
```

### As Context Manager

```python
with TinyDB('data.json') as db:
    db.insert({'name': 'Alice'})
# Automatically closed on exit
```

## File Organization

### Why Each File Exists

- **`tinydb/__init__.py`**: Package root, re-exports public API for convenience
- **`tinydb/database.py`**: TinyDB main class, entry point for users
- **`tinydb/table.py`**: Table class, handles document CRUD operations
- **`tinydb/storages.py`**: Storage abstractions, enables persistence customization
- **`tinydb/queries.py`**: Query building, enables fluent query syntax
- **`tinydb/middlewares.py`**: Middleware system, enables storage enhancement
- **`tinydb/operations.py`**: Update operations, provides field manipulation
- **`tinydb/utils.py`**: Utility classes, supports caching and hashing
- **`tinydb/version.py`**: Version constant, enables runtime version queries
- **`tinydb/mypy_plugin.py`**: Type checker plugin, enables IDE support

### Internal Collaboration

```
database.py → table.py → storages.py
                 → queries.py
                 → utils.py

queries.py → utils.py

middlewares.py → storages.py

mypy_plugin.py → utils.py
```

### Design Rationale Summary

TinyDB's architecture prioritizes:

1. **Simplicity**: Single file, no external dependencies, intuitive API
2. **Customizability**: Storage and middleware are pluggable
3. **Performance**: Query caching, persistent file handles
4. **Type Safety**: mypy plugin for IDE support
5. **Correctness**: fsync for durability, cache clearing for consistency

The modular design allows users to swap components (custom storage, middleware) while maintaining a simple default experience.