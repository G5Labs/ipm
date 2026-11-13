---
child_spec_ids: []
complexity: medium
component_id: tinydb/database
dependencies:
- tinydb/storages
- tinydb/utils
- tinydb/table
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/database.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Database Module

## Overview
This module implements the main TinyDB class, which is the primary entry point for the TinyDB document database. It manages storage instances, provides table access and management, and forwards operations to a default table for convenience. The TinyDB class acts as both a database manager and a convenient interface to the default table.

## Behavior

### TinyDB Class
The TinyDB class provides the following functionality:

- **Storage Management**: Initializes and manages the storage instance that persists database data
- **Table Management**: Creates, retrieves, and drops tables by name
- **Default Table Access**: Forwards unknown method calls and property access to the default table
- **Context Manager**: Supports `with` statement for automatic cleanup
- **Repr**: Provides informative string representation showing tables and document counts

### Data Storage Model
TinyDB stores data using a storage class that provides persistence for a dict containing all tables and their documents:

```python
{
    '_default': {
        0: {document...},
        1: {document...},
    },
    'table1': {
        ...
    }
}
```

Each entry uses the table name as key and a dict of documents as value. Document dicts contain document IDs as keys and documents as values.

### Customization Points
The class supports customization via class variables:
- `table_class`: The class used to create table instances (default: Table)
- `default_table_name`: Name of the default table (default: '_default')
- `default_storage_class`: Storage class used when none specified (default: JSONStorage)

### Forwarding Pattern
TinyDB implements `__getattr__` to forward all unknown attribute access to the default table. This allows direct usage like `db.insert({'field': 'value'})` which delegates to the default table.

Magic methods (`__len__`, `__iter__`) are forwarded manually since `__getattr__` doesn't handle them.

## Dependencies
- `tinydb.storages.Storage`: Abstract storage interface
- `tinydb.table.Table`: Table class for database operations
- `tinydb.utils.with_typehint`: Function to add type hints from base class

## Domain Context: Database Instance Management

### Storage Initialization Pattern
**Context**: TinyDB must initialize storage before any operations
**Risk**: Operations fail if storage not properly initialized
**Solution**: Storage is created in `__init__` from provided class and arguments

**Implementation Pattern**:
```python
def __init__(self, *args, **kwargs) -> None:
    storage = kwargs.pop('storage', self.default_storage_class)
    self._storage: Storage = storage(*args, **kwargs)
    self._opened = True
    self._tables: Dict[str, Table] = {}
```

**Design Rationale**:
- Storage class is passed as argument, not instantiated separately
- All remaining args/kwargs passed to storage constructor
- Allows flexible storage configuration without subclassing

### Table Instance Caching
**Context**: Table instances should be reused for the same name
**Risk**: Creating new instances each time loses state and performance
**Solution**: Cache table instances in `self._tables` dict

**Implementation Pattern**:
```python
def table(self, name: str, **kwargs) -> Table:
    if name in self._tables:
        return self._tables[name]
    table = self.table_class(self.storage, name, **kwargs)
    self._tables[name] = table
    return table
```

**Why This Matters**:
- Table instances maintain their own state (cache, etc.)
- Reusing instances preserves query cache across operations
- Ensures consistent behavior within same TinyDB instance

### Default Table Forwarding
**Context**: Users expect to call methods directly on db object
**Risk**: Requiring `db.table('_default').insert()` is verbose
**Solution**: `__getattr__` forwards to default table

**Implementation Pattern**:
```python
def __getattr__(self, name):
    return getattr(self.table(self.default_table_name), name)
```

**Magic Methods Handling**:
- `__getattr__` doesn't handle dunder methods (e.g., `__len__`)
- Must manually forward: `def __len__(self): return len(self.table(self.default_table_name))`

### Context Manager for Cleanup
**Context**: Storage may need cleanup (closing file handles)
**Risk**: Users forget to call close()
**Solution**: Support `with` statement for automatic cleanup

**Implementation Pattern**:
```python
def __enter__(self):
    return self

def __exit__(self, *args):
    if self._opened:
        self.close()
```

**Usage**:
```python
with TinyDB('data.json') as db:
    db.insert({'foo': 'bar'})
# close() called automatically on exit
```

### Table Names from Storage
**Context**: Need to enumerate all tables in database
**Risk**: In-memory `_tables` dict doesn't reflect persisted tables
**Solution**: Read table names from storage, not internal cache

**Implementation Pattern**:
```python
def tables(self) -> Set[str]:
    return set(self.storage.read() or {})
```

**Why This Matters**:
- Storage is the source of truth for persisted tables
- Internal `_tables` only contains accessed tables
- `tables()` returns all tables including never-accessed ones

### Table Drop Operations
**Context**: Need to remove tables (irreversibly)
**Risk**: Dropping wrong table causes data loss
**Solution**: Separate methods for dropping one vs all tables

**Implementation Pattern**:
```python
def drop_table(self, name: str) -> None:
    # Remove from cache if present
    if name in self._tables:
        del self._tables[name]
    
    # Read and modify storage
    data = self.storage.read()
    if data is None or name not in data:
        return
    
    del data[name]
    self.storage.write(data)

def drop_tables(self) -> None:
    # Write empty dict to storage
    self.storage.write({})
    # Clear cache
    self._tables.clear()
```

**Key Behaviors**:
- `drop_table()`: Only removes from storage if table exists
- `drop_tables()`: Removes all tables, clears cache
- Both are irreversible (documented with warning)

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `TinyDB(*args, **kwargs)` - Main database class
- [ ] `TinyDB.table_class = Table` - Class variable for table creation
- [ ] `TinyDB.default_table_name = '_default'` - Default table name
- [ ] `TinyDB.default_storage_class = JSONStorage` - Default storage class
- [ ] `TinyDB.__init__(*args, **kwargs) -> None` - Initialize database
- [ ] `TinyDB.__repr__() -> str` - String representation with table info
- [ ] `TinyDB.table(name: str, **kwargs) -> Table` - Get or create table
- [ ] `TinyDB.tables() -> Set[str]` - Get all table names
- [ ] `TinyDB.drop_tables() -> None` - Drop all tables (irreversible)
- [ ] `TinyDB.drop_table(name: str) -> None` - Drop specific table (irreversible)
- [ ] `TinyDB.storage -> Storage` - Property to access storage instance
- [ ] `TinyDB.close() -> None` - Close database and storage
- [ ] `TinyDB.__enter__() -> TinyDB` - Context manager entry
- [ ] `TinyDB.__exit__(*args) -> None` - Context manager exit
- [ ] `TinyDB.__getattr__(name) -> Any` - Forward to default table
- [ ] `TinyDB.__len__() -> int` - Document count in default table
- [ ] `TinyDB.__iter__() -> Iterator[Document]` - Iterate default table documents
- [ ] `TableBase: Type[Table]` - Type-hinted base class for IDE support

#### Required Behavior (NON-NEGOTIABLE)
- [ ] MUST initialize storage from `storage` kwarg or default_storage_class
- [ ] MUST pass all *args and **kwargs (except storage) to storage constructor
- [ ] MUST cache table cache table cache table cache_defaultname:
    [ ] `self
- [ ] MUST forward __len__ to default table
- [ ] MUST forward __iter__ to default table
- [ ] MUST use with_typehint to create TableBase with type hints

#### Performance Requirements
- [ ] Table instance caching prevents repeated instantiation
- [ ] Table names read from storage once per call (not cached)
- [ ] Default table forwarding is O(1) lookup

#### Error Handling Requirements
- [ ] MUST handle storage.read() returning None in tables()
- [ ] MUST handle missing table name in drop_table() gracefully
- [ ] MUST ensure close() is idempotent (can be called multiple times)

## Architectural Rationale

### Database as Facade Pattern
**Pattern**: TinyDB acts as facade to storage and table operations
**Rationale**:
- Provides single entry point for all database operations
- Hides storage complexity from users
- Enables convenient default table access

**Trade-offs**:
- Forwarding via `__getattr__` can make debugging harder
- Magic methods must be manually forwarded
- Benefit: Simple API outweighs debugging complexity

### Storage-Agnostic Design
**Pattern**: TinyDB works with any Storage implementation
**Rationale**:
- Allows custom storage backends (Redis, SQLite, etc.)
- Storage interface is simple (read/write/close)
- Enables testing with MemoryStorage

**Implementation**:
- Accept storage class in constructor
- Pass all args/kwargs to storage
- Never assume specific storage behavior

### Class-Level Customization
**Pattern**: Use class variables for customization
**Rationale**:
- Subclassing for customization is more Pythonic
- No complex configuration objects needed
- Easy to override in subclasses

**Customizable Components**:
- `table_class`: Change Table implementation
- `default_table_name`: Change default table name
- `default_storage_class`: Change default storage

### Type Hinting Support
**Pattern**: Use with_typehint to add Table methods to TinyDB
**Rationale**:
- IDEs need TinyDB to have Table methods for autocomplete
- with_typehint copies method signatures without runtime overhead
- Enables PyCharm, Pyright, MyPy support

**Implementation**:
```python
TableBase: Type[Table] = with_typehint(Table)
class TinyDB(TableBase):
    ...
```

This makes TinyDB appear to IDEs as having all Table methods.

## Detailed Method Specifications

### __init__(*args, **kwargs)
1. Pop 'storage' from kwargs, default to default_storage_class
2. Instantiate storage with remaining args/kwargs
3. Set _opened = True
4. Initialize _tables = {}

### table(name: str, **kwargs) -> Table
1. If name in _tables, return cached instance
2. Create new table using table_class(self.storage, name, **kwargs)
3. Cache in _tables[name]
4. Return table

### tables() -> Set[str]
1. Call self.storage.read()
2. If None, return empty set
3. Return set(data.keys())

### drop_tables() -> None
1. Write empty dict to storage
2. Clear _tables cache

### drop_table(name: str) -> None
1. If name in _tables, delete from cache
2. Read data from storage
3. If data is None, return (nothing to do)
4. If name not in data, return (table doesn't exist)
5. Delete name from data
6. Write data back to storage

### close() -> None
1. Set _opened = False
2. Call self.storage.close()

### __enter__() -> TinyDB
1. Return self

### __exit__(*args) -> None
1. If self._opened is True, call self.close()

### __getattr__(name) -> Any
1. Get default table: self.table(self.default_table_name)
2. Return getattr(default_table, name)

### __len__() -> int
1. Return len(self.table(self.default_table_name))

### __iter__() -> Iterator[Document]
1. Return iter(self.table(self.default_table_name))