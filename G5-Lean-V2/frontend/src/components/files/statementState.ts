// =============================================================================
// statementState — fold and verification-panel choices in one ontology Preview
// =============================================================================
// `g5.documents.ontology.disclosure`: the choices live outside React state so
// that opening a fold or a statement's verification details re-renders only
// that statement, never the whole document. Each statement subscribes to its
// own version.
// =============================================================================

/** A reader's fold choice, made while `reasons` were opening the fold. */
export interface FoldChoice {
  open: boolean;
  reasons: string;
}

export function createStatementState() {
  let folds = new Map<string, FoldChoice>();
  let panels = new Set<string>();
  const versions = new Map<string, number>();
  const listeners = new Set<() => void>();
  const changed = (ids: Iterable<string>) => {
    for (const id of ids) versions.set(id, (versions.get(id) ?? 0) + 1);
    listeners.forEach((listener) => listener());
  };
  return {
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /** Changes whenever the statement's fold or panel choice changes. */
    version: (id: string) => versions.get(id) ?? 0,
    fold: (id: string) => folds.get(id),
    setFold(id: string, choice: FoldChoice) {
      folds = new Map(folds).set(id, choice);
      changed([id]);
    },
    /** Replaces every fold choice, as Expand all and Collapse all do. */
    setFolds(next: ReadonlyMap<string, FoldChoice>) {
      const ids = new Set([...folds.keys(), ...next.keys()]);
      folds = new Map(next);
      changed(ids);
    },
    panelOpen: (id: string) => panels.has(id),
    togglePanel(id: string) {
      panels = new Set(panels);
      if (!panels.delete(id)) panels.add(id);
      changed([id]);
    },
  };
}

export type StatementState = ReturnType<typeof createStatementState>;
