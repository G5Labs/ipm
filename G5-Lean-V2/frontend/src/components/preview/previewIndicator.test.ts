import { describe, expect, it } from "vitest";
import {
  previewIndicatorState,
  previewIndicatorText,
  type PreviewRunView,
} from "./previewIndicator";

const view = (over: Partial<PreviewRunView> = {}): PreviewRunView => ({
  status: "running",
  ready: false,
  closing: false,
  close: null,
  port: 3000,
  ...over,
});

describe("previewIndicatorState", () => {
  it("is off without a preview and lit by the live state", () => {
    expect(previewIndicatorState(null)).toBe("none");
    expect(previewIndicatorState(view())).toBe("starting");
    expect(previewIndicatorState(view({ ready: true }))).toBe("serving");
    expect(previewIndicatorState(view({ ready: true, closing: true }))).toBe(
      "closing",
    );
  });

  it("stays red for a failed run or a close whose cleanup failed, off after a clean close", () => {
    expect(previewIndicatorState(view({ status: "failed" }))).toBe("failed");
    expect(
      previewIndicatorState(view({ status: "cancelled", close: "killed" })),
    ).toBe("failed");
    expect(
      previewIndicatorState(view({ status: "cancelled", close: "lost" })),
    ).toBe("failed");
    expect(
      previewIndicatorText(
        "failed",
        view({ status: "cancelled", close: "lost" }),
      ),
    ).toBe("the last close was cut short by a lost VM; open for details");
    expect(previewIndicatorText("failed", view({ status: "failed" }))).toBe(
      "the last run failed; open for its log",
    );
    expect(
      previewIndicatorState(view({ status: "cancelled", close: "clean" })),
    ).toBe("none");
    expect(previewIndicatorState(view({ status: "succeeded" }))).toBe("none");
  });

  it("says the state in words with the port", () => {
    expect(previewIndicatorText("serving", view())).toBe(
      "serving on port 3000",
    );
    expect(previewIndicatorText("none", view())).toBeNull();
  });
});
