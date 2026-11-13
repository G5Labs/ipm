---
child_spec_ids: []
complexity: medium
component_id: tinydb/table
dependencies:
- tinydb/storages
- tinydb/utils
- tinydb/queries
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/table.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Table Module

## Overview
This module implements the core table functionality for TinyDB, a lightweight document-oriented database. The Table class provides methods for CRUD operations (Create, Read, Update, Delete) on documents, along with query caching for performance optimization. It acts as the central interface between users and the underlying storage layer.

## Behavior

### Document Class
The `Document` class extends `dict` to wrap document content with a `doc_id` attribute. It provides access to both document content (via dict interface) and its unique identifier.

### Table Class
The Table class is the primary interface for database operations:

- **Insert Operations**: `insert()` adds a single document, `insert_multiple()` adds multiple documents. Documents receive sequential IDs unless explicitly specified.
- **Query Operations**: `search()` returns all matching documents, `get()` returns single document or list, `all()` returns all documents, `count()` counts matches.
- **Update Operations**: `update()` modifies matching documents with new fields or a function, `update_multiple()` applies multiple updates, `upsert()` updates existing or inserts new.
- **Delete Operations**: `remove()` deletes matching documents, `truncate()` deletes all documents.
- **Query Cache**: LRU cache stores search results, automatically cleared on any write operation.

### Query Caching
The Table implements an LRU query cache using `tinydb.utils.LRUCache`. On search operations, results are cached using the query condition as key. The cache is automatically cleared on any write operation (insert, update, upsert, remove, truncate) to ensure data consistency.

### ID Generation
Document IDs are sequential integers starting from 1. The `_get_next_id()` method caches the next ID to avoid scanning the table on every insert.

### Storage Format Conversion
The storage layer uses string keys (for JSON compatibility), but the Table API uses integer keys. Conversion happens in `_update_table()` - NOT in `_read_table()`.

## Dependencies
- `tinydb.queries.QueryLike`: Protocol for query conditions
- `tinydb.storages.Storage`: Abstract storage interface
- `tinydb.utils.LRUCache`: Least Recently Used cache for query results
- `typing`: Type hints for the module

## Domain Context: Document Database Table Operations

### Read-Modify-Write Pattern
**Context**: TinyDB's storage interface only supports reading/writing the entire database at once
**Risk**: Concurrent modifications could lead to data loss or inconsistency
**Solution**: All modifications use `_update_table(updater)` pattern

**Implementation Pattern**:
```python
def _update_table(self, updater: Callable[[Dict[int, Mapping]], None]):
    # 1. Read entire database from storage
    tables = self._storage.read()
    
    # 2. Extract this table's data
    raw_table = tables.get(self.name, {})
    
    # 3. Convert string keys from storage to int keys for API
    table = {self.document_id_class(doc_id): doc 
             for doc_id, doc in raw_table.items()}
    
    # 4. Perform the update (modify table dict)
    updater(table)
    
    # 5. Convert int keys back to string keys for storage
    tables[self.name] = {str(doc_id): doc 
                         for doc_id, doc in table.items()}
    
    # 6. Write entire database back
    self._storage.write(tables)
    
    # 7. Clear query cache (data changed)
    self.clear_cache()
```

**Why This Pattern Matters**:
- Atomicity: All modifications happen within a single read-modify-write cycle
- Consistency: Cache invalidation happens immediately after write
- Middleware Support: The pattern allows intercepting modifications
- Storage Abstection: Hides storage format (string keys) from API users

### Query Cache Invalidation
**Context**: Query results become stale after data modifications
**Risk**: Returning cached results that no longer match current data
**Solution**: Clear entire cache on any write operation

**Critical Constraint**: This is a design decision, NOT an optimization
- Simpler approach: Track which queries affected by changes
- Why rejected: Complexity, edge cases, potential bugs
- Trade-off: Clears all cached queries even if unrelated to change
- This is NON-NEGOTIABLE for correctness

### ID Key Conversion Location
**Context**: Storage uses string keys (JSON), API uses integer keys
**Risk**: Converting in wrong location causes double-conversion or wrong types
**Solution**: Conversion in `_update_table()`, NOT in `_read_table()`

**Why in _update_table**:
- `_read_table()` returns raw storage format (string keys)
- Most operations don't need all documents converted
- Conversion happens only when modifying (update path)
- Get/Search convert at point of use (lazy conversion)

### Empty Table Representation
**Context**: `_read_table()` returns empty dict `{}` for empty/non-existent tables
**Risk**: Distinguishing "table doesn't exist" from "table is empty"
**Solution**: This is handled at storage layer, Table receives consistent format

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `Document(value: Mapping, doc_id: int)` - Document class extending dict
- [ ] `Document.doc_id: int` - Document identifier attribute
- [ ] `Table` - Main table class
- [ ] `Table.document_class = Document` - Class variable for document type
- [ ] `Table.document_id_class = int` - Class variable for ID type
- [ ] `Table.query_cache_class = LRUCache` - Class variable for cache type
- [ ] `Table.default_query_cache_capacity = 10` - Default cache size
- [ ] `Table.__init__(storage: Storage, name: str, cache_size: int = 10, persist_empty: bool = False)` - Constructor
- [ ] `Table.name: str` - Property returning table name
- [ ] `Table.storage: Storage` - Property returning storage instance
- [ ] `Table.insert(document: Mapping) -> int` - Insert single document
- [ ] `Table.insert_multiple(documents: Iterable[Mapping]) -> List[int]` - Insert multiple documents
- [ ] `Table.all() -> List[Document]` - Get all documents
- [ ] `Table.search(cond: QueryLike) -> List[Document]` - Search with condition
- [ ] `Table.get(cond: Optional[QueryLike] = None, doc_id: Optional[int] = None, doc_ids: Optional[List] = None)` - Get document(s)
- [ ] `Table.contains(cond: Optional[QueryLike] = None, doc_id: Optional[int] = None) -> bool` - Check existence
- [ ] `Table.update(fields: Union[Mapping, Callable[[Mapping], None]], cond: Optional[QueryLike] = None, doc_ids: Optional[Iterable[int]] = None) -> List[int]` - Update documents
- [ ] `Table.update_multiple(updates: Iterable[Tuple[...]]) -> List[int]` - Multiple updates
- [ ] `Table.upsert(document: Mapping, cond: Optional[QueryLike] = None) -> List[int]` - Update or insert
- [ ] `Table.remove(cond: Optional[QueryLike] = None, doc_ids: Optional[Iterable[int]] = None) -> List[int]` - Remove documents
- [ ] `Table.truncate() -> None` - Remove all documents
- [ ] `Table.count(cond: QueryLike) -> int` - Count matching documents
- [ ] `Table.clear_cache() -> None` - Clear query cache
- [ ] `Table.__len__() -> int` - Document count
- [ ] `Table.__iter__() -> Iterator[Document]` - Iterate documents
- [ ] `__all__` - Exports: ('Document', 'Table')

#### Required Behavior (NON-NEGOTIABLE)
- [ ] MUST use `_update_table(updater)` for ALL modifications (insert, update, remove, truncate)
- [ ] MUST call `clear_cache()` after EVERY write operation
- [ ] MUST raise ValueError when document is not a Mapping in insert()
- [ ] MUST raise ValueError when document with ID already exists in insert()
- [ ] MUST raise ValueError when document is not a Mapping in insert_multiple()
- [ ] MUST raise ValueError when document ID already exists in insert_multiple()
- [ ] MUST raise RuntimeError when neither cond nor doc_id provided to get()
- [ ] MUST raise RuntimeError when neither cond nor doc_id provided to contains()
- [ ] MUST raise RuntimeError when remove() called without cond or doc_ids
- [ ] MUST support callable fields in update() (function that modifies document)
- [ ] MUST convert int doc_id to string for storage lookup
- [ ] MUST convert string keys from storage to int for API
- [ ] MUST preserve document ID class type throughout operations
- [ ] MUST return None for missing document in get(doc_id=...) (NOT raise exception)
- [ ] MUST return empty list when no documents match in get(doc_ids=...)
- [ ] MUST support custom document_class via class variable
- [ ] MUST support custom document_id_class via class variable
- [ ] MUST support custom query_cache_class via class variable

#### Performance Requirements
- [ ] Query cache: O(1) lookup via LRU cache
- [ ] Query cache eviction: O(1) when capacity exceeded
- [ ] ID generation: O(1) when _next_id cached, O(n) otherwise (n = current max ID)
- [ ] Search: O(n) scan all documents, cache results for repeated queries

#### Error Handling Requirements
- [ ] MUST raise ValueError when insert() receives non-Mapping document
- [ ] MUST raise ValueError when insert() receives duplicate doc_id
- [ ] MUST raise ValueError when insert_multiple() receives non-Mapping document
- [ ] MUST raise ValueError when insert_multiple() receives duplicate doc_id
- [ ] MUST raise RuntimeError when get() called without cond, doc_id, or doc_ids
- [ ] MUST raise RuntimeError when contains() called without cond or doc_id
- [ ] MUST raise RuntimeError when remove() called without cond or doc_ids (use truncate() instead)
- [ ] MUST raise ValueError when upsert() without cond or Document.doc_id
- [ ] MUST return None (not raise) when get(doc_id=...) not found
- [ ] MUST return empty list when get(doc_ids=...) has no matches

## Architectural Rationale

### Updater Pattern for Atomic Modifications
**Pattern**: All table modifications use `_update_table(updater: Callable)`
**Rationale**:
- Ensures atomic read-modify-write operations on storage
- Enables middleware/caching layers to intercept updates
- Prevents race conditions in concurrent scenarios
- Allows transaction logging and undo/redo functionality

**Critical Constraint**: NEVER modify table data outside of `_update_table()`
- Downlift agents must NOT add direct `self._table` attribute
- Always read from storage via `_read_table()` inside updater
- This pattern is NON-NEGOTIABLE - simplification breaks atomicity

### Lazy Conversion Pattern
**Pattern**: Document ID conversion happens at API boundary, not storage boundary
**Rationale**:
- Storage format (string keys) preserved for efficiency
- Conversion happens only when needed (update path)
- Get operations convert at point of use
- Prevents double-conversion bugs

**Implementation**:
- `_read_table()` returns storage format: Dict[str, Mapping]
- `_update_table()` converts to API format before update, back to storage format after
- Search/Get convert individual documents lazily

### Query Cache Invalidation Strategy
**Pattern**: Clear entire cache on any write operation
**Rationale**:
- Simpler than tracking affected queries
- No false positives from stale cache
- Trade-off: Clears all cache even for unrelated changes
- This is an architectural decision, NOT premature optimization

### ID Assignment Strategy
**Pattern**: Sequential IDs with caching via `_next_id`
**Rationale**:
- Sequential IDs are simple and predictable
- Caching next ID avoids O(n) scan on every insert
- Reset on explicit ID insert to prevent reuse bugs

**Implementation Details**:
- On insert with explicit ID: Reset `_next_id` to None (will recalculate)
- On insert with auto ID: Use cached `_next_id`, increment for next
- On truncate: Reset `_next_id` to None

### Customization via Class Variables
**Pattern**: Table class uses class variables for customization
**Rationale**:
- Allows subclassing Table for custom document classes
- Supports different ID types (int, UUID, etc.)
- Enables different cache implementations
- No runtime configuration needed

**Customizable Components**:
- `document_class`: Default Document, can be subclassed
- `document_id_class`: Default int, can be changed
- `query_cache_class`: Default LRUCache, can be replaced
- `default_query_cache_capacity`: Default 10

## Detailed Method Specifications

### insert(document: Mapping) -> int
1. Validate document is Mapping, raise ValueError if not
2. Determine doc_id: use explicit ID from Document, or get next ID
3. Use `_update_table()` with updater that:
   - Raises ValueError if doc_id already exists
   - Converts document to dict and stores at doc_id key
4. Return doc_id

### insert_multiple(documents: Iterable[Mapping]) -> List[int]
1. Create empty doc_ids list
2. Use `_update_table()` with updater that:
   - Iterates through documents
   - Validates each is Mapping, raise ValueError if not
   - Uses explicit ID from Document or generates new ID
   - Raises ValueError on duplicate ID
   - Appends doc_id to doc_ids list
   - Stores converted document
3. Return doc_ids list

### search(cond: QueryLike) -> List[Document]
1. Check query cache for cond, return copy if found
2. Read table via `_read_table()`
3. Evaluate cond against each document
4. Convert matching documents to Document class with IDs
5. If query is cacheable, store results in cache
6. Return list of matching documents

### get(cond=None, doc_id=None, doc_ids=None) -> Optional[Document|List[Document]]
1. Read table via `_read_table()`
2. If doc_id provided: lookup by string key, return Document or None
3. If doc_ids provided: lookup each, return list of Documents
4. If cond provided: find first matching, return Document or None
5. If none provided: raise RuntimeError

### update(fields, cond=None, doc_ids=None) -> List[int]
1. Determine whether fields is callable or Mapping
2. Use `_update_table()` with updater that:
   - If doc_ids: iterate and update each
   - If cond: iterate all, update matching, collect IDs
   - If neither: update all documents
   - Handle callable fields (call function) or Mapping fields (update dict)
3. Return list of updated document IDs

### upsert(document, cond=None) -> List[int]
1. If document is Document with doc_id, extract it
2. If no doc_id and no cond, raise ValueError
3. Try update with document and cond
4. If update returns empty (no matches), insert document
5. Return list of affected document IDs

### remove(cond=None, doc_ids=None) -> List[int]
1. If doc_ids provided: use `_update_table()` to remove each
2. If cond provided: use `_update_table()` to find and remove matches
3. If neither: raise RuntimeError (use truncate() for all)
4. Return list of removed document IDs

### truncate() -> None
1. Use `_update_table()` with updater that clears table
2. Reset `_next_id` to None
3. Cache automatically cleared by _update_table

### _get_next_id() -> int
1. If _next_id is cached, return and increment it
2. Read table via `_read_table()`
3. If table empty, set next_id = 1, cache = 2
4. Otherwise, find max ID, set next_id = max + 1, cache = max + 2
5. Return next_id

### _read_table() -> Dict[str, Mapping]
1. Call storage.read()
2. If returns None, return {}
3. Try to get table by name, return {} if KeyError
4. Return raw table (string keys, not converted)

### _update_table(updater: Callable[[Dict[int, Mapping]], None]) -> None
1. Read tables from storage
2. Get or create table for this name
3. Convert string keys to document_id_class keys
4. Call updater(table)
5. Convert int keys back to strings
6. Write tables to storage
7. Clear query cache