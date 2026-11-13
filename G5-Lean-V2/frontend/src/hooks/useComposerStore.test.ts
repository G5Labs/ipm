// The page's owner of what is typed into composers
// (`g5.documents.comments.failure`): fields outlive the composer, a subject
// is active while one of its fields is, and another scope starts empty.

import React from "react";
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  ComposerStoreContext,
  createComposerStore,
  holdsWords,
  isSet,
  useComposerRecords,
  useComposerField,
  useScopedComposerStore,
} from "./useComposerStore";

const thread = { kind: "thread" as const, id: "t-1" };

describe("composer store", () => {
  it.each(["reply", "reason", "edited"])(
    "evaluates a carried %s completion against each field's current words",
    (field) => {
      const store = createComposerStore();
      const wrapper = ({ children }: { children: React.ReactNode }) =>
        React.createElement(
          ComposerStoreContext.Provider,
          { value: store },
          children,
        );
      const old = renderHook(
        () =>
          useComposerField<string>(
            { kind: "suggestion", id: "a:edit" },
            field,
            "",
            holdsWords,
            "edit",
          ),
        { wrapper },
      );
      act(() => old.result.current[1]("first words"));
      const newer = renderHook(
        () =>
          useComposerField<string>(
            { kind: "suggestion", id: "b:edit" },
            field,
            "",
            holdsWords,
            "edit",
          ),
        { wrapper },
      );
      expect(newer.result.current[0]).toBe("first words");
      act(() => newer.result.current[1]("later words"));
      act(() =>
        old.result.current[1]((current) =>
          current === "first words" ? "" : current,
        ),
      );
      expect(old.result.current[0]).toBe("");
      expect(newer.result.current[0]).toBe("later words");
      const continued = renderHook(
        () =>
          useComposerField<string>(
            { kind: "suggestion", id: "c:edit" },
            field,
            "",
            holdsWords,
            "edit",
          ),
        { wrapper },
      );
      expect(continued.result.current[0]).toBe("later words");
      expect(store.activeIds("suggestion").has("edit")).toBe(false);
      expect(store.has(`suggestion/a:edit/${field}`)).toBe(true);
      expect(store.get(`suggestion/a:edit/${field}`)).toBe("");
      expect(store.activeIds("suggestion").has("a:edit")).toBe(false);
      expect(
        store.record({ kind: "suggestion", id: "a:edit" }),
      ).toBeUndefined();
    },
  );

  it.each(["editing", "rejecting"])(
    "evaluates a carried %s update against each field's current flag",
    (field) => {
      const store = createComposerStore();
      const wrapper = ({ children }: { children: React.ReactNode }) =>
        React.createElement(
          ComposerStoreContext.Provider,
          { value: store },
          children,
        );
      const old = renderHook(
        () =>
          useComposerField<boolean>(
            { kind: "suggestion", id: "a:edit" },
            field,
            false,
            isSet,
            "edit",
          ),
        { wrapper },
      );
      act(() => old.result.current[1](true));
      const newer = renderHook(
        () =>
          useComposerField<boolean>(
            { kind: "suggestion", id: "b:edit" },
            field,
            false,
            isSet,
            "edit",
          ),
        { wrapper },
      );
      act(() => newer.result.current[1](false));
      act(() => old.result.current[1]((current) => !current));
      const continued = renderHook(
        () =>
          useComposerField<boolean>(
            { kind: "suggestion", id: "c:edit" },
            field,
            false,
            isSet,
            "edit",
          ),
        { wrapper },
      );
      expect(continued.result.current[0]).toBe(true);
      expect(store.activeIds("suggestion").has("edit")).toBe(false);
    },
  );

  it("keeps a field until it is back to its initial value", () => {
    const store = createComposerStore();
    const key = "thread/t-1/reply";
    store.set(key, "Yes", { subject: thread, active: true, remove: false });
    expect(store.get(key)).toBe("Yes");
    store.set(key, "", { subject: thread, active: false, remove: true });
    expect(store.has(key)).toBe(false);
  });

  it("names a subject active while any of its fields is, and tells its watchers", () => {
    const store = createComposerStore();
    const watcher = vi.fn();
    store.subscribeActive(watcher);
    store.set("thread/t-1/reply", "Yes", {
      subject: thread,
      active: holdsWords("Yes"),
      remove: false,
    });
    store.set("thread/t-1/sending", true, {
      subject: thread,
      active: isSet(true),
      remove: false,
    });
    expect([...store.activeIds("thread")]).toEqual(["t-1"]);
    store.set("thread/t-1/reply", "", {
      subject: thread,
      active: false,
      remove: true,
    });
    // Still sending: still active.
    expect(store.activeIds("thread").has("t-1")).toBe(true);
    store.set("thread/t-1/sending", false, {
      subject: thread,
      active: false,
      remove: true,
    });
    expect(store.activeIds("thread").size).toBe(0);
    // Once when it became active, once when it stopped.
    expect(watcher).toHaveBeenCalledTimes(2);
  });

  it("drops every field of a subject when it is cleared", () => {
    const store = createComposerStore();
    const listener = vi.fn();
    store.subscribe("draft/code:a/words", listener);
    store.set("draft/code:a/words", "Hi", {
      subject: { kind: "draft", id: "code:a" },
      active: false,
      remove: false,
    });
    store.clear({ kind: "draft", id: "code:a" });
    expect(store.has("draft/code:a/words")).toBe(false);
    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("keeps a subject's record only while it is active, and drops it when it settles", () => {
    const store = createComposerStore();
    const record = { id: "t-1", content: "Split this?" };
    // Not active: nothing is kept.
    store.setRecord(thread, record);
    expect(store.record(thread)).toBeUndefined();
    store.set("thread/t-1/reply", "Yes", {
      subject: thread,
      active: true,
      remove: false,
    });
    store.setRecord(thread, record);
    expect(store.record(thread)).toBe(record);
    // Settled: the record goes with the words.
    store.set("thread/t-1/reply", "", {
      subject: thread,
      active: false,
      remove: true,
    });
    expect(store.record(thread)).toBeUndefined();
  });

  it("changes nothing for an equal record from another list", () => {
    const store = createComposerStore();
    store.set("thread/t-1/reply", "Yes", {
      subject: thread,
      active: true,
      remove: false,
    });
    store.setRecord(thread, { id: "t-1", replyCount: 0 });
    const version = store.recordsVersion();
    const watcher = vi.fn();
    store.subscribeRecords(watcher);
    store.setRecord(thread, { id: "t-1", replyCount: 0 });
    expect(store.recordsVersion()).toBe(version);
    expect(watcher).not.toHaveBeenCalled();
    store.setRecord(thread, { id: "t-1", replyCount: 1 });
    expect(store.recordsVersion()).toBe(version + 1);
  });

  it("replaces a held record only with one its kind accepts", () => {
    const store = createComposerStore();
    const card = { kind: "suggestion" as const, id: "sug-1" };
    store.set("suggestion/sug-1/reply", "Yes", {
      subject: card,
      active: true,
      remove: false,
    });
    const wrapper = ({ children }: { children: React.ReactNode }) =>
      React.createElement(
        ComposerStoreContext.Provider,
        { value: store },
        children,
      );
    // A decision never gives way to an open view.
    const decidedStays = (held: { status: string }, next: { status: string }) =>
      held.status === "open" || next.status !== "open";
    const { result } = renderHook(
      () => useComposerRecords<{ status: string }>("suggestion", decidedStays),
      { wrapper },
    );
    act(() => result.current.remember("sug-1", { status: "open" }));
    act(() => result.current.remember("sug-1", { status: "rejected" }));
    act(() => result.current.remember("sug-1", { status: "open" }));
    expect(store.record(card)).toEqual({ status: "rejected" });
  });

  it("starts empty for another scope", () => {
    const { result, rerender } = renderHook(
      ({ scope }) => useScopedComposerStore(scope),
      { initialProps: { scope: "ws-1" } },
    );
    const first = result.current;
    act(() => {
      first.set("thread/t-1/reply", "Yes", {
        subject: thread,
        active: true,
        remove: false,
      });
    });
    rerender({ scope: "ws-1" });
    expect(result.current).toBe(first);
    rerender({ scope: "ws-2" });
    expect(result.current).not.toBe(first);
    expect(result.current.has("thread/t-1/reply")).toBe(false);
  });
});
