---
child_spec_ids: []
complexity: medium
component_id: tinydb/queries
dependencies:
- tinydb/utils
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/queries.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB Query Interface

## Overview
This module provides TinyDB's query building and evaluation system. It allows constructing complex queries using a fluent, chainable API that can be combined with logical operators (AND, OR, NOT). Queries are evaluated against documents (dictionaries) to determine matches. The query system supports caching for performance optimization through stable hash values.

## Behavior

### Query Building Pattern
Queries are built using the `Query` class which acts as a builder. Fields are accessed via attribute or item notation, creating a query path that is resolved when the query is evaluated against a document.

```python
# ORM-like usage
User = Query()
db.search(User.name == 'John Doe')
db.search(User['logged-in'] == True)

# Classical usage
db.search(where('value') == True)
```

### Query Composition
Query instances support logical composition:
- `&` (AND): Both queries must match
- `|` (OR): Either query must match
- `~` (NOT): Query must not match

### Query Execution
Queries are callable objects that accept a document (Mapping) and return a boolean indicating whether the document matches the query. Path resolution traverses nested dictionary keys.

### Caching System
Queries implement `__hash__` to enable caching. Cacheable queries have stable hash values. The `is_cacheable()` method indicates whether a query can be cached. Queries containing non-deterministic functions (via `.test()` or `.map()`) are not cacheable.

## Dependencies
- `re`: Regular expression matching for `.matches()` and `.search()` methods
- `tinydb.utils.freeze`: For creating stable hashes of nested structures

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `QueryLike` - Protocol class defining query interface with `__call__(value: Mapping) -> bool` and `__hash__() -> int`
- [ ] `Query` - Main query builder class
- [ ] `Query()` - Constructor creating empty query with no path
- [ ] `Query._path: Tuple` - Internal path tuple for field access
- [ ] `Query._hash: Optional[Tuple]` - Internal hash value for caching
- [ ] `Query.__getattr__(item: str) -> Query` - Field access via attribute
- [ ] `Query.__getitem__(item: str) -> Query` - Field access via index
- [ ] `Query.__hash__() -> int` - Hash method
- [ ] `Query.__repr__() -> str` - String representation
- [ ] `Query._generate_test(test: Callable, hashval: Tuple, allow_empty_path: bool = False) -> QueryInstance` - Internal method to create query instances
- [ ] `Query.__eq__(rhs: Any) -> QueryInstance` - Equality test
- [ ] `Query.__ne__(rhs: Any) -> QueryInstance` - Inequality test
- [ ] `Query.__lt__(rhs: Any) -> QueryInstance` - Less than test
- [ ] `Query.__le__(rhs: Any) -> QueryInstance` - Less than or equal test
- [ ] `Query.__gt__(rhs: Any) -> QueryInstance` - Greater than test
- [ ] `Query.__ge__(rhs: Any) -> QueryInstance` - Greater than or equal test
- [ ] `Query.exists() -> QueryInstance` - Check key exists
- [ ] `Query.matches(regex: str, flags: int = 0) -> QueryInstance` - Full regex match
- [ ] `Query.search(regex: str, flags: int = 0) -> QueryInstance` - Partial regex search
- [ ] `Query.test(func: Callable[[Mapping], bool], *args) -> QueryInstance` - Custom test function
- [ ] `Query.any(cond: Union[QueryInstance, List[Any]]) -> QueryInstance` - Any element matches
- [ ] `Query.all(cond: Union[QueryInstance, List[Any]]) -> QueryInstance` - All elements match
- [ ] `Query.one_of(items: List[Any]) -> QueryInstance` - Value in list
- [ ] `Query.fragment(document: Mapping) -> QueryInstance` - Partial document match
- [ ] `Query.noop() -> QueryInstance` - Always true query
- [ ] `Query.map(fn: Callable[[Any], Any]) -> Query` - Add function to path
- [ ] `QueryInstance` - Base class for query evaluation
- [ ] `QueryInstance.__init__(test: Callable[[Mapping], bool], hashval: Optional[Tuple])` - Constructor
- [ ] `QueryInstance.is_cacheable() -> bool` - Check if query can be cached
- [ ] `QueryInstance.__call__(value: Mapping) -> bool` - Evaluate query
- [ ] `QueryInstance.__hash__() -> int` - Hash for caching
- [ ] `QueryInstance.__repr__() -> str` - String representation
- [ ] `QueryInstance.__eq__(other: object) -> bool` - Equality comparison
- [ ] `QueryInstance.__and__(other: QueryInstance) -> QueryInstance` - AND operation
- [ ] `QueryInstance.__or__(other: QueryInstance) -> QueryInstance` - OR operation
- [ ] `QueryInstance.__invert__() -> QueryInstance` - NOT operation
- [ ] `where(key: str) -> Query` - Shorthand for Query()[key]
- [ ] `is_sequence(obj) -> bool` - Helper to check if object is iterable
- [ ] `__all__` - Module exports: ('Query', 'QueryLike', 'where')

#### Required Behavior (NON-NEGOTIABLE)
- [ ] Query MUST raise RuntimeError when evaluated with empty path (notest function)
- [ ] Query._generate_test() MUST raise ValueError when path is empty and allow_empty_path is False
- [ ] Query path resolution MUST catch KeyError and TypeError, returning False on exception
- [ ] Query.matches() MUST return False for non-string values
- [ ] Query.search() MUST return False for non-string values
- [ ] Query.any() MUST use is_sequence() to check if value is iterable
- [ ] Query.all() MUST use is_sequence() to check if value is iterable
- [ ] Query.one_of() MUST use `value in items` for membership test
- [ ] Query.fragment() MUST check all key-value pairs match (allow_empty_path=True)
- [ ] Query.map() MUST set _hash to None (non-cacheable due to potential mutability)
- [ ] QueryInstance.__and__ MUST use frozenset for hash (commutative property)
- [ ] QueryInstance.__or__ MUST use frozenset for hash (commutative property)
- [ ] QueryInstance.__hash__ MUST hash the internal _hash tuple
- [ ] Query.__getattr__ MUST create new Query instance and append to path
- [ ] Query.__getitem__ MUST delegate to __getattr__ for field access

#### Performance Requirements
- [ ] Query path resolution: O(n) where n is path depth
- [ ] Query hashing: O(1) for simple queries, O(n) for composed queries (frozenset)
- [ ] Query evaluation: O(1) for simple comparisons, O(n) for any/all operations

## Architectural Rationale

### Query Builder Pattern
**Pattern**: Builder pattern with fluent interface
**Rationale**: Allows composing complex queries from simple conditions
**Constraints**: Each operation returns new QueryInstance, preserving immutability

### Path-Based Field Access
**Pattern**: __getattr__ and __getitem__ for field access
**Rationale**: Enables ORM-like syntax `Query().field.subfield`
**Implementation**: Stores path as tuple, resolves at evaluation time

### Stable Hashing for Caching
**Pattern**: Hash-based query identification using frozen structures
**Rationale**: Enables query result caching in Table class
**Trade-off**: Non-deterministic queries (custom functions) cannot be cached

### Commutative Operations
**Pattern**: frozenset for AND/OR hash values
**Rationale**: `a & b == b & a` and `a | b == b | a` should have same hash
**Implementation**: frozenset ignores order, ensuring consistent hashes

### Empty Path Handling
**Pattern**: allow_empty_path parameter in _generate_test
**Rationale**: Fragment queries need empty path to match entire document
**Constraint**: Regular queries require path, fragment/allow_empty queries don't

### Query vs QueryInstance Separation
**Pattern**: Query (builder) vs QueryInstance (executable)
**Rationale**: Query builds the query, QueryInstance executes it
**Why**: Builder can be modified (add path), Instance is immutable once created

## Domain Context: Query Evaluation

### Path Resolution
**Context**: Queries must traverse nested dictionary structures
**Risk**: Missing keys or type errors during traversal
**Solution**: Try-except block catching KeyError and TypeError, returning False

**Implementation Pattern**:
```python
def runner(value):
    try:
        for part in self._path:
            if isinstance(part, str):
                value = value[part]
            else:
                value = part(value)
    except (KeyError, TypeError):
        return False
    else:
        return test(value)
```

### Regex Safety
**Context**: User-provided regex patterns must handle non-string values
**Risk**: TypeError when calling re.match on non-string
**Solution**: Explicit isinstance check, return False for non-strings

### Sequence Handling
**Context**: any() and all() operate on lists/sequences within documents
**Risk**: TypeError when iterating over non-sequences
**Solution**: is_sequence() check before iteration
```python
def test(value):
    return is_sequence(value) and any(cond(e) for e in value)
```