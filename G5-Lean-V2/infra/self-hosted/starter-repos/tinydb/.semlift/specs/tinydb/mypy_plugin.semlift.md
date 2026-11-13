---
child_spec_ids: []
complexity: medium
component_id: tinydb/mypy_plugin
dependencies:
- tinydb/utils
granularity_score: 0.5
hierarchy_level: 1
is_root: false
language: python
parent_spec_id: tinydb_aggregate
source_files:
- tinydb/mypy_plugin.py
type: uplift
version: 1.0.0
---

# Design Doc for TinyDB mypy Plugin

## Overview
This module provides a mypy plugin for TinyDB that enables proper type inference when using dynamic class definitions. The plugin specifically handles the `with_typehint` utility from `tinydb.utils`, allowing mypy to correctly resolve type information for dynamically created classes at type-checking time.

## Behavior
The plugin hooks into mypy's dynamic class definition system to intercept and properly handle the `tinydb.utils.with_typehint` function call. When mypy encounters a dynamic class definition through this utility, the plugin resolves the type and adds it to the symbol table, enabling accurate type checking and IDE support.

### Key Behaviors
1. **Plugin Initialization**: The `TinyDBPlugin` class initializes with an empty `named_placeholders` dictionary to track placeholder types.
2. **Dynamic Class Hook**: The `get_dynamic_class_hook` method returns a hook function when the fullname matches `tinydb.utils.with_typehint`.
3. **Type Resolution**: Inside the hook, the plugin extracts the class argument, looks up its fully qualified name, and adds it to the symbol table.
4. **Plugin Entry Point**: The `plugin` function serves as the mypy plugin entry point, returning the `TinyDBPlugin` class.

## Dependencies
- **typing**: TypeVar, Optional, Callable, Dict for type hints
- **mypy.nodes**: NameExpr for AST node handling
- **mypy.options**: Options for plugin configuration
- **mypy.plugin**: Plugin base class and DynamicClassDefContext
- **tinydb.utils**: The target utility being type-checked (with_typehint function)

## Domain Context: Type Checker Plugins

### Purpose of mypy Plugins
**Context**: Dynamic class creation patterns break static type analysis because the class doesn't exist in the source code at definition time.
**Risk**: Without plugin support, mypy cannot resolve types for dynamically created classes, leading to "module has no attribute" errors and lost type information.
**Solution**: The plugin intercepts the dynamic class definition and provides the actual type to mypy's type system.

**Implementation Pattern**:
```python
def get_dynamic_class_hook(self, fullname: str) -> CB[DynamicClassDef]:
    if fullname == 'tinydb.utils.with_typehint':
        def hook(ctx: DynamicClassDefContext):
            # Extract the class from the call arguments
            klass = ctx.call.args[0]
            assert isinstance(klass, NameExpr)
            
            # Get the fully qualified name
            type_name = klass.fullname
            assert type_name is not None
            
            # Look up and add to symbol table
            qualified = self.lookup_fully_qualified(type_name)
            assert qualified is not None
            ctx.api.add_symbol_table_node(ctx.name, qualified)
        
        return hook
    
    return None
```

**Why This Pattern Matters**:
- The hook receives a `DynamicClassDefContext` containing the call information
- The first argument to `with_typehint(cls)` is the class to be type-checked
- `lookup_fully_qualified` resolves the type in mypy's symbol table
- `add_symbol_table_node` makes the type available under the dynamic class name

### Plugin Registration
**Context**: mypy needs a way to discover and load plugins.
**Solution**: The module exports a `plugin` function that returns the plugin class.

```python
def plugin(_version: str):
    return TinyDBPlugin
```

The `_version` parameter allows the plugin to check compatibility with mypy versions.

## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `TinyDBPlugin(Plugin)` - Main plugin class extending mypy's Plugin base
- [ ] `TinyDBPlugin.__init__(self, options: Options) -> None` - Initialize plugin with mypy options
- [ ] `TinyDBPlugin.get_dynamic_class_hook(self, fullname: str) -> Optional[Callable[[DynamicClassDefContext], None]]` - Return hook for dynamic class definitions
- [ ] `plugin(_version: str) -> Type[TinyDBPlugin]` - Plugin entry point function
- [ ] `T: TypeVar` - Type variable for generic callbacks
- [ ] `CB: TypeAlias` - Type alias for optional callbacks
- [ ] `DynamicClassDef: TypeAlias` - Type alias for DynamicClassDefContext

#### Required Behavior (NON-NEGOTIABLE)
- [ ] MUST return hook only when fullname equals 'tinydb.utils.with_typehint'
- [ ] MUST extract first argument from ctx.call.args as the target class
- [ ] MUST look up fully qualified name using lookup_fully_qualified
- [ ] MUST add resolved type to symbol table via add_symbol_table_node
- [ ] MUST return None for non-matching fullnames (fallback to default behavior)
- [ ] MUST maintain named_placeholders dict for tracking placeholder types

#### Type Checker Integration
- [ ] Plugin must be registered via plugin() function returning TinyDBPlugin class
- [ ] Plugin must accept Options in __init__ and pass to super().__init__
- [ ] Plugin must implement get_dynamic_class_hook returning Optional[Callable]

### Implementation Details

#### TinyDBPlugin Class
```python
class TinyDBPlugin(Plugin):
    def __init__(self, options: Options):
        super().__init__(options)
        self.named_placeholders: Dict[str, str] = {}

    def get_dynamic_class_hook(self, fullname: str) -> CB[DynamicClassDef]:
        if fullname == 'tinydb.utils.with_typehint':
            def hook(ctx: DynamicClassDefContext):
                klass = ctx.call.args[0]
                assert isinstance(klass, NameExpr)

                type_name = klass.fullname
                assert type_name is not None

                qualified = self.lookup_fully_qualified(type_name)
                assert qualified is not None

                ctx.api.add_symbol_table_node(ctx.name, qualified)

            return hook

        return None
```

#### Plugin Entry Point
```python
def plugin(_version: str):
    return TinyDBPlugin
```

## Architectural Rationale

### Design Patterns
- **Plugin Pattern**: Extends mypy's Plugin base class to customize type checking behavior
- **Hook Pattern**: Uses callback hooks for specific AST node types (dynamic class definitions)
- **Type Alias Pattern**: Uses TypeVar and TypeAlias for reusable type definitions

### Why a mypy Plugin is Necessary
Dynamic class creation with `with_typehint` creates classes at runtime that don't exist in the source code. Without the plugin:
1. mypy cannot find the class definition
2. Type inference fails for instances of dynamically created classes
3. IDE autocompletion doesn't work for these types

The plugin bridges runtime behavior with static type analysis by telling mypy where to find the actual type.

### Critical Constraints
- The hook MUST only activate for `tinydb.utils.with_typehint` - other dynamic classes should use default behavior
- Type resolution MUST use `lookup_fully_qualified` to properly resolve the type in mypy's symbol table
- The plugin function MUST accept a `_version` parameter (mypy passes the version string)