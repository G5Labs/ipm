// Protocol checks for the lifecycle test driver. Real Stop/Resume effects and
// Preview fencing are qualified separately by workspace-preview.spec.ts.
import { expect, test } from "@playwright/test";
import { requestWorkspaceVmAction } from "../workspace-vm-action";

const aborted = {
  status: 503,
  body: { error: { code: "DATABASE_RETRYABLE" } },
};

for (const status of [200, 202]) {
  test(`accepts lifecycle ${status} without replay`, async () => {
    const ids: string[] = [];
    const waits: number[] = [];
    await requestWorkspaceVmAction(
      async (id) => {
        ids.push(id);
        return { status, body: { data: { status: "stopping" } } };
      },
      async (milliseconds) => {
        waits.push(milliseconds);
      },
    );
    expect(ids).toHaveLength(1);
    expect(ids[0]).toMatch(/^[0-9a-f-]{36}$/);
    expect(waits).toEqual([]);
  });
}

test("replays typed transaction aborts with the same operation ID", async () => {
  const ids: string[] = [];
  const waits: number[] = [];
  await requestWorkspaceVmAction(
    async (id) => {
      ids.push(id);
      return ids.length < 3 ? aborted : { status: 202, body: {} };
    },
    async (milliseconds) => {
      waits.push(milliseconds);
    },
  );
  expect(ids).toHaveLength(3);
  expect(new Set(ids).size).toBe(1);
  expect(waits).toEqual([250, 500]);
});

test("fails after three transaction aborts instead of retrying indefinitely", async () => {
  const ids: string[] = [];
  const waits: number[] = [];
  await expect(
    requestWorkspaceVmAction(
      async (id) => {
        ids.push(id);
        return aborted;
      },
      async (milliseconds) => {
        waits.push(milliseconds);
      },
    ),
  ).rejects.toThrow("503");
  expect(ids).toHaveLength(3);
  expect(new Set(ids).size).toBe(1);
  expect(waits).toEqual([250, 500]);
});

for (const response of [
  { status: 503, body: { error: { code: "SERVICE_UNAVAILABLE" } } },
  { status: 503, body: { error: { code: "PREVIEW_NOT_READY" } } },
  { status: 503, body: "unconfirmed response" },
  ...[403, 409, 500].map((status) => ({ status, body: aborted.body })),
]) {
  test(`does not replay ${response.status} ${JSON.stringify(response.body)}`, async () => {
    let requests = 0;
    let waits = 0;
    await expect(
      requestWorkspaceVmAction(
        async () => {
          requests++;
          return response;
        },
        async () => {
          waits++;
        },
      ),
    ).rejects.toThrow(String(response.status));
    expect(requests).toBe(1);
    expect(waits).toBe(0);
  });
}

test("does not replay an unconfirmed transport failure", async () => {
  let requests = 0;
  let waits = 0;
  await expect(
    requestWorkspaceVmAction(
      async () => {
        requests++;
        throw new Error("connection lost");
      },
      async () => {
        waits++;
      },
    ),
  ).rejects.toThrow("connection lost");
  expect(requests).toBe(1);
  expect(waits).toBe(0);
});

test("a refusal after an abort ends the same operation without another replay", async () => {
  const ids: string[] = [];
  let waits = 0;
  await expect(
    requestWorkspaceVmAction(
      async (id) => {
        ids.push(id);
        return ids.length === 1 ? aborted : { status: 403, body: {} };
      },
      async () => {
        waits++;
      },
    ),
  ).rejects.toThrow("403");
  expect(ids).toHaveLength(2);
  expect(new Set(ids).size).toBe(1);
  expect(waits).toBe(1);
});
