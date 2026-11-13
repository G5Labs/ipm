// =============================================================================
// Composer store — the page's one owner of what is typed into composers
// =============================================================================

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
import { useRenderedPrincipal } from "./useAuth";

/** What a composer's state is about. */
export interface ComposerSubject {
  kind: "thread" | "suggestion" | "draft" | "card-edit";
  id: string;
}

type Listener = () => void;

/**
 * One owner, per page, of every composer's state: the words, a send in
 * flight and its error of a thread's or a suggestion's reply box, a card's
 * rejection reason, edited replacement and decision, each file's
 * new-thread draft, and a chat changes card's statement edit. Composers read and write it, so removing or rebuilding
 * one (a Source/Rich switch, a tab switch, regrouping, deletion, a commit)
 * loses nothing, and a send completes into it even if its composer is gone.
 * A subject with an active field (words, a send, a decision) is kept in view
 * by the views (`g5.documents.comments.failure`).
 */
export interface ComposerStore {
  has(key: string): boolean;
  get(key: string): unknown;
  /** Sets `key`; `remove` drops it (it is back to its initial value). */
  set(
    key: string,
    value: unknown,
    change: { subject: ComposerSubject; active: boolean; remove: boolean },
  ): void;
  /** Drops every field of `subject`. */
  clear(subject: ComposerSubject): void;
  subscribe(key: string, listener: Listener): () => void;
  /** Ids of the subjects of `kind` with an active field. */
  activeIds(kind: ComposerSubject["kind"]): ReadonlySet<string>;
  subscribeActive(listener: Listener): () => void;
  /**
   * The record a view last listed for an active subject (a thread, a
   * suggestion), so any view can show it again once it left the list, after
   * a tab switch too. Kept only while the subject is active.
   */
  setRecord(subject: ComposerSubject, record: unknown): void;
  record(subject: ComposerSubject): unknown;
  recordsVersion(): number;
  subscribeRecords(listener: Listener): () => void;
}

const EMPTY_IDS: ReadonlySet<string> = new Set();

function subjectKey(subject: ComposerSubject): string {
  return `${subject.kind}/${subject.id}`;
}

function fieldKey(subject: ComposerSubject, field: string): string {
  return `${subjectKey(subject)}/${field}`;
}

export function createComposerStore(): ComposerStore {
  const values = new Map<string, unknown>();
  const listeners = new Map<string, Set<Listener>>();
  const activeListeners = new Set<Listener>();
  // Active field keys by subject key.
  const activeFields = new Map<string, Set<string>>();
  const subjects = new Map<string, ComposerSubject>();
  const fieldsBySubject = new Map<string, Set<string>>();
  let activeByKind = new Map<ComposerSubject["kind"], ReadonlySet<string>>();
  const records = new Map<string, unknown>();
  const recordListeners = new Set<Listener>();
  let recordsVersion = 0;
  const dropRecord = (owner: string) => {
    if (!records.delete(owner)) return;
    recordsVersion += 1;
    for (const listener of recordListeners) listener();
  };

  const notify = (key: string) => {
    for (const listener of listeners.get(key) ?? []) listener();
  };
  const recomputeActive = () => {
    const next = new Map<ComposerSubject["kind"], Set<string>>();
    for (const [key, fields] of activeFields) {
      const subject = subjects.get(key);
      if (!subject || fields.size === 0) continue;
      const ids = next.get(subject.kind) ?? new Set<string>();
      ids.add(subject.id);
      next.set(subject.kind, ids);
    }
    activeByKind = next;
    for (const listener of activeListeners) listener();
  };
  const markActive = (
    subject: ComposerSubject,
    key: string,
    active: boolean,
  ) => {
    const owner = subjectKey(subject);
    const fields = activeFields.get(owner) ?? new Set<string>();
    const wasActive = fields.size > 0;
    if (active) fields.add(key);
    else fields.delete(key);
    if (fields.size > 0) activeFields.set(owner, fields);
    else activeFields.delete(owner);
    subjects.set(owner, subject);
    if (wasActive !== fields.size > 0) {
      // Settled: the record shown for it goes with it.
      if (fields.size === 0) dropRecord(owner);
      recomputeActive();
    }
  };

  return {
    has: (key) => values.has(key),
    get: (key) => values.get(key),
    set(key, value, { subject, active, remove }) {
      const owner = subjectKey(subject);
      const fields = fieldsBySubject.get(owner) ?? new Set<string>();
      if (remove) {
        values.delete(key);
        fields.delete(key);
      } else {
        values.set(key, value);
        fields.add(key);
      }
      if (fields.size > 0) fieldsBySubject.set(owner, fields);
      else fieldsBySubject.delete(owner);
      markActive(subject, key, active && !remove);
      notify(key);
    },
    clear(subject) {
      const owner = subjectKey(subject);
      const fields = [...(fieldsBySubject.get(owner) ?? [])];
      fieldsBySubject.delete(owner);
      for (const key of fields) values.delete(key);
      const wasActive = (activeFields.get(owner)?.size ?? 0) > 0;
      activeFields.delete(owner);
      dropRecord(owner);
      if (wasActive) recomputeActive();
      for (const key of fields) notify(key);
    },
    subscribe(key, listener) {
      const set = listeners.get(key) ?? new Set<Listener>();
      set.add(listener);
      listeners.set(key, set);
      return () => {
        set.delete(listener);
        if (set.size === 0) listeners.delete(key);
      };
    },
    activeIds: (kind) => activeByKind.get(kind) ?? EMPTY_IDS,
    subscribeActive(listener) {
      activeListeners.add(listener);
      return () => activeListeners.delete(listener);
    },
    setRecord(subject, record) {
      const owner = subjectKey(subject);
      // Only an active subject is kept; one that settled meanwhile is not.
      if ((activeFields.get(owner)?.size ?? 0) === 0) return;
      // The same record listed again, or an equal one from another list,
      // changes nothing: views would only be told again.
      const current = records.get(owner);
      if (
        current === record ||
        (current !== undefined &&
          JSON.stringify(current) === JSON.stringify(record))
      ) {
        return;
      }
      records.set(owner, record);
      recordsVersion += 1;
      for (const listener of recordListeners) listener();
    },
    record: (subject) => records.get(subjectKey(subject)),
    recordsVersion: () => recordsVersion,
    subscribeRecords(listener) {
      recordListeners.add(listener);
      return () => recordListeners.delete(listener);
    },
  };
}

export const ComposerStoreContext = createContext<ComposerStore | null>(null);

/** The page's owner, or a store of this component's own without one. */
function useStore(): ComposerStore {
  const page = useContext(ComposerStoreContext);
  const own = useScopedComposerStore("");
  return page ?? own;
}

/**
 * A store for one scope (a workspace) and the principal it is rendered for:
 * another scope, person or org starts empty. An org switch keeps the page
 * (`RequireAuth` replaces it only for another person) but not what was typed
 * for the previous org (`g5.security.login.principal`).
 */
export function useScopedComposerStore(scope: string): ComposerStore {
  const principal = useRenderedPrincipal();
  const key = `${principal ?? ""}/${scope}`;
  const [owned, setOwned] = useState(() => ({
    key,
    store: createComposerStore(),
  }));
  if (owned.key !== key) {
    const next = { key, store: createComposerStore() };
    setOwned(next);
    return next.store;
  }
  return owned.store;
}

/**
 * One field of a composer's state, held by the page's owner: like
 * `useState`, but it outlives the component. `isActive` (a module-level
 * function) says when the field keeps its subject in view.
 */
export function useComposerField<T>(
  subject: ComposerSubject,
  field: string,
  initial: T,
  isActive?: (value: T) => boolean,
  /** An exact-proposal continuation shares draft words, never decision state. */
  carryId?: string,
): [T, (next: T | ((current: T) => T)) => void] {
  const store = useStore();
  const { kind, id } = subject;
  const key = fieldKey(subject, field);
  const carryKey = carryId ? fieldKey({ kind, id: carryId }, field) : key;
  const subscribe = useCallback(
    (listener: Listener) => {
      const own = store.subscribe(key, listener);
      const carried =
        carryKey !== key ? store.subscribe(carryKey, listener) : undefined;
      return () => {
        own();
        carried?.();
      };
    },
    [store, key, carryKey],
  );
  const value = useSyncExternalStore(subscribe, () =>
    store.has(key)
      ? (store.get(key) as T)
      : store.has(carryKey)
        ? (store.get(carryKey) as T)
        : initial,
  );
  useEffect(() => {
    if (!carryId || store.has(key) || !store.has(carryKey)) return;
    store.set(key, value, {
      subject: { kind, id },
      active: isActive ? isActive(value) : false,
      remove: Object.is(value, initial) && carryKey === key,
    });
  }, [store, key, carryKey, carryId, kind, id, value, initial, isActive]);
  const setValue = useCallback(
    (next: T | ((current: T) => T)) => {
      const current = store.has(key)
        ? (store.get(key) as T)
        : store.has(carryKey)
          ? (store.get(carryKey) as T)
          : initial;
      const value =
        typeof next === "function"
          ? (next as (current: T) => T)(current)
          : next;
      const carried = store.has(carryKey)
        ? (store.get(carryKey) as T)
        : initial;
      const carriedValue =
        carryKey === key
          ? value
          : typeof next === "function"
            ? (next as (current: T) => T)(carried)
            : next;
      store.set(key, value, {
        subject: { kind, id },
        active: isActive ? isActive(value) : false,
        // A cleared display must not fall through to a newer carried draft.
        remove: Object.is(value, initial) && carryKey === key,
      });
      if (carryId && carryKey !== key)
        store.set(carryKey, carriedValue, {
          subject: { kind, id: carryId },
          active: false,
          remove: Object.is(carriedValue, initial),
        });
    },
    [store, key, carryKey, carryId, kind, id, initial, isActive],
  );
  return [value, setValue];
}

/**
 * A field read and written without re-rendering, like a ref, held by the
 * page's owner so a composer built again sees it (a send in flight).
 */
export function useComposerRef<T>(
  subject: ComposerSubject,
  field: string,
  initial: T,
): { current: T } {
  const store = useStore();
  const { kind, id } = subject;
  const key = fieldKey(subject, field);
  return useMemo(
    () => ({
      get current() {
        return store.has(key) ? (store.get(key) as T) : initial;
      },
      set current(value: T) {
        store.set(key, value, {
          subject: { kind, id },
          active: false,
          remove: Object.is(value, initial),
        });
      },
    }),
    [store, key, kind, id, initial],
  );
}

/** Drops every field of a subject (a draft closed, words dismissed). */
export function useClearComposer(): (subject: ComposerSubject) => void {
  const store = useStore();
  return useCallback(
    (subject: ComposerSubject) => store.clear(subject),
    [store],
  );
}

/**
 * The records of the subjects of `kind` held at page scope. `get` reads the
 * one last listed for an active subject; it is a new function whenever a
 * record changes, so what reads it is derived again. `remember` keeps one
 * while it is listed; it stays the same function, so what writes records
 * is never run again by its own writes.
 */
export function useComposerRecords<T>(
  kind: ComposerSubject["kind"],
  /** Whether `next` may replace `held` (a module-level function). */
  accepts?: (held: T, next: T) => boolean,
): {
  get: (id: string) => T | undefined;
  remember: (id: string, record: T) => void;
} {
  const store = useStore();
  const version = useSyncExternalStore(
    store.subscribeRecords,
    store.recordsVersion,
  );
  const get = useMemo(
    // `version` makes a new reader whenever a record changes.
    () => (id: string) =>
      version >= 0 ? (store.record({ kind, id }) as T | undefined) : undefined,
    [store, kind, version],
  );
  const remember = useCallback(
    (id: string, record: T) => {
      const held = store.record({ kind, id }) as T | undefined;
      if (held !== undefined && accepts && !accepts(held, record)) return;
      store.setRecord({ kind, id }, record);
    },
    [store, kind, accepts],
  );
  return { get, remember };
}

/** The subjects of `kind` whose composers hold something now. */
export function useActiveComposerSubjects(
  kind: ComposerSubject["kind"],
): ReadonlySet<string> {
  const store = useStore();
  return useSyncExternalStore(store.subscribeActive, () =>
    store.activeIds(kind),
  );
}

/** Field predicates: words held, or a flag set. */
export const holdsWords = (value: string): boolean => value.trim().length > 0;
export const isSet = (value: boolean): boolean => value;
export const holdsValue = (value: unknown): boolean => value != null;
