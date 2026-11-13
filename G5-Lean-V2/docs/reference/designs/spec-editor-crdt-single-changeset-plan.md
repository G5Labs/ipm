# Spec Editor Incremental CRDT Rendering

## Single Change Set Plan

## 1. Goal
Ship one integrated change set that keeps rich-mode editing, preserves immediate local typing feel, and restores reliable two-peer CRDT propagation without requiring repeated follow-up refactors.

## 2. Constraints
1. No forced fallback to source mode for large documents.
2. Local keystrokes must render immediately to the typist.
3. Peer propagation must be observable and fail-fast in tests.
4. Large docs must avoid full markdown reparse on each local flush.
5. Existing Yjs document format must remain compatible.

## 3. Scope of This Single Change Set
1. Frontend incremental reconcile architecture for `SpecEditor`.
2. Frontend crash hardening for malformed awareness/user payloads.
3. Backend CRDT observability and timing instrumentation.
4. Deterministic fast-fail two-tab peer sync test and local typing benchmark updates.
5. Acceptance gates and rollback toggles included in same diff.

## 4. Architecture Decision
Keep `Y.Text` markdown as canonical shared state, but stop coupling each local Yjs write to immediate full editor reparse.

### 4.1 Runtime Model
1. `contenteditable` remains the immediate paint path.
2. Yjs writes remain immediate and minimal (char-level edits).
3. Rich editor structure (`blocks`, `offsets`) is updated incrementally for changed ranges.
4. Full document parse remains as guarded fallback, executed on idle/degraded states only.

### 4.2 Why This Works
1. Typing latency is dominated by synchronous parse/render work, not Yjs op generation.
2. Yjs delta is already granular; we can use it to bound reparsing windows.
3. Fallback full parse protects correctness for worst-case markdown transforms.

## 5. Frontend Detailed Plan

## 5.1 New Internal Module
Add `generation/client/src/components/editor/spec/BlockIndex.ts` with:
1. `buildBlockIndex(markdown): BlockIndex`.
2. `expandDirtyRangeByBlocks(index, start, end, guardBlocks): DirtyWindow`.
3. `reparseWindow(markdown, window): WindowParseResult`.
4. `spliceWindowIntoIndex(index, result): BlockIndex`.
5. `validateIndexAgainstMarkdown(index, markdown): boolean`.

The index stores:
1. `blockId` stable id.
2. `start` char offset.
3. `end` char offset.
4. `kind` block type.
5. `hash` for cheap unchanged detection.

## 5.2 SpecEditor Core Changes
File: `generation/client/src/components/editor/SpecEditor.tsx`

1. Replace whole-document local reconcile scheduling with:
1.1. `applyYjsDeltaToDirtyRange`.
1.2. `scheduleIncrementalReconcile`.
1.3. `scheduleIdleFullReconcile` fallback.

2. Maintain refs:
2.1. `blockIndexRef`.
2.2. `pendingDirtyRangeRef`.
2.3. `incrementalReconcileTimerRef`.
2.4. `idleFullReconcileTimerRef`.

3. Local edit path:
3.1. Write minimal Yjs op immediately.
3.2. Merge dirty range.
3.3. Incremental reconcile after short debounce (100-140ms).
3.4. Do not full-parse on every local flush.

4. Remote edit path:
4.1. Keep remote debounce (50ms).
4.2. Use received Yjs delta to drive dirty window parse.
4.3. Full parse only on index validation failure.

5. Fallback rules:
5.1. If dirty window exceeds threshold percentage of doc, run one full parse.
5.2. If parse splice validation fails twice in row, full parse and reset index.
5.3. If markdown parse throws, keep current view and retry with full parse on next tick.

## 5.3 EditableInline Compatibility
File: `generation/client/src/components/editor/EditableInline.tsx`

1. Preserve immediate DOM typing behavior.
2. Keep debounce flush semantics.
3. Add optional callback to provide last local changed span for better dirty-range seeding.

## 5.4 Renderer Performance Controls
File: `generation/client/src/components/editor/SpecEditor.tsx`

1. Memoize block rows by `blockId + hash`.
2. Prevent rerender for untouched blocks.
3. Add viewport windowing for very large block counts:
3.1. Render visible range + overscan.
3.2. Keep comment anchors aligned using absolute top offsets.

## 5.5 Crash Hardening (already aligned with current direction)
Files:
1. `generation/client/src/lib/crdt.ts`
2. `generation/client/src/pages/WorkspacePage.tsx`
3. `generation/client/src/components/editor/SpecEditor.tsx`
4. `generation/client/src/components/workspace/TopBar.tsx`

Ensure all awareness/user display strings are sanitized before `.charAt` or render assumptions.

## 6. Backend Detailed Plan

## 6.1 CRDT Authority Instrumentation
File: `generation/g5-api/src/crdt/authority.rs`

Add structured timing and counters:
1. WS connect handshake duration.
2. Yjs update receive size and decode time.
3. Broadcast fanout count and duration.
4. Awareness update processing duration.
5. Flush queue depth and lag metrics.

## 6.2 Flush Path Visibility
Where CRDT persistence is handled:
1. Measure enqueue-to-persist latency.
2. Emit warning on threshold breach.
3. Confirm persistence work never blocks WS event loop.

## 6.3 Fast Diagnosis Fields
Include in logs:
1. `workspace_id`
2. `branch_id`
3. `session_id`
4. `client_count`
5. `update_bytes`
6. `fanout_ms`

## 7. Performance Budgets
1. Local keydown->input p95 <= 16ms on target machine.
2. Two-tab localhost propagation p95 <= 250ms.
3. Two-tab propagation hard failure if >5s.
4. No UI lockups due to full parse during normal typing.
5. Large doc rich-mode scroll remains interactive (no multi-second stalls).

## 8. Test Plan in Same Change Set

## 8.1 Unit Tests
Add tests for:
1. Dirty range merge logic.
2. Block window expansion.
3. Window splice correctness.
4. Index validation/fallback behavior.

Target files:
1. `generation/client/src/test/spec-block-index.test.ts`
2. `generation/client/src/test/spec-incremental-reconcile.test.ts`

## 8.2 Integration/E2E
1. Keep and harden two-tab sync probe:
   `generation/client/e2e/flows/crdt-peer-sync.spec.ts`
2. Keep typing benchmark:
   `generation/client/e2e/flows/typing-latency.spec.ts`
3. Parameterize with:
   `E2E_TARGET_PATH=/g5labs/base/spec/70f42434-0bbf-41b9-95ea-d0703dc2c1c9`

Assertions:
1. Editable surface found in both tabs.
2. Marker typed in tab A appears in tab B under 5s.
3. Fail fast with useful debug payload on timeout.

## 8.3 Backend Validation
1. Existing CRDT tests pass.
2. Add/update awareness/disconnect instrumentation assertions where practical.
3. Validate no regression in handshake and awareness snapshot logic.

## 9. Feature Flags and Safety
Add frontend flag:
1. `VITE_SPEC_INCREMENTAL_RECONCILE=true|false`

Behavior:
1. `true` uses new incremental pipeline.
2. `false` reverts to current full-reconcile behavior.
3. Flag checked at runtime to enable immediate rollback without code rollback.

## 10. Delivery Steps (Single Diff, Ordered)
1. Add block index module and tests.
2. Refactor `SpecEditor` to incremental reconcile with fallback full parse.
3. Add render memoization and windowing.
4. Add awareness/user crash-hardening final pass.
5. Add backend CRDT instrumentation.
6. Update e2e peer sync and typing benchmarks.
7. Run full validation matrix and attach benchmark/probe artifacts.

## 11. Validation Matrix Before Merge
1. `npx tsc --noEmit` (client).
2. Relevant frontend unit tests.
3. `cargo check -p g5-api` (or equivalent API package check).
4. CRDT-focused Rust tests.
5. `playwright` peer sync fast-fail probe on target spec path.
6. `playwright` local typing benchmark on same path.

## 12. Acceptance Criteria
1. Rich mode remains available for large docs.
2. Local typing visibly updates immediately during continuous typing.
3. No `charAt` crash from collaborator/awareness rendering.
4. Two-tab peer sync probe passes within SLA on local stack.
5. Debug metrics/logs identify stage of delay if SLA fails.

## 13. Risks and Mitigations
1. Risk: index drift on complex markdown edits.
   Mitigation: strict validation and deterministic full-parse fallback.
2. Risk: virtualization breaks comment alignment.
   Mitigation: preserve absolute offsets and add alignment tests.
3. Risk: instrumentation overhead.
   Mitigation: lightweight counters, sampled structured logs for high-volume events.

## 14. Out of Scope for This Change Set
1. Server-side markdown AST storage redesign.
2. Protocol-level Yjs document schema changes.
3. Cross-region deployment tuning.

## 15. Expected Outcome
One merge delivers:
1. Native-feel local typing in rich mode.
2. Reliable peer sync on local stack.
3. Actionable telemetry when peer sync degrades.
4. A rollback switch if incremental reconcile misbehaves.
