---
child_spec_ids: []
complexity: medium
component_id: tinydb/storages
dependencies: []
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/storages.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Storage Implementations

## Overview

This module provides storage abstractions for TinyDB, a lightweight document-oriented database. It defines the `Storage` abstract base class and two concrete implementations: `JSONStorage` for persistent file-based storage and `MemoryStorage` for in-memory storage. The storage layer handles serialization/deserialization of database documents and provides durability guarantees for persistent storage.

## Behavior

### Module Functions

- `touch(path: str, create_dirs: bool) -> None`
  - Creates a file at the specified path if it doesn't exist
  - If `create_dirs=True`, creates all missing parent directories using `os.makedirs()`
  - Opens file in append mode ('a') which creates the file without modifying contents
  - Used internally to ensure storage file exists before opening

### Storage Base Class

The `Storage` abstract base class defines the interface all storage implementations must follow:

- `read() -> Optional[Dict[str, Dict[str, Any]]]`
  - Reads and deserializes the current database state
  - Returns `None` to indicate empty storage (NOT empty dict `{}`)
  - Subclasses MUST implement this method

- `write(data: Dict[str, Dict[str, Any]]) -> None`
  - Serializes and writes the entire database state to storage
  - Called after every database modification operation
  - Subclasses MUST implement this method

- `close() -> None`
  - Optional cleanup method for closing file handles, connections, etc.
  - Default implementation is a no-op (pass)
  - Called when database is closed

### JSONStorage Implementation

File-based JSON storage with the following characteristics:

- **Constructor**: `__init__(path: str, create_dirs: bool = False, encoding: Optional[str] = None, access_mode: str = 'r+', **kwargs)`
  - `path`: Location of the JSON file
  - `create_dirs`: Whether to create parent directories if missing
  - `encoding`: File encoding (passed to `open()`)
  - `access_mode`: File open mode ('r', 'rb', 'r+', 'rb+')
  - `**kwargs`: Additional arguments passed to `json.dumps()` on write

- **Persistent File Handle**: Maintains `self._handle` throughout instance lifetime for performance (avoids open/close per operation)

- **Read Behavior**:
  - Seeks to end of file to check size
  - Returns `None` if file is empty (size == 0)
  - Otherwise seeks to beginning and loads JSON

- **Write Behavior**:
  - Seeks to beginning of file
  - Serializes data using `json.dumps(data, **self.kwargs)`
  - Writes serialized data
  - **CRITICAL**: Calls `flush()` then `os.fsync()` to force data to physical disk
  - **CRITICAL**: Calls `truncate()` to remove stale data if file shrank

- **Access Mode Warnings**: Warns if mode is not 'r', 'rb', 'r+', or 'rb+' (dangerous modes like 'w', 'a', 'w+', 'a+' can cause data loss)

### MemoryStorage Implementation

In-memory storage with the following characteristics:

- **Constructor**: `__init__() -> None`
  - Simple initialization, sets `self.memory = None`

- **Read Behavior**: Returns `self.memory` (which may be `None` for empty)

- **Write Behavior**: Simply assigns `self.memory = data`

## Dependencies

This module has no external dependencies beyond Python standard library:
- `io`: For IOError exception
- `json`: For JSON serialization/deserialization
- `os`: For file operations, fsync, seek constants
- `warnings`: For access mode warnings
- `abc`: For ABCMeta and abstractmethod
- `typing`: For type hints (Dict, Any, Optional)

## Domain Context: Database File Persistence

### Data Durability Strategy

**Context**: Document database must survive crashes and power loss
**Risk**: Without proper durability, writes may remain in OS buffers
**Consequence**: Power loss or crash = data loss, database corruption
**Solution**: Force data to physical storage using fsync after writes

**Implementation Pattern**:
```python
def write(self, data: Dict[str, Dict[str, Any]]) -> None:
    # Step 1: Position at file start
    self._handle.seek(0)

    # Step 2: Write serialized data
    serialized = json.dumps(data, **self.kwargs)
    self._handle.write(serialized)

    # Step 3: CRITICAL - Force Python buffers to OS
    self._handle.flush()

    # Step 4: CRITICAL - Force OS buffers to physical disk
    os.fsync(self._handle.fileno())

    # Step 5: Remove stale data if file shrank
    self._handle.truncate()
```

**Why This Order Matters**:
- `flush()` BEFORE `fsync()`: Ensures Python buffers flushed to OS
- `fsync()` BEFORE `truncate()`: Ensures new data on disk before removing old
- `truncate()` LAST: Prevents corruption if process killed mid-write

**Failure Mode Without fsync**:
1. Application calls write({'users': {'1': {'name': 'Alice'}}})
2. Data written to OS page cache
3. Function returns (appears successful)
4. Power loss occurs before OS flushes cache
5. On restart: Old data still present OR file corrupted
6. Database integrity compromised

**Failure Mode Without truncate**:
- Old content: `{"users": {"1": {"name": "Bob"}, "2": {"name": "Charlie"}}}` (65 bytes)
- New content: `{"users": {"1": {"name": "Alice"}}}` (42 bytes)
- Without truncate: File still 65 bytes, trailing garbage: `...Alice"}}}me": "Charlie"}}}`
- Result: Invalid JSON, cannot read database

### Persistent File Handle Pattern

**Context**: Database operations (read/write) happen frequently
**Risk**: Opening/closing file for each operation is 10x slower
**Consequence**: Performance degradation, potential race conditions
**Solution**: Open file once in `__init__()`, reuse for all operations

**Performance Comparison**:
```python
# SLOW (open/close per operation):
def read(self):
    with open(self.path, 'r') as f:
        return json.load(f)

# FAST (persistent handle):
def read(self):
    self._handle.seek(0, os.SEEK_END)
    size = self._handle.tell()
    if not size: return None
    self._handle.seek(0)
    return json.load(self._handle)
```

### Empty Storage Representation

**Context**: Database must distinguish "never initialized" from "explicitly empty"
**Risk**: Returning `{}` for empty storage loses this distinction
**Consequence**: Cannot tell if database file is missing vs intentionally empty
**Solution**: `read()` returns `None` for empty/missing, `{}` only when explicitly set

**Semantics**:
- `None`: Storage never initialized OR file empty
- `{}`: Database was explicitly set to empty dict (valid state)
- Allows: `if storage.read() is None: initialize_database()`

### Access Mode Safety

**Context**: File access modes determine read/write behavior
**Risk**: Modes like 'w' (write/truncate) or 'a' (append) can cause data loss
**Consequence**: Opening in wrong mode can corrupt or erase database
**Solution**: Warn users when using dangerous modes, document safe modes

**Safe Modes**:
- `'r'`: Read-only, fails if file doesn't exist
- `'rb'`: Binary read-only
- `'r+'`: Read-write, fails if file doesn't exist
- `'rb+'`: Binary read-write

**Dangerous Modes** (trigger warning):
- `'w'`: Write-only, truncates existing file
- `'wb'`: Binary write-only
- `'a'`: Append-only, cannot overwrite existing content
- `'a+'`: Append-read

## Implementation Requirements

### Abstract Base Class Requirements

- `Storage` MUST use `ABCMeta` as metaclass (via `ABC`)
- `read()` and `write()` MUST be decorated with `@abstractmethod`
- `close()` has default implementation (no-op)
- Storage implementations MUST implement all abstract methods

### JSONStorage Specific Requirements

- MUST maintain persistent `self._handle` file object throughout instance lifetime
- MUST call `os.fsync(self._handle.fileno())` after EVERY write operation
- MUST call `self._handle.truncate()` after EVERY write operation
- MUST support `**kwargs` parameter passed through to `json.dumps(data, **self.kwargs)`
- MUST call `touch(path, create_dirs)` in `__init__()` if write mode enabled
- MUST return `None` from `read()` when storage is empty (NOT empty dict)
- MUST warn if access_mode is not in safe modes ('r', 'rb', 'r+', 'rb+')
- MUST raise `IOError` when write fails due to unsupported operation

### MemoryStorage Specific Requirements

- MUST initialize `self.memory = None` in `__init__()`
- MUST return `self.memory` directly in `read()` (no transformation)
- MUST simply assign `self.memory = data` in `write()`

### Module-Level Requirements

- `touch()` function MUST create parent directories if `create_dirs=True`
- `touch()` function MUST use append mode ('a') to avoid modifying existing content
- Module MUST export `Storage`, `JSONStorage`, `MemoryStorage` in `__all__`

## Success Criteria Checklist

### Public API Surface (MANDATORY)

- [ ] `touch(path: str, create_dirs: bool) -> None` - Module-level file creation utility
- [ ] `Storage` - Abstract base class with ABCMeta
- [ ] `Storage.read() -> Optional[Dict[str, Dict[str, Any]]]` - Returns None if empty
- [ ] `Storage.write(data: Dict[str, Dict[str, Any]]) -> None` - Writes database state
- [ ] `Storage.close() -> None` - Cleanup method (default no-op)
- [ ] `JSONStorage(Storage)` - File-based storage implementation
- [ ] `JSONStorage.__init__(path: str, create_dirs: bool = False, encoding: Optional[str] = None, access_mode: str = 'r+', **kwargs)` - ALL parameters required
- [ ] `JSONStorage.read() -> Optional[Dict[str, Dict[str, Any]]]` - Returns None for empty file
- [ ] `JSONStorage.write(data: Dict[str, Dict[str, Any]]) -> None` - Full write with fsync and truncate
- [ ] `JSONStorage.close() -> None` - Closes file handle
- [ ] `MemoryStorage(Storage)` - In-memory storage implementation
- [ ] `MemoryStorage.__init__() -> None` - No parameters
- [ ] `MemoryStorage.read() -> Optional[Dict[str, Dict[str, Any]]]` - Returns self.memory
- [ ] `MemoryStorage.write(data: Dict[str, Dict[str, Any]]) -> None` - Simple assignment

### Required Behavior (NON-NEGOTIABLE)

- [ ] MUST maintain persistent `self._handle` file object in JSONStorage throughout instance lifetime
- [ ] MUST call `os.fsync(self._handle.fileno())` after EVERY write in JSONStorage
- [ ] MUST call `self._handle.truncate()` after EVERY write in JSONStorage
- [ ] MUST support `**kwargs` parameter passed through to `json.dumps(data, **self.kwargs)`
- [ ] MUST call `touch(path, create_dirs)` in JSONStorage.__init__() if write mode enabled
- [ ] MUST return None from read() when storage is empty (NOT empty dict)
- [ ] MUST warn if access_mode is not in ('r', 'rb', 'r+', 'rb+')
- [ ] MUST raise IOError when write fails due to unsupported operation
- [ ] MUST use ABCMeta metaclass for Storage base class
- [ ] MUST mark read() and write() as abstract methods in Storage

### Performance Requirements

- [ ] Persistent file handle for 10x faster operations (JSONStorage)
- [ ] O(1) read/write for in-memory storage (MemoryStorage)
- [ ] Single seek to check file size before reading (JSONStorage)

### Error Handling Requirements

- [ ] MUST raise IOError when JSONStorage.write() fails with UnsupportedOperation
- [ ] MUST warn when access_mode is dangerous ('w', 'a', 'w+', 'a+')
- [ ] MUST handle empty file by returning None (not raising exception)
- [ ] MUST handle missing parent directories if create_dirs=True in touch()