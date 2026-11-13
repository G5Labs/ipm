/** Named phase timings on the current test result (ontology
 * g5.quality.delivery.ci.timing-evidence). One `g5-timings` annotation per
 * attempt holds `{ phase: { ms, count } }`; it is updated in place and
 * Playwright reports its final state, so CI's shard summary can separate
 * fixture setup, body, cleanup and capacity waits. Evidence only: recording
 * never fails or slows a test, and outside a test it is a no-op.
 */
import { test, type TestInfo } from "@playwright/test";

export const TIMINGS_ANNOTATION = "g5-timings";

type Timings = Record<string, { ms: number; count: number }>;

const byTest = new WeakMap<
  TestInfo,
  {
    timings: Timings;
    annotation: { type: string; description: string };
    scope?: string;
  }
>();

function currentTest(): TestInfo | undefined {
  try {
    return test.info();
  } catch {
    return undefined;
  }
}

function entryFor(info: TestInfo) {
  let entry = byTest.get(info);
  if (!entry) {
    entry = {
      timings: {},
      annotation: { type: TIMINGS_ANNOTATION, description: "{}" },
    };
    info.annotations.push(entry.annotation);
    byTest.set(info, entry);
  }
  return entry;
}

/** Prefix later phases of this test with `scope/` (for example a fixture's
 * setup, body and cleanup), so shared helpers are attributed to their stage. */
export function setTimingScope(scope: string | undefined): void {
  const info = currentTest();
  if (info) entryFor(info).scope = scope;
}

export function recordTiming(phase: string, ms: number): void {
  const info = currentTest();
  if (!info || !Number.isFinite(ms) || ms < 0) return;
  const entry = entryFor(info);
  const name = entry.scope ? `${entry.scope}/${phase}` : phase;
  const current = entry.timings[name] ?? { ms: 0, count: 0 };
  entry.timings[name] = {
    ms: current.ms + Math.round(ms),
    count: current.count + 1,
  };
  entry.annotation.description = JSON.stringify(entry.timings);
}

/** Run `action` and record its wall time under `phase`, even when it throws. */
export async function timed<T>(
  phase: string,
  action: () => Promise<T>,
): Promise<T> {
  const started = Date.now();
  try {
    return await action();
  } finally {
    recordTiming(phase, Date.now() - started);
  }
}
