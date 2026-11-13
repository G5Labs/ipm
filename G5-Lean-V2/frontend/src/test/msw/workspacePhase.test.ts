// =============================================================================
// workspacePhase.test.ts — the MSW workspace PATCH keeps the chosen phase
// =============================================================================
// The phase mutation refetches the workspace detail after a PATCH
// (g5.workbench.phases.state). The shared handlers back both the tests and the
// `frontend-only` preview, so a phase the PATCH accepted must come back on the
// next detail read, as from the API; otherwise the preview's chevrons snap
// back to Business Engineering. The store resets after every test.
// =============================================================================

import { describe, expect, it } from "vitest";

import { resetMockWorkspacePhases } from "./handlers";

const DETAIL = "/api/v1/workspaces/task-graph-rework";

async function readPhase(): Promise<unknown> {
  const res = await fetch(DETAIL);
  const json = (await res.json()) as { data: { phase?: unknown } };
  return json.data.phase;
}

async function patchPhase(phase: unknown): Promise<unknown> {
  const res = await fetch(DETAIL, {
    method: "PATCH",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ phase }),
  });
  const json = (await res.json()) as { data: { phase?: unknown } };
  return json.data.phase;
}

describe("MSW workspace PATCH keeps the chosen phase", () => {
  it("returns an accepted phase on later reads until reset", async () => {
    const initial = await readPhase();
    expect(initial).not.toBe("ie");

    expect(await patchPhase("ie")).toBe("ie");
    expect(await readPhase()).toBe("ie");

    // An invalid phase changes nothing.
    expect(await patchPhase("nope")).toBe("ie");
    expect(await readPhase()).toBe("ie");

    resetMockWorkspacePhases();
    expect(await readPhase()).toBe(initial);
  });

  it("starts each test from the fixture's phase", async () => {
    expect(await readPhase()).not.toBe("ie");
  });
});
