// =============================================================================
// G5 Client — Platform Bug Reporting
// =============================================================================
// POST hook for reporting a bug about the G5 platform itself. Sends the report
// as multipart/form-data (a `report` JSON part + an optional `screenshot` PNG
// part) to the platform intake route, which delivers it to a G5-owned GitHub
// repository server-side. The client never talks to GitHub directly.

import { useMutation } from "@tanstack/react-query";
import { api } from "../lib/api";
import { routes } from "../lib/routes";
import type {
  MutationHook,
  PlatformIssueReportInput,
  PlatformIssueReportResult,
} from "../lib/types";
import { toMutationHook } from "./hookUtils";

export function usePlatformIssueReport(): MutationHook<
  PlatformIssueReportInput,
  PlatformIssueReportResult
> {
  const mutation = useMutation<
    PlatformIssueReportResult,
    Error,
    PlatformIssueReportInput
  >({
    mutationFn: (input) => {
      const formData = new FormData();
      const report = {
        title: input.title,
        body: input.body,
        kind: input.kind,
        severity: input.severity,
        context: input.context,
      };
      formData.append("report", JSON.stringify(report));
      if (input.screenshot) {
        formData.append("screenshot", input.screenshot, "screenshot.png");
      }
      return api.post<PlatformIssueReportResult>(
        routes.platformIssues.create,
        formData,
      );
    },
  });
  return toMutationHook(mutation);
}
