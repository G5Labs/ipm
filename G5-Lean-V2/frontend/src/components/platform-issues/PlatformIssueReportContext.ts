// =============================================================================
// Platform report controller context.
// =============================================================================
// Kept apart from the drawer so shell code that runs before any page (auth
// gates, loading screens) can reach the controller without pulling in the
// reporter's data hooks (ui g5.quality.platform-reports.trigger).

import { createContext } from "react";

export interface PlatformIssueReportController {
  enabled: boolean;
  openReporter: () => Promise<void>;
  /** A chrome (`topbar`) trigger announces itself; returns the unregister. */
  registerChromeTrigger: () => () => void;
}

export const PlatformIssueReportContext =
  createContext<PlatformIssueReportController | null>(null);
