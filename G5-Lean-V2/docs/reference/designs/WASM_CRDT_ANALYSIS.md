# Rust-in-Frontend WASM Analysis: Shared CRDT Implementation

## Problem Statement

The G5 codebase maintains **two parallel CRDT implementations** that must stay in sync:

| Layer | Library | Key Code |
|-------|---------|----------|
| Backend (Rust) | `yrs` (Rust port of Yjs) | `g5-common/src/crdt/doc.rs` — `CrdtDoc` struct |
| Frontend (TypeScript) | `yjs` (JavaScript) | `client/src/lib/crdt.ts` — `markFileDirty()`, `useCrdtDirtyCount()` |

Both implementations share the same Yjs binary protocol (v1 encoding), so they interoperate at the wire level. However, they duplicate application-level logic:

- **`markFileDirty()`** — sets `_meta.dirty_files[path] = "true"` on a shared Y.Map
- **`get_dirty_files()` / `clear_dirty_files()`** — reads/clears the dirty files map
- **`get_text()` / `set_text()`** — keyed Y.Text access by file path
- **State encoding** — v1 encode/decode, state vector, diff generation

Any behavioral change to the CRDT conventions (e.g., renaming `_meta.dirty_files` to `_meta.modified`) requires synchronized updates in both codebases.

## Option: Compile Rust CRDT Module to WASM

Using `wasm-pack` + `wasm-bindgen`, the `CrdtDoc` struct from `g5-common` could be compiled to a WASM module and used directly in the browser, eliminating the TypeScript CRDT code.

### What Would Move to WASM

| Function | Current (TS) | WASM replacement |
|----------|--------------|------------------|
| `markFileDirty(yDoc, path)` | `crdt.ts` | `CrdtDoc::mark_file_dirty(path)` |
| `useCrdtDirtyCount(yDoc)` | `crdt.ts` hook | Calls `CrdtDoc::get_dirty_files().len()` |
| Document creation & encoding | `new Y.Doc()` | `CrdtDoc::new()` / `CrdtDoc::from_state()` |
| State sync encoding | `Y.encodeStateAsUpdate()` | `CrdtDoc::encode_state_as_update()` |
| Update application | `Y.applyUpdate()` | `CrdtDoc::apply_update()` |
| File content read/write | `yDoc.getText(path)` | `CrdtDoc::get_text()` / `set_text()` |

### What Would Remain in TypeScript

- **React hooks** — `useDraftCRDT`, `useCrdtDirtyCount` (the React reactivity layer)
- **WebSocket transport** — `y-websocket` or a custom transport calling WASM encode/decode
- **Editor bindings** — `y-prosemirror`, `y-monaco` (these need a JS `Y.Doc` reference)
- **Awareness protocol** — cursor/presence data (JS-side concern)

### Challenges

1. **Editor binding incompatibility** — `y-prosemirror` and `y-monaco` expect a JavaScript `Y.Doc` instance. A WASM `CrdtDoc` backed by `yrs` cannot be passed directly to these plugins. The editor bindings would need either:
   - A compatibility shim that mirrors WASM state into a JS `Y.Doc` (defeats the purpose)
   - Rewritten editor bindings that speak to WASM directly (massive effort)

2. **y-websocket dependency** — The `y-websocket` provider manages the sync protocol, buffering, and reconnection. It binds to a JS `Y.Doc`. Replacing it would require re-implementing the sync protocol on top of raw WebSocket + WASM encode/decode.

3. **Bundle size** — A `wasm-pack` build of `g5-common` (which pulls in `yrs`, `serde`, `thiserror`) would add ~200-400KB gzipped to the frontend bundle. The JS `yjs` package is ~25KB gzipped.

4. **Memory model mismatch** — `yrs::Doc` is not `Send+Sync`. In WASM's single-threaded environment this is fine, but bridging between WASM memory and JS objects requires explicit serialization at the boundary, adding overhead to every edit operation.

5. **Development velocity** — Hot-reload works for TypeScript changes instantly. WASM requires a compile step (`wasm-pack build`), adding friction to the frontend development loop.

## Assessment

| Criterion | Score | Notes |
|-----------|-------|-------|
| Dual-implementation elimination | Partial | Only application logic (`mark_file_dirty`, etc.) would be shared. Editor bindings and transport remain in JS. |
| Bundle size impact | Negative | ~10-15× larger than current JS yjs bundle |
| Editor integration | Blocking | y-prosemirror / y-monaco require JS `Y.Doc` |
| Development experience | Negative | Adds WASM compile step to frontend dev loop |
| Sync protocol safety | Neutral | Both yrs and yjs use identical v1 wire format; interop is already proven |
| Maintenance cost reduction | Low | The shared surface area (`mark_file_dirty`, `get_dirty_files`) is small (~50 lines of TS) |

## Recommendation

**Do not pursue WASM embedding at this time.** The primary blocker is editor binding incompatibility — `y-prosemirror` and `y-monaco` fundamentally require a JavaScript `Y.Doc`, and no practical shim exists to bridge WASM `yrs::Doc` to these plugins without duplicating state.

The actual shared surface area (dirty-file tracking conventions) is small enough to manage through:

1. **Contract tests** — The new `crdt-commit.test.tsx` tests validate that `markFileDirty()` and `useCrdtDirtyCount()` use the same `_meta.dirty_files` Y.Map convention as the Rust `CrdtDoc::mark_file_dirty()`.
2. **Integration tests** — The Rust `crdt_sync.rs` tests verify that updates encoded by `yrs` are correctly decoded by `yjs` (and vice versa) through the WebSocket sync protocol.
3. **Shared constants** — If the convention space grows, a shared JSON/YAML file defining map names and key conventions could be consumed by both codebases at build time.

### Revisit Conditions

The WASM approach becomes more attractive if:
- The shared CRDT logic surface grows significantly beyond dirty-file tracking
- A `yrs`-native editor binding for ProseMirror/Monaco emerges
- The `y-crdt` project ships a first-class WASM+JS bridge with editor plugin support
