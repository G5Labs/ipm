/**
 * nativeAttachments — which image sources are this deployment's G5
 * attachment addresses, the client-side upload checks and inserting
 * Markdown on its own line, at the end of a box whose caret was not placed.
 */

import { describe, expect, it } from "vitest";

import {
  MAX_NATIVE_ATTACHMENT_BYTES,
  imageFileRefusal,
  insertAtCaret,
  insertOnOwnLine,
  parseNativeAttachmentUrl,
} from "./nativeAttachments";

const ORIGINS = ["http://localhost:3000", "https://g5.example"];

describe("parseNativeAttachmentUrl", () => {
  it("reads absolute addresses at the deployment and paths", () => {
    expect(
      parseNativeAttachmentUrl(
        "https://g5.example/acme/issues/g5/12/attachments/7",
        ORIGINS,
      ),
    ).toEqual({ orgSlug: "acme", number: 12, attachmentId: "7" });
    expect(
      parseNativeAttachmentUrl("/acme/issues/g5/3/attachments/41?x=1", ORIGINS),
    ).toEqual({ orgSlug: "acme", number: 3, attachmentId: "41" });
  });

  it("leaves other images alone", () => {
    for (const source of [
      "https://elsewhere.example/acme/issues/g5/12/attachments/7",
      "//g5.example/acme/issues/g5/12/attachments/7",
      "https://g5.example/acme/issues/g5/12",
      "/acme/issues/github/11/12/attachments/7",
      "/acme/issues/g5/12/attachments/7/extra",
      "diagram.png",
      "pending-image:abc",
    ]) {
      expect(parseNativeAttachmentUrl(source, ORIGINS)).toBeNull();
    }
  });
});

describe("imageFileRefusal", () => {
  it("accepts the four image types up to 10 MiB", () => {
    const png = new File([new Uint8Array([1])], "a.png", { type: "image/png" });
    expect(imageFileRefusal(png)).toBeNull();
    const big = new File([new Uint8Array([1])], "big.webp", {
      type: "image/webp",
    });
    Object.defineProperty(big, "size", {
      value: MAX_NATIVE_ATTACHMENT_BYTES + 1,
    });
    expect(imageFileRefusal(big)).toBe("big.webp is larger than 10 MiB.");
  });

  it("refuses other files, including SVG", () => {
    const svg = new File(["<svg/>"], "logo.svg", { type: "image/svg+xml" });
    expect(imageFileRefusal(svg)).toBe(
      "logo.svg is not a PNG, JPEG, GIF or WebP image.",
    );
  });
});

describe("insertOnOwnLine", () => {
  it("puts the snippet on its own line at the caret", () => {
    expect(insertOnOwnLine("Look: here", 5, 5, "![a](u)")).toEqual({
      text: "Look:\n![a](u)\n here",
      caret: 14,
    });
    expect(insertOnOwnLine("", 0, 0, "![a](u)")).toEqual({
      text: "![a](u)\n",
      caret: 8,
    });
    expect(insertOnOwnLine("one\ntwo", 4, 4, "![a](u)").text).toBe(
      "one\n![a](u)\ntwo",
    );
  });
});

describe("insertAtCaret", () => {
  function box(value: string, caret: number) {
    const textarea = document.createElement("textarea");
    textarea.value = value;
    textarea.setSelectionRange(caret, caret);
    return textarea;
  }

  it("uses the caret the person placed", () => {
    const changes: string[] = [];
    const text = insertAtCaret(box("Top", 0), "Top", "![a](u)", (next) =>
      changes.push(next),
    );
    expect(text).toBe("![a](u)\nTop");
    expect(changes).toEqual([text]);
  });

  it("goes at the end of a box the person has not been in", () => {
    expect(
      insertAtCaret(box("Top", 0), "Top", "![a](u)", () => {}, false),
    ).toBe("Top\n![a](u)\n");
  });
});
