// Instance-feature gates for optional sections of the one workbench shell
// (ui g5.ui.workbench-shell). The shell itself is never gated: these read the
// cached instance-info `features` list for sections an instance may switch
// off, failing closed on missing, unloaded or malformed payloads (a brief
// absence while instance info loads is acceptable; a render throw is not).

/** The `guided_journey` instance feature (ui g5.ui.guided-toggle). */
export const GUIDED_JOURNEY_FEATURE = "guided_journey";

export function hasInstanceFeature(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
  feature: string,
): boolean {
  const features = instanceInfo?.features;
  // Array.isArray guards malformed/legacy instance-info payloads (the API
  // client performs no runtime validation) so the gate fails closed instead
  // of throwing during render.
  return Array.isArray(features) && features.includes(feature);
}

/**
 * Resolve the Guided-section gate from instance-info features. The server
 * advertises it unless its kill-switch is set, so an instance without a
 * feature list still reads as off here.
 */
export function isGuidedJourneyEnabled(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
): boolean {
  return hasInstanceFeature(instanceInfo, GUIDED_JOURNEY_FEATURE);
}
