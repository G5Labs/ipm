import { describe, expect, it, vi } from "vitest";
import { MutationCache, QueryClient } from "@tanstack/react-query";
import { ApiError, WORKSPACE_READ_ONLY_CODE } from "./api";
import { emitReadOnlyNotice, subscribeReadOnlyNotice } from "./readOnlyNotice";

describe("readOnlyNotice bus", () => {
  it("delivers emits to every active subscriber and stops after unsubscribe", () => {
    const a = vi.fn();
    const b = vi.fn();
    const unsubA = subscribeReadOnlyNotice(a);
    const unsubB = subscribeReadOnlyNotice(b);

    emitReadOnlyNotice();
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);

    unsubA();
    emitReadOnlyNotice();
    expect(a).toHaveBeenCalledTimes(1); // no longer notified
    expect(b).toHaveBeenCalledTimes(2);

    unsubB();
  });

  it("a MutationCache wired like main.tsx re-emits ONLY for read-only rejections", async () => {
    const listener = vi.fn();
    const unsub = subscribeReadOnlyNotice(listener);

    // Mirror the production wiring (main.tsx): inspect, never swallow.
    const client = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
      mutationCache: new MutationCache({
        onError: (error) => {
          if (error instanceof ApiError && error.isReadOnly) {
            emitReadOnlyNotice();
          }
        },
      }),
    });

    // A non-read-only rejection must NOT emit.
    await client
      .getMutationCache()
      .build(client, {
        mutationFn: async () => {
          throw new ApiError({ code: "FORBIDDEN", message: "no", status: 403 });
        },
      })
      .execute(undefined)
      .catch(() => {});
    expect(listener).not.toHaveBeenCalled();

    // A WORKSPACE_READ_ONLY rejection emits exactly once.
    await client
      .getMutationCache()
      .build(client, {
        mutationFn: async () => {
          throw new ApiError({
            code: WORKSPACE_READ_ONLY_CODE,
            message: "read only",
            status: 403,
          });
        },
      })
      .execute(undefined)
      .catch(() => {});
    expect(listener).toHaveBeenCalledTimes(1);

    unsub();
  });
});
