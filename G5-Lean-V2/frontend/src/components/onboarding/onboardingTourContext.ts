// =============================================================================
// G5 Client — Onboarding tour store (ui g5.ui.onboarding-tour)
// =============================================================================
// What pages read from the tour controller: the scratchpad composer seed
// while the issue step shows, and the replay control Settings offers. A
// module store rather than a context, so the controller sits beside the
// routes instead of wrapping them.
// =============================================================================

import { useSyncExternalStore } from "react";

export interface OnboardingTourValue {
  /** Seed for the personal scratchpad composer while the issue step shows. */
  composerSeed: { token: number; text: string } | null;
  /** Start the tour over from the welcome card (Settings → Appearance → Tour). */
  replay: () => void;
}

const IDLE: OnboardingTourValue = { composerSeed: null, replay: () => {} };

let current: OnboardingTourValue = IDLE;
const listeners = new Set<() => void>();

/** The controller publishes its value; `null` returns the store to idle. */
export function publishOnboardingTour(value: OnboardingTourValue | null): void {
  current = value ?? IDLE;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function snapshot(): OnboardingTourValue {
  return current;
}

export function useOnboardingTour(): OnboardingTourValue {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}
