import { describe, expect, it } from "vitest";
import {
  fileExtension,
  fileViewerKind,
  imageMimeForPath,
  isHtmlPath,
  isMarkdownPath,
  isPdfPath,
  isSvgPath,
  MAX_EDITABLE_TEXT_BYTES,
} from "./fileText";

// `g5.documents.viewers`: the extension alone chooses a viewer.
describe("fileViewerKind", () => {
  const text = { content: "hello", encoding: "utf-8" as const, size: 5 };
  const binary = { content: "AAEC", encoding: "base64" as const, size: 3 };

  it("reads extensions case-insensitively and only from the file name", () => {
    expect(fileExtension("a/b/Photo.JPG")).toBe("jpg");
    expect(fileExtension("a.b/noext")).toBe("");
    expect(fileExtension(".hidden")).toBe("");
    expect(isMarkdownPath("docs/README.MD")).toBe(true);
    expect(isHtmlPath("docs.xhtml/plain.txt")).toBe(false);
    expect(isHtmlPath("docs/page.html.txt")).toBe(false);
    expect(isHtmlPath(".html")).toBe(false);
  });

  it.each(["html", "HTM", "XhTmL", "shtml"])(
    "recognizes %s previews without bypassing text admission",
    (extension) => {
      const path = `docs/page.${extension}`;
      expect(isHtmlPath(path)).toBe(true);
      expect(
        fileViewerKind(path, { ...text, size: MAX_EDITABLE_TEXT_BYTES }),
      ).toBe("text");
      expect(
        fileViewerKind(path, { ...text, size: MAX_EDITABLE_TEXT_BYTES + 1 }),
      ).toBe("too-large");
      expect(fileViewerKind(path, binary)).toBe("binary");
      expect(fileViewerKind(path, { ...text, content: "a\u0000b" })).toBe(
        "binary",
      );
    },
  );

  it("maps raster extensions to the types the browser decodes", () => {
    expect(imageMimeForPath("x.png")).toBe("image/png");
    expect(imageMimeForPath("x.apng")).toBe("image/apng");
    expect(imageMimeForPath("x.jpeg")).toBe("image/jpeg");
    expect(imageMimeForPath("x.gif")).toBe("image/gif");
    expect(imageMimeForPath("x.webp")).toBe("image/webp");
    expect(imageMimeForPath("x.avif")).toBe("image/avif");
    expect(imageMimeForPath("x.bmp")).toBe("image/bmp");
    expect(imageMimeForPath("x.ico")).toBe("image/x-icon");
    expect(imageMimeForPath("x.svg")).toBe("image/svg+xml");
    // Outside the initial set: HEIC, TIFF, PSD, RAW, JPEG XL.
    for (const extension of [
      "heic",
      "heif",
      "tif",
      "tiff",
      "psd",
      "cr2",
      "jxl",
    ])
      expect(imageMimeForPath(`x.${extension}`)).toBeNull();
    // Inherited object keys are not extensions.
    expect(imageMimeForPath("x.constructor")).toBeNull();
    expect(imageMimeForPath("x.toString")).toBeNull();
    expect(imageMimeForPath("x.__proto__")).toBeNull();
  });

  it("sends PDFs and raster images to their viewers whatever the bytes", () => {
    expect(isPdfPath("report.PDF")).toBe(true);
    expect(fileViewerKind("report.pdf", binary)).toBe("pdf");
    expect(fileViewerKind("report.pdf", text)).toBe("pdf");
    expect(fileViewerKind("photo.png", binary)).toBe("image");
    // A text file misnamed as an image reaches the image viewer, which
    // fails to decode it and offers Download.
    expect(fileViewerKind("photo.png", text)).toBe("image");
  });

  it("keeps a text SVG in the editor and sends a binary one to the viewer", () => {
    expect(isSvgPath("logo.svg")).toBe(true);
    expect(fileViewerKind("logo.svg", text)).toBe("text");
    expect(fileViewerKind("logo.svg", binary)).toBe("image");
    expect(fileViewerKind("logo.svg", { ...text, content: "a\u0000b" })).toBe(
      "image",
    );
  });

  it("leaves every other file to text eligibility", () => {
    expect(fileViewerKind("main.rs", text)).toBe("text");
    expect(fileViewerKind("archive.zip", binary)).toBe("binary");
    expect(fileViewerKind("photo.heic", binary)).toBe("binary");
    expect(
      fileViewerKind("big.txt", { ...text, size: MAX_EDITABLE_TEXT_BYTES + 1 }),
    ).toBe("too-large");
  });
});
