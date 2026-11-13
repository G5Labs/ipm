import { afterEach, describe, expect, it, vi } from "vitest";

import { formatApprovalDateTime } from "./dateTime";

describe("formatApprovalDateTime", () => {
  afterEach(() => vi.restoreAllMocks());

  it("requests the viewer-local short timezone", () => {
    const formatter = vi
      .spyOn(Date.prototype, "toLocaleString")
      .mockReturnValue("Sep 1, 2026, 08:00 AM EDT");

    expect(formatApprovalDateTime("2026-09-01T12:00:00Z")).toContain("EDT");
    expect(formatter).toHaveBeenCalledWith(
      undefined,
      expect.objectContaining({ timeZoneName: "short" }),
    );
  });
});
