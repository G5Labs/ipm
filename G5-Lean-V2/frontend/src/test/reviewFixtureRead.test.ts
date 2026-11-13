import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readReviewFixtureFile } from "../../e2e/review-fixture-read";

type Response = { status: number; body: unknown };
const closed: Response = {
  status: 503,
  body: {
    error: {
      code: "SERVICE_UNAVAILABLE",
      message:
        "Service unavailable: workspace runtime call failed: HOST_TUNNEL_CLOSED",
    },
  },
};
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

it("recovers a read after the observed host-tunnel closure without inventing a missing file", async () => {
  const ready: Response = {
    status: 200,
    body: { data: { hash: "original-hash" } },
  };
  const read = vi
    .fn<() => Promise<Response>>()
    .mockResolvedValueOnce(closed)
    .mockResolvedValue(ready);
  const result = readReviewFixtureFile(read);
  await vi.advanceTimersByTimeAsync(500);
  expect(await result).toBe(ready);
  expect(read).toHaveBeenCalledTimes(2);
});

it("returns persistent closure unchanged after ten bounded read retries", async () => {
  const read = vi.fn(async () => closed);
  const result = readReviewFixtureFile(read);
  await vi.runAllTimersAsync();
  expect(await result).toBe(closed);
  expect(read).toHaveBeenCalledTimes(11);
});

it.each([
  { status: 200, body: { data: { hash: "original-hash" } } },
  { status: 404, body: { error: { code: "NOT_FOUND" } } },
  { status: 403, body: { error: { code: "FORBIDDEN" } } },
  { status: 409, body: { error: { code: "CONFLICT" } } },
  {
    status: 503,
    body: { error: { code: "OTHER", message: "HOST_TUNNEL_CLOSED" } },
  },
  {
    status: 503,
    body: {
      error: { code: "SERVICE_UNAVAILABLE", message: "different failure" },
    },
  },
  { status: 503, body: null },
])(
  "preserves an unrelated response without retry: $status / $body",
  async (response) => {
    const read = vi.fn(async () => response);
    expect(await readReviewFixtureFile(read)).toBe(response);
    expect(read).toHaveBeenCalledTimes(1);
  },
);

it("propagates a thrown transport error instead of replaying an unknown outcome", async () => {
  const error = new Error("unexpected network failure");
  const read = vi.fn().mockRejectedValue(error);
  await expect(readReviewFixtureFile(read)).rejects.toBe(error);
  expect(read).toHaveBeenCalledTimes(1);
});
