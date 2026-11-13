# G5 Prompt Architecture

> **Canonical reference for agent prompt construction, templates, and dialog summarization.**
> Cross-references: AGENTS.md §5 (system prompt architecture), §10 (dialog prepopulation), §11 (dialog compaction).

---

## 0. Overview

G5 agents receive their instructions through a **Jinja2 template pipeline** managed by the `PromptManager`. Every agent session — uplift, downlift, review, or chat — is seeded with a **system prompt** (persona + constraints) and a **user prompt** (task-specific instructions + context). These are assembled from composable templates organized into four categories:

| Category | Directory | Purpose |
|----------|-----------|---------|
| **System** | `templates/system/` | Agent persona, workspace rules, reasoning patterns |
| **User** | `templates/user/` | Task instructions, context injection, completion requirements |
| **Components** | `templates/components/` | Reusable guidance blocks (uplift, downlift, format, tools) |
| **Planning** | `templates/planning/` | Architectural decomposition and D4 spec generation |

The PromptManager also loads **sample specifications** from `examples/sample_specs/` as few-shot exemplars injected into prompts at configurable compression levels.

---

## 1. PromptManager

The `PromptManager` is the central orchestrator that selects templates, builds rendering context, and returns `Message[]` arrays ready for the agent loop.

### 1.1 Entry Points

| Method | Agent Type | Returns |
|--------|-----------|---------|
| `create_constrained_prompt(task_context)` | Constrained (default) | `[system, user, ?task_msg]` |
| `create_react_prompt(task_context)` | ReAct | `[system, user, ?task_msg]` |
| `create_chat_prompt(task_context, history, request)` | Chat | `[system, user]` |
| `create_prompt(agent_type, task_context, ...)` | Dispatcher | Routes to above |

D4 planning variants:
| Method | Purpose |
|--------|---------|
| `_create_d4_architectural_planning_prompt(...)` | Root-level architectural decomposition |
| `_create_d4_component_spec_prompt(...)` | Component-level spec from parent plan |
| `_create_d4_leaf_spec_prompt(...)` | File-level spec from component plan |

### 1.2 Template Context

The `_build_template_context()` method assembles the full rendering context passed to Jinja2:

```python
context = {
    # Task identity
    "task_type":                  task_context.task_type,          # uplift | downlift | analysis | ...
    "task_id_info":               "Task ID: <id>",                # for complete_task calls
    "task_description":           task_context.task_description,

    # Source & workspace
    "source_path_info":           "Source files: [...]",
    "workspace_context_info":     "...",                           # differs by task type (§1.3)
    "workspace_path":             task_context.workspace_path,

    # Spec metadata
    "frontmatter_template_info":  "component_id: ...\n...",       # YAML frontmatter block

    # Guidance blocks (from component templates)
    "unified_downlift_guidance":  rendered_downlift_guidance,      # only for downlift
    "tool_descriptions":          rendered_tool_descriptions,
    "semlift_format_guidance":    rendered_format_guidance,

    # Sample specs (few-shot examples)
    "sample_specs_content":       loaded_examples,

    # Feature flags
    "use_d4_format":              bool,
    "compression_level":          "full" | "reduced" | "minimal" | "ultra-minimal",
    "use_suffix_component_id":    bool,
    "task_context":               task_context,
}
```

### 1.3 Workspace Context by Task Type

The `workspace_context_info` field varies based on task type:

| Task Type | Content |
|-----------|---------|
| **Uplift** | Source files to analyze, target spec output path |
| **Downlift (full)** | Spec to implement, target source file paths |
| **Downlift (incremental)** | Git diff of spec changes, files to update surgically |
| **Analysis** | Workspace structure and discovery targets |

### 1.4 Compression Levels

The PromptManager supports four compression levels that control how much example content is included:

| Level | Behavior |
|-------|----------|
| `full` | All sample specs included as few-shot examples |
| `reduced` | Subset of sample specs |
| `minimal` | Minimal examples, abbreviated guidance |
| `ultra-minimal` | No sample specs, compressed guidance (uses `ultra_minimal_guidance.j2`) |

### 1.5 Task-Specific Message Injection

For downlift tasks using suffix-optimized component IDs, the PromptManager appends a third message after `[system, user]`:

```python
def _create_task_specific_message(task_context):
    """Inject component ID, output path, and task type into a trailing user message."""
    return Message.user(f"""
        Component ID: {component_id}
        Output Path: {output_path}
        Task Type: {task_type}
        REMEMBER: Call create_spec(spec_id="{component_id}", content="...", file_path="{output_path}")
    """)
```

This ensures the agent always has the exact IDs and paths it needs, regardless of dialog length.

---

## 2. System Prompts

System prompts establish the agent's identity, capabilities, and constraints. All system prompts include `system_context.j2` as a shared base, then add agent-type-specific instructions.

### 2.1 Base Context (`system_context.j2`)

The foundation shared by all agent types:

```
You are a SemLift AI agent — an expert software engineer specialized in code analysis
and specification generation using the SemLift framework.

WORKSPACE ISOLATION:
- You operate in an isolated git worktree for your specific task
- Write directly to target locations without worrying about conflicts
- Your workspace is isolated from other concurrent tasks

COMMIT INTEGRATION WORKFLOW:
1. You perform file operations (create_spec, write_file, apply_patch, etc.)
2. All changes accumulate in your isolated workspace
3. When you call complete_task with success=True, changes are committed
4. The commit message includes your task ID and description
5. Changes are applied to the main repository atomically
6. Your workspace is cleaned up automatically

WORKSPACE BEHAVIOR NOTES:
- Failed tasks (complete_task with success=False) discard workspace changes
- Only successful tasks contribute changes to the main repository
- You can freely experiment — only final success commits are applied
- File conflicts are handled automatically by the workspace system

CRITICAL TOOL USAGE RULES:
- ALWAYS provide ALL required parameters when calling tools
- For create_spec: MUST include both spec_id AND content parameters
- NEVER call create_spec with only spec_id parameter
- Check tool descriptions for required parameters before calling
- If you get parameter errors, READ the error and fix it
- Generate complete specification content BEFORE calling create_spec
- Do not retry the same incomplete tool call multiple times
```

### 2.2 Constrained System (`constrained_system.j2`)

Extends `system_context.j2` with structured task execution rules:

**Key additions:**

- **Spec hierarchy terminology**:
  - *Leaf Spec* — file-level specification for a single source file
  - *Aggregate Spec* — module/directory-level specification summarizing children
  - *Root Spec* — top-level project specification

- **Multi-agent delegation model**: "You are ONE agent in a HIERARCHICAL multi-agent system. Each agent handles ONE specification."

- **Task scope rules**:
  - `source_files` is empty → spec-only generation (call `create_spec`)
  - `source_files` is populated → code generation (call `write_file`)

- **Common mistakes to avoid**: Missing parameters, wrong tool usage, inventing file paths

- **Spec ID addressing**: Golden Standard hierarchical format, YAML frontmatter field guidance (auto-populated vs agent-populated)

- **Validation requirements**: Must call `validate_spec()` before completion

### 2.3 ReAct System (`react_system.j2`)

Extends `system_context.j2` with iterative reasoning:

**Key additions:**

- **Think → Act → Observe** loop pattern
- **Task & spec graph awareness**: Agent understands its position in the dependency DAG
- **Edit failure recovery strategy**:
  ```
  After 2+ failed edit attempts on the same file:
  1. STOP trying edits
  2. Read the ENTIRE file with read_file
  3. Rewrite the file completely using write_file
  ```

### 2.4 Chat System (`chat_system.j2`)

Extends `system_context.j2` with conversational principles:

**Key additions:**

- Role: "debugging chat assistant"
- Principles:
  1. Confirm the user's goal before acting
  2. Present a short plan, then execute
  3. Use tools deliberately (read before write)
  4. Close every turn with `respond_to_user`

### 2.5 Spec-Centric System (`spec_centric_system.j2`)

Alternative system prompt for spec-graph-first workflows:

**Key additions:**

- **Spec graph architecture**: Dependency edges vs hierarchy edges
- **Golden Standard validation workflow**: Validate every spec before completion
- **Spec statuses**: `pending` → `in_progress` → `completed` | `blocked` | `draft`

---

## 3. User Prompts

User prompts inject the task-specific context, instructions, and completion requirements.

### 3.1 Constrained User (`constrained_user.j2`)

The primary user prompt for uplift and downlift tasks:

```
TASK TYPE: {{ task_type }}
TASK DESCRIPTION: {{ task_description }}

{{ unified_downlift_guidance }}      ← Full downlift/uplift guidance block

TASK ID: {{ task_id_info }}
SOURCE FILES: {{ source_path_info }}

WORKSPACE CONTEXT:
{{ workspace_context_info }}

SPECIFICATION METADATA:
{{ frontmatter_template_info }}

AVAILABLE TOOLS:
{{ tool_descriptions }}

FORMAT GUIDANCE:
{{ semlift_format_guidance }}

ESSENTIAL TOOLS:
- create_spec(spec_id, content)      — Create/update specifications
- write_file(path, content)          — Write source files
- read_file(path)                    — Read files
- get_spec(spec_id)                  — Read specifications
- validate_spec(spec_id)             — Validate spec format
- complete_task(task_id, success)    — REQUIRED: Signal completion

COMPLETION REQUIREMENTS:
- Task ID for complete_task: {{ task_id }}
- MUST call complete_task(task_id='{{ task_id }}', success=True) when done
- Tasks without complete_task calls are marked FAILED

PRE-FLIGHT CHECKLIST:
☐ Read relevant source files / specs
☐ Generate complete content before tool calls
☐ Validate all specs
☐ Call complete_task with correct task_id
```

### 3.2 ReAct User (`react_user.j2`)

Similar structure to constrained, with additions:

- **Iterative reasoning approach**: Explicit Think → Act → Observe → Continue instructions
- **Parameter validation checklist**: Before each tool call, state tool name + required params + values
- **Error recovery protocol**: After failed tool calls, stop → read error → fix → single retry

### 3.3 Chat User (`chat_user.j2`)

For interactive dialog sessions:

```
MISSION: {{ mission }}

CONVERSATION HISTORY:
{{ conversation_history }}

WORKSPACE TOOLS:
{{ tool_list }}

CURRENT REQUEST:
{{ current_request }}

EXECUTION CHECKLIST:
☐ Confirm understanding of request
☐ Use tools to gather context
☐ Provide actionable response
☐ Call respond_to_user to deliver answer
```

### 3.4 Spec-Centric Variants

Two spec-centric user prompts for spec-graph-first workflows:

- **`spec_centric_chat_user.j2`**: Steps through explore → analyze → create/update → process → track
- **`spec_centric_react_user.j2`**: ReAct pattern starting with "thought about existing specs"

---

## 4. Component Templates

Reusable guidance blocks included by system and user prompts via Jinja2 `{% include %}`.

### 4.1 Template Composition

The `semlift_format_guidance.j2` template acts as a router, composing the right guidance blocks based on task type:

```jinja2
{% if task_type in ["uplift", "downlift"] %}
  {% if use_d4_format %}
    {% include 'components/d4_format_guidance.j2' %}
  {% else %}
    {% include 'components/standard_format_guidance.j2' %}
  {% endif %}

  {% if task_type == 'uplift' %}
    {% include 'components/uplift_guidance.j2' %}
  {% endif %}

  {% if task_type == 'downlift' %}
    {% if task_context.incremental_update %}
      {% include 'components/incremental_downlift_guidance.j2' %}
    {% elif compression_level == 'ultra-minimal' %}
      {# Skip verbose guidance #}
    {% else %}
      {% include 'components/downlift_guidance.j2' %}
    {% endif %}
  {% endif %}

  {{ sample_specs_content }}
{% endif %}
```

### 4.2 Uplift Guidance (`uplift_guidance.j2`, ~850 lines)

Comprehensive instructions for code → spec transformation. Key sections:

**Universal uplift strategy:**
1. Scan for documentation (`.md`, `docs/`, `README`) first
2. Connect code structures with design rationale
3. Extract architectural context from whatever exists
4. Elevate from "what does this do?" to "why was this designed this way?"

**Dual-layer documentation:**
- *Strategic layer*: Architectural rationale, design decisions, project vision
- *Technical layer*: Interfaces, error behavior, algorithms, complexity, edge cases

**Leaf spec requirements** — must capture complete function/symbol signatures:
- Full method signatures with exact parameter names, types, defaults
- Complete return types and return value semantics
- All public API surface (classes, functions, constants, exceptions)
- Constructor signatures with all parameters
- Error contracts (which exceptions, under what conditions)
- Edge case behaviors and state mutations

**Aggregate spec requirements** — focus on cross-cutting concerns:
- Design patterns used across files
- Shared abstractions and protocols
- Cross-module symbols and contracts
- Data flow between components
- Error handling strategy, concurrency, security boundaries

**Architectural rationale** (mandatory for every spec):
1. Design patterns with PURPOSE and constraints
2. Data flow and lifecycle documentation
3. Core design decisions with trade-offs
4. Atypical/surprising design choices ("looks like X but actually Y")
5. Error handling philosophy

**Domain-specific guidance** — specialized sections for:
- Database/storage: durability, consistency, file handle management
- Network: connection pooling, retry logic, backpressure
- Security: timing attacks, cryptographic guarantees
- Concurrency: thread safety, race conditions, deadlock prevention

**Success criteria checklist** — every spec must include:
```markdown
## Implementation Requirements

### Success Criteria Checklist

#### Public API Surface (MANDATORY)
- [ ] `function_name(param1: Type1, param2: Type2 = default) -> ReturnType`
[... every exported symbol]

#### Required Behavior (NON-NEGOTIABLE)
- [ ] MUST maintain [specific state] throughout [lifecycle]
- [ ] MUST call [specific function] [when]
[... every critical requirement with MUST language]

#### Performance Requirements
- [ ] [Pattern/optimization] for [reason]
```

### 4.3 Downlift Guidance (`downlift_guidance.j2`, ~1180 lines)

Comprehensive instructions for spec → code transformation. Key sections:

**API contract requirements:**
- Every function/method signature from spec must be implemented exactly
- All error contracts honored (exceptions, return values)
- Edge case behaviors must match specification
- State mutation contracts preserved
- All public API surface must exist

**Handling existing implementations (blending strategy):**
1. Read existing file → classify as headers-only / partial / full / stale
2. Read spec + parent spec + dependency specs
3. Blend strategy: **preserve what works, fix what doesn't**
   - Keep existing structure when spec-compliant
   - Extend stubs with implementations
   - Fix incomplete implementations surgically
   - Align signatures/contracts with spec
   - Apply parent/aggregate architectural patterns

**Testing workflow (iterative correction):**
```
Generate implementation with inline tests
  → Run tests (pytest)
  → If failures: investigate → fix IMPLEMENTATION (not tests!) → re-test
  → Repeat until all pass
  → Remove test functions
  → Complete task
```

Key testing principles:
- Fix underlying implementation, not test expectations
- Preserve existing test suites (treat as code assets)
- Use `/tmp` for ephemeral validation tests
- Focused agent responsibility — only fix YOUR component's failures

**Aggregate spec processing:**
- `child_spec_ids` must be populated for subtask spawning
- Empty `child_spec_ids` = no subtasks created
- System spawns a downlift task per child spec ID

**Success criteria verification:**
Before `complete_task()`, verify every item in the spec's checklist:
- All public symbols implemented with correct signatures
- All behavioral contracts honored
- Domain patterns followed exactly (ordering, durability, etc.)
- Implementation tests passed

### 4.4 Incremental Downlift Guidance (`incremental_downlift_guidance.j2`, ~84 lines)

For targeted updates when a spec changes (rather than full regeneration):

**Steps:**
1. **Analyze changes**: `get_spec_git_diff(component_id)` — understand what changed
2. **Update own content**: Resolve inconsistencies in the current spec
3. **Update children** (aggregation specs only): Propagate changes to child specs
4. **Update source code**: Incrementally patch implementation files
5. **Validate and complete**

**Core principles:**
- Make only necessary changes — preserve existing functionality
- No formatting/styling/cleanup unless explicitly asked
- Cannot add/remove child specs unless clearly required
- Efficiency: only call `get_spec_git_diff` on current spec, not children

### 4.5 Format Guidance

**D4 Format (`d4_format_guidance.j2`):**
- Two-section structure: Free-form Description + Structured Design Doc
- Both wrapped in `<details>` / `<summary>` tags
- Structured section required for file specs, empty for aggregation specs
- Structured section must capture complete API documentation: classes, methods, constructors, attributes, constants, imports

**Standard Format (`standard_format_guidance.j2`):**
- Single natural-language body with sections: Overview, Behavior, Dependencies, Domain Context, Implementation Requirements
- Domain Context section required for specialized components (database, networking, security, concurrency)
- Success Criteria Checklist section required for downlift verification

### 4.6 Tool Descriptions (`tool_descriptions.j2`, ~105 lines)

Renders available MCP tools with:
- Specification creation mission statement
- Workspace-aware execution context
- Parameter validation requirements (Claude Sonnet 4-specific behavioral mitigations)
- Loop prevention guidance
- Unified patch approach for text editing
- Per-tool schema: name, description, parameters (required/optional), types

### 4.7 Spec Type Examples (`spec_type_examples.j2`)

Quick reference for common spec structures:

```yaml
# Root Spec (project-level)
component_id: root
is_root: true
type: module
child_spec_ids: ['src_aggregate', 'tests_aggregate', 'docs_aggregate']

# Aggregate Spec (directory-level)
component_id: src/models_aggregate
type: aggregation
child_spec_ids: ['src/models/user', 'src/models/product', 'src/models/order']

# File Spec (implementation-level)
component_id: src/models/user
type: module
source_files: ['src/models/user.py']
parent_spec_id: src/models_aggregate
```

---

## 5. Planning Templates

Planning templates drive the **top-down decomposition** workflow, where a high-level user request is broken into an architectural plan, then into component specs, then into file-level specs.

### 5.1 Architectural Planner (`architectural_planner.j2`)

Generates a markdown architectural plan from a user request:

**Role:** Expert Software Architect with domain-specific knowledge

**Instructions:**
1. Analyze the domain (simulation, web app, data processing, game, etc.)
2. Apply domain knowledge to identify patterns, algorithms, approaches
3. Decompose into specialized components with domain terminology
4. Define domain-specific responsibilities per component
5. Choose file names reflecting the domain and technical approach
6. Identify technical dependencies

**Output format:**
```markdown
# Architectural Plan: {{ project_name }}

## 1. Core Requirements
### Primary Goal
### Critical Requirements

## 2. High-Level Architecture
```mermaid
[component relationship diagram]
```

## 3. Global Data Models
```{{ language }}
[shared data structures]
```

## 4. Component Breakdown
### Component: [Domain-Specific Name]
Responsibility: [domain terminology]
#### Public API / Interface
#### Files
```

### 5.2 D4 Architectural Planner (`d4_architectural_planner.j2`)

Generates a D4-format root spec from a user request:

**Key constraints:**
- **Single-component default**: Default to simplest architecture unless complexity requires more
- **Complexity justification mandate**: Must justify >1 component
- **Never empty child_spec_ids**: At least one child component required
- Creates a root spec with `is_root: true` and `d4_aggregation_task: true`

**Example output (single-component):**
```yaml
---
title: Minimal Calculator App
component_id: calculator
type: module
language: python
is_root: true
d4_aggregation_task: true
child_spec_ids:
  - calculator_app
dependencies: []
---
# Design Doc for Minimal Calculator App
[free-form architectural narrative]
```

### 5.3 D4 Component Spec Generator (`d4_component_spec_generator.j2`)

Generates a D4-format component spec from a parent architectural plan:

**Key constraints:**
- **Minimal file set principle**: Absolute minimum source files required
- **Single-file default**: Don't split into multiple files unless justified
- `d4_aggregation_task: true` — component specs can spawn leaf tasks
- Must determine source files by analyzing component responsibilities
- Receives parent architectural plan as context

**Inputs:** `component_id`, `component_name`, `parent_spec_id`, `language`, `parent_content`

### 5.4 D4 Leaf Spec Generator (`d4_leaf_spec_generator.j2`)

Generates a D4-format file-level spec from a parent component spec:

**Key constraints:**
- `d4_aggregation_task: false` — leaf specs don't spawn subtasks
- If D4 format enabled: includes both Free-form Description and Structured Design Doc
- If D4 format disabled: free-form content only
- Both sections wrapped in `<details>` / `<summary>` tags

**Structured section** (when D4 enabled) includes:
- Classes with constructors, methods, attributes
- Functions with signatures, parameters, return types, exceptions
- Constants with types and values
- Import dependencies (external and internal)

**Inputs:** `file_path`, `file_id`, `parent_component_id`, `language`, `use_d4_format`, `parent_content`

### 5.5 Ultra-Minimal Guidance (`ultra_minimal_guidance.j2`)

Compressed single-template guidance for the `ultra-minimal` compression level. Combines spec format, tool usage, code generation blending, and validation into a dense reference. Used when token budget is tight.

**Key sections:**
- Role & Goal (2 lines)
- Critical Requirements (tools, format, validation, workspace, source_files check)
- Specification Formats (D4 and free-form templates)
- Required YAML fields
- Code Generation workflow (read → blend → verify)
- API Contract Compliance
- Spec Hierarchy Usage
- Critical Warnings (parameter validation, loop prevention, completion required)

---

## 6. Dialog Summarization

When agent dialogs exceed the token budget, the LSM compactor (AGENTS.md §11) uses an LLM to summarize older messages. The summarization prompt is:

### 6.1 System Prompt

```
You are an expert dialog summarizer for AI agent conversations. Your role is to create
concise summaries that preserve essential context for task continuation.

SUMMARY MESSAGE FORMAT:
- Always start with "Summary of [topic/activity]:" to clearly identify summary messages
- Focus on: (1) What the agent accomplished/read, (2) Key findings and unique details,
  (3) What remains to be done
- Preserve tool call patterns and important state information
- Maintain continuity with the overall task objectives

Your summaries will be inserted into the dialog history to manage context length while
preserving critical information for the agent's continued work.
```

### 6.2 User Prompt

```
You need to summarize the following dialog following a Log Structured Merge Tree pattern.
You need to preserve relevant and unique details at each summary level to accomplish the
objectives laid out in the system message and the first user message.

The dialog is as follows:
{dialog_up_to_point}

Please summarize the following messages to be added to the dialog.
Remember to:
1) Outline what the agent did/read
2) Relevant and unique details — e.g. sections of the specification that should be
   written based on what was observed
3) What remains to be done to complete the overall task goals.

{messages_text}

IMPORTANT: Start your response with "Summary of [topic/activity]:" to clearly identify
this as a summary message. The summary must fit within approximately {target_tokens} tokens.
```

### 6.3 Compaction Mechanics

The `Summarizer` class implements the `DialogCompactor` interface (AGENTS.md §11.1):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `enable_summarization` | `false` | Master switch |
| `max_tokens_per_level` | 100,000 | Token budget per LSM level |
| `max_levels` | 2 | Compactable levels (+ 1 preserved) |
| `preserve_prefix_count` | 2 | Messages in non-compacting level |
| `token_estimate_divisor` | 4 | chars ÷ 4 ≈ tokens |
| `target_summary_tokens` | 10,000 | Max tokens per summary |

**Compaction trigger:** After each `append()`, if Level 0's estimated tokens exceed `max_tokens_per_level`, forward greedy selection runs — picking messages from index 0, keeping tool-call/result pairs atomic. The selected span is summarized via the LLM and promoted to the next level.

**Dialog reconstruction:** `get_dialog()` concatenates levels from oldest (preserved) to newest:
```
[preserved level N] + [level N-1 summaries] + ... + [level 0 active messages]
```

---

## 7. Sample Specifications

The PromptManager loads `.semlift.md` files from `examples/sample_specs/` as few-shot exemplars. These demonstrate the expected spec format and quality level.

### 7.1 Available Examples

| Sample | Type | Domain | Complexity |
|--------|------|--------|-----------|
| `isbn_validator.semlift.md` | function | Validation | low |
| `database_storage.semlift.md` | module | Database persistence | high |
| `network_http_client.semlift.md` | module | Networking | high |
| `security_crypto.semlift.md` | module | Security/crypto | high |
| `authentication_service.semlift.md` | module | Auth | medium |
| `user_profile_service.semlift.md` | module | User management | medium |
| `payment_processor.semlift.md` | module | Payments | medium |
| `api_gateway.semlift.md` | module | API routing | medium |
| `library_core.semlift.md` | module | Library system | medium |
| `borrowing_service.semlift.md` | module | Library borrowing | medium |
| `test_fixtures.semlift.md` | module | Testing | low |
| `data_processing_aggregate.semlift.md` | aggregation | Data pipeline | medium |
| `d4_table.semlift.md` | file (D4) | Database table | high |

### 7.2 Example: Leaf Spec (ISBN Validator)

```yaml
---
component_id: isbn_validator
type: function
source_files:
  - "src/library_management/utils/validation.py"
dependencies: []
version: "1.0.0"
language: python
complexity: low
---

# isbn_validator

## Description
Utility function that validates ISBN-10 and ISBN-13 codes according to international
standards. Performs checksum verification and format validation.

## Rationale
ISBN validation is critical for data integrity in library systems. Centralizing this
logic ensures consistent checking across all parts of the application.

## Natural Language Specification
Accepts an ISBN string and validates against ISBN-10 and ISBN-13 standards. Handles
hyphens, spaces, and different separators. Performs format validation and checksum
verification. Returns boolean validity and optionally normalizes the ISBN.

## Interface

### Public Functions
- `validate_isbn(isbn: str, normalize: bool = False) -> bool | tuple[bool, str]`
- `normalize_isbn(isbn: str) -> str`
- `calculate_isbn_checksum(isbn_digits: str, isbn_type: str) -> str`

### Utility Functions
- `_clean_isbn(isbn: str) -> str`
- `_is_valid_isbn10(isbn: str) -> bool`
- `_is_valid_isbn13(isbn: str) -> bool`

## Implementation Hints

### Algorithms
- ISBN-10 checksum: Modulo 11 algorithm with weights 10-2
- ISBN-13 checksum: Modulo 10 algorithm with alternating weights 1 and 3
- Format normalization using regex patterns

### Design Patterns
- Pure function design with no side effects
- Strategy pattern for different ISBN type validation
- Input validation with early returns

## Constraints
- Must support both ISBN-10 and ISBN-13 formats
- Input can contain hyphens, spaces, or no separators
- Must handle 'X' in ISBN-10 check digits
- Function must be pure (no side effects)
```

### 7.3 Example: Domain-Specific Spec (Database Storage, excerpt)

```yaml
---
component_id: database/storage
type: module
source_files:
  - src/database/storage.py
dependencies:
  - database/serializer
language: python
complexity: high
---

# Database Storage Layer

## Overview
Provides persistent storage interface for document database with ACID durability
guarantees. Implements abstract `Storage` base class with concrete `JSONStorage`
(file-based) and `MemoryStorage` (in-memory) implementations.

## Domain Context: Database File Persistence

### Critical Database Requirements

#### 1. Data Durability (WHY fsync matters)
**Domain Requirement**: Database writes must survive crashes and power loss
**Risk Without**: Data sits in OS page cache, never reaches disk
**Consequence**: Crash/power loss = recent writes lost, database corrupted
**Solution**: Call `os.fsync(handle.fileno())` after every write operation

**Failure Scenario**:
  Time 0: App writes {"users": {"1": {"name": "Alice"}}}
  Time 1: write() returns successfully (data in OS cache)
  Time 2: Power loss occurs
  Time 3: System restarts, file still contains old data
  Result: Database corrupted, Alice's record never persisted

#### 2. Concurrent Access Safety (WHY persistent handles matter)
**Domain Requirement**: Multiple processes may access database simultaneously
**Risk Without**: Race conditions with repeated open/close operations
**Solution**: Maintain single persistent file handle throughout instance lifetime
```

---

## 8. Prompt Assembly Flow

The complete assembly flow from task creation to agent dialog:

```
┌──────────────────────────────────────────────────────────────────┐
│  Job Scheduler assigns task to worker                            │
│  → TaskContext created with spec_context, workspace_path, etc.   │
└──────────────────────┬───────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│  PromptManager.create_prompt(agent_type, task_context)           │
│                                                                  │
│  1. _build_template_context(task_context)                        │
│     → Assembles rendering context (§1.2)                         │
│     → Loads sample specs at compression level                    │
│                                                                  │
│  2. Render system template (§2)                                  │
│     system_context.j2 + {constrained|react|chat}_system.j2      │
│                                                                  │
│  3. Render user template (§3)                                    │
│     {constrained|react|chat}_user.j2                             │
│     → Includes component templates via semlift_format_guidance   │
│     → Injects format guidance, tool descriptions, samples        │
│                                                                  │
│  4. Optional: _create_task_specific_message() (§1.5)             │
│     → Component ID, output path for suffix-optimized mode        │
│                                                                  │
│  Returns: [system_msg, user_msg, ?task_msg]                      │
└──────────────────────┬───────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│  Dialog Prepopulation (AGENTS.md §10)                            │
│                                                                  │
│  Phase 0: read_spec(component_id)         — source of truth      │
│  Phase 1: list_specs() + ls               — workspace overview    │
│  Phase 2: read_file(path) per src_file    — source content       │
│  Phase 3: read_spec(dep) per dependency   — dependency contracts │
│  Phase 4: read_spec(child) per child      — child specs          │
│                                                                  │
│  Each phase appends synthetic tool-call/result pairs             │
│  Compactor may trigger during prepopulation                      │
└──────────────────────┬───────────────────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────────────────┐
│  ReAct Loop begins                                               │
│  Agent sees: system + user + prepopulated context                │
│  Agent reasons, calls tools, observes, iterates                  │
│  Compactor manages token growth (§6)                             │
│  Terminates via transition_task_state or respond_to_user         │
└──────────────────────────────────────────────────────────────────┘
```

---

## 9. Cross-Reference Map

| Topic | Location |
|-------|----------|
| System prompt architecture | AGENTS.md §5 |
| Dialog prepopulation | AGENTS.md §10 |
| Dialog compaction (LSM) | AGENTS.md §11 |
| TaskContext & SpecContext | AGENTS.md §9 |
| Tool set definitions | AGENTS.md §6 |
| Task lifecycle & transitions | AGENTS.md §4 |
| Codebase analysis (task graph) | AGENTS.md §3 |
| Worker infrastructure | WORKER_INFRASTRUCTURE.md §10 |
| Session persistence | WORKER_INFRASTRUCTURE.md §10.4 |

---

## 10. Design Rationale & Lessons

### 10.1 Why Jinja2 Templates

Templates separate prompt logic from prompt content. This enables:
- **A/B testing**: Swap guidance blocks without changing orchestration code
- **Compression levels**: Same template, different content density
- **Composability**: Shared blocks (`system_context.j2`) avoid duplication
- **Readability**: Prompt authors edit markdown-like `.j2` files, not Python strings

### 10.2 Why Explicit Failure Mitigations

Several templates include model-specific workarounds (e.g., "CLAUDE SONNET 4 SPECIFIC: MANDATORY TOOL PARAMETER VALIDATION"). These address observed failure modes:
- **Missing parameters**: Models skip required `content` param on `create_spec`
- **Retry loops**: Models repeat the same failing call without reading errors
- **Premature completion**: Models call `complete_task` before verifying work

The mitigations are explicit in-prompt instructions because they're cheaper and more reliable than post-hoc retry logic.

### 10.3 Why Success Criteria Checklists

Downlift agents frequently "simplify" implementations — omitting fsync calls, changing return types, dropping edge cases. The checklist pattern forces the agent to verify each requirement against its generated code before signaling completion. This dramatically improves first-pass correctness for complex specifications.

### 10.4 Why Domain Context Sections

Generic "implement this interface" instructions produce code that works in happy-path tests but fails in production (missing durability guarantees, timing vulnerabilities, race conditions). Domain context sections explain *why* patterns exist and *what breaks* without them, making the agent less likely to "optimize away" critical behavior.
