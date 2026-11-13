// =============================================================================
// G5 Editor — loadMonaco
// =============================================================================
// One lazy load of `@monaco-editor/react` with the bundled Monaco runtime
// (`g5.documents.editor.assets`), shared by every editor and the Compare
// dialog: the code is fetched once, and a view that switches from read-only
// to editable while the first load is still in flight waits for that load
// instead of starting another. A failed load is forgotten so the next view
// can try again.
// =============================================================================

type MonacoReact = typeof import("@monaco-editor/react");

let pending: Promise<MonacoReact> | null = null;

export function loadMonacoReact(): Promise<MonacoReact> {
  pending ??= Promise.all([
    import("@monaco-editor/react"),
    import("./monacoRuntime"),
  ])
    .then(([monacoReact]) => monacoReact)
    .catch((error: unknown) => {
      pending = null;
      throw error;
    });
  return pending;
}
