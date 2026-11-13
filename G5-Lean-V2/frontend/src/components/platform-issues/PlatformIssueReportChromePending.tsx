import { useContext, useLayoutEffect } from "react";

import { PlatformIssueReportContext } from "./PlatformIssueReportContext";

/**
 * Placeholder for a chrome report trigger that has not mounted yet. A loading
 * screen that will be replaced by page chrome renders this so the floating
 * fallback does not flash until the chrome arrives; error screens omit it so
 * the fallback stays reachable when the chrome never comes
 * (ui g5.quality.platform-reports.trigger).
 */
export function PlatformIssueReportChromePending() {
  const registerChromeTrigger = useContext(
    PlatformIssueReportContext,
  )?.registerChromeTrigger;

  useLayoutEffect(() => {
    if (!registerChromeTrigger) return;
    return registerChromeTrigger();
  }, [registerChromeTrigger]);

  return null;
}
