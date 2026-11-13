import { randomUUID } from "node:crypto";

interface LifecycleResponse {
  status: number;
  body: unknown;
}

function isDatabaseRetryable(response: LifecycleResponse): boolean {
  if (response.status !== 503) return false;
  const body = response.body;
  if (!body || typeof body !== "object" || !("error" in body)) return false;
  const error = body.error;
  return (
    !!error &&
    typeof error === "object" &&
    "code" in error &&
    error.code === "DATABASE_RETRYABLE"
  );
}

/** Replay only the server's typed transaction-abort response, retaining the
 * lifecycle identity even if an earlier phase already admitted the operation.
 * Transport errors and every other refusal remain immediate failures.
 */
export async function requestWorkspaceVmAction(
  request: (operationId: string) => Promise<LifecycleResponse>,
  wait: (milliseconds: number) => Promise<void>,
): Promise<void> {
  const operationId = randomUUID();
  for (let attempt = 0; attempt < 3; attempt++) {
    const response = await request(operationId);
    if (response.status === 200 || response.status === 202) return;
    if (attempt < 2 && isDatabaseRetryable(response)) {
      await wait(250 * (attempt + 1));
      continue;
    }
    throw new Error(
      `Workspace lifecycle request failed (${response.status}): ${JSON.stringify(response.body)}`,
    );
  }
}
