---
child_spec_ids: []
complexity: medium
component_id: tinydb/operations
dependencies: []
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/operations.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Document Update Operations

## Overview
A collection of update operations for TinyDB documents. These operations are used to transform documents during update queries, enabling field manipulation like deletion, addition, subtraction, setting values, and increment/decrement operations. Each operation returns a transform function that takes a document and modifies it in-place.

## Behavior
The module provides six factory functions that each return a `transform` callable. These transform functions are used by TinyDB's update mechanism to modify documents matching a query condition. The operations follow a consistent pattern where each function accepts field name and optional value parameters, then returns a function that performs the actual document modification.

### Operation Details
- **delete(field)**: Removes the specified field from the document entirely using `del doc[field]`
- **add(field, n)**: Adds `n` to the numeric value of the field using `+=` operator
- **subtract(field, n)**: Subtracts `n` from the numeric value of the field using `-=` operator
- **set(field, val)**: Sets the field to an arbitrary value using assignment
- **increment(field)**: Shortcut for adding 1 to a field (equivalent to `add(field, 1)`)
- **decrement(field)**: Shortcut for subtracting 1 from a field (equivalent to `subtract(field, 1)`)

## Dependencies
No external dependencies. This is a pure Python module with no imports.

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `delete(field: str) -> Callable[[dict], None]` - Returns transform function that deletes the specified field from document
- [ ] `add(field: str, n: Union[int, float]) -> Callable[[dict], None]` - Returns transform function that adds n to field value
- [ ] `subtract(field: str, n: Union[int, float]) -> Callable[[dict], None]` - Returns transform function that subtracts n from field value
- [ ] `set(field: str, val: Any) -> Callable[[dict], None]` - Returns transform function that sets field to val
- [ ] `increment(field: str) -> Callable[[dict], None]` - Returns transform function that increments field by 1
- [ ] `decrement(field: str) -> Callable[[dict], None]` - Returns transform function that decrements field by 1

#### Required Behavior (NON-NEGOTIABLE)
- [ ] Each operation function MUST return a callable (transform function)
- [ ] Transform functions MUST accept a single `doc` parameter (dict-like mapping)
- [ ] Transform functions MUST modify the document in-place (mutate the passed document)
- [ ] Transform functions MUST NOT return any value (return None implicitly)
- [ ] `delete` operation MUST use `del` statement to remove field
- [ ] `add/subtract` operations MUST use in-place operators (`+=`, `-=`)
- [ ] `increment` MUST add exactly 1 to the field value
- [ ] `decrement` MUST subtract exactly 1 from the field value

#### Usage Pattern
The operations are designed to be used with TinyDB's update method:
```python
db.update(delete('foo'), where('foo') == 2)  # Delete field 'foo' where foo == 2
db.update(add('count', 5), where('id') == 1) # Add 5 to 'count' where id == 1
db.update(increment('visits'), where('id') == 1) # Increment 'visits' by 1
```

## Domain Context
This module provides document transformation utilities for a document-oriented database. The operations follow the "transform function" pattern where each operation factory returns a callable that will be applied to each matching document. This enables composable, reusable update operations that integrate with TinyDB's query system.

### Design Pattern: Transform Factory
Each operation follows the factory pattern:
1. Accept operation parameters (field name, optional value)
2. Return an inner function (transform) that captures these parameters via closure
3. The transform function is called by TinyDB for each document matching the query

This pattern allows:
- Lazy evaluation of operation configuration
- Clean separation between operation definition and execution
- Easy composition of multiple operations

### Error Handling
The operations assume documents are mutable mappings with the specified field present. They do not perform validation - errors will propagate naturally from the underlying operations (KeyError for missing fields, TypeError for invalid operations).

## Architectural Rationale
The transform function pattern is chosen for simplicity and integration with TinyDB's update mechanism. Each function returns a callable that TinyDB's Table.update() method can apply to each document in the result set. This avoids storing state in the operations themselves and keeps them stateless and reusable.