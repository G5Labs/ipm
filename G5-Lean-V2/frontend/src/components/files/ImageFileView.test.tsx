import React from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const downloads = vi.hoisted(() => ({ calls: [] as [Blob, string][] }));
vi.mock("../../lib/utils", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/utils")>()),
  downloadBlob: (blob: Blob, name: string) =>
    downloads.calls.push([blob, name]),
}));

import { ImageFileView } from "./ImageFileView";
import { MAX_IMAGE_PIXELS } from "./imageZoom";

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
function pngHeader(width: number, height: number): Uint8Array {
  const be = (value: number) => [
    (value >>> 24) & 0xff,
    (value >> 16) & 0xff,
    (value >> 8) & 0xff,
    value & 0xff,
  ];
  return new Uint8Array([
    ...PNG_SIGNATURE,
    ...be(13),
    ...[0x49, 0x48, 0x44, 0x52],
    ...be(width),
    ...be(height),
    8,
    6,
    0,
    0,
    0,
  ]);
}

function decoded(image: HTMLElement, width: number, height: number) {
  Object.defineProperty(image, "naturalWidth", { value: width });
  Object.defineProperty(image, "naturalHeight", { value: height });
  fireEvent.load(image);
}

/** A ResizeObserver the test fires once the stage has a size. */
class FakeResizeObserver {
  static latest: FakeResizeObserver | null = null;
  constructor(private callback: () => void) {
    FakeResizeObserver.latest = this;
  }
  observe() {}
  unobserve() {}
  disconnect() {}
  fire() {
    this.callback();
  }
}
const realResizeObserver = window.ResizeObserver;

function view(
  overrides: Partial<React.ComponentProps<typeof ImageFileView>> = {},
) {
  const bytes = overrides.bytes ?? pngHeader(64, 48);
  return render(
    <ImageFileView
      filePath="a/photo.png"
      blob={new Blob([bytes], { type: "image/png" })}
      bytes={bytes}
      byteSize={6200}
      {...overrides}
    />,
  );
}

beforeEach(() => {
  downloads.calls.length = 0;
  vi.mocked(URL.createObjectURL).mockClear();
  vi.mocked(URL.revokeObjectURL).mockClear();
});
afterEach(() => {
  window.ResizeObserver = realResizeObserver;
});

// `g5.documents.viewers.images`
describe("ImageFileView", () => {
  it("shows the image over a typed object URL, opens at Fit and steps the zoom", () => {
    view();
    const image = screen.getByTestId("image-view-image");
    expect(image.getAttribute("src")).toBe("blob:mock-url");
    expect(image).toHaveAttribute("data-status", "loading");
    expect(image).toHaveClass("g5-checkerboard");
    const created = vi.mocked(URL.createObjectURL).mock.calls[0][0] as Blob;
    expect(created.type).toBe("image/png");
    // The header's declared size shows before the decode finishes.
    expect(screen.getByTestId("image-view-dimensions")).toHaveTextContent(
      "64 × 48 · 6.1 KB",
    );
    expect(screen.getByTestId("image-view-zoom-in")).toBeDisabled();

    decoded(image, 64, 48);
    expect(image).toHaveAttribute("data-status", "loaded");
    // Without a measured panel, Fit shows the image at its own size.
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "100%",
    );
    expect(image.style.width).toBe("64px");
    expect(image.style.height).toBe("48px");

    fireEvent.click(screen.getByTestId("image-view-zoom-in"));
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "125%",
    );
    expect(image.style.width).toBe("80px");
    fireEvent.click(screen.getByTestId("image-view-zoom-in"));
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "156%",
    );
    fireEvent.click(screen.getByTestId("image-view-actual"));
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "100%",
    );
    fireEvent.click(screen.getByTestId("image-view-zoom-out"));
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "80%",
    );
    expect(image.style.width).toBe("51px");
    fireEvent.click(screen.getByTestId("image-view-fit"));
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "100%",
    );

    fireEvent.click(screen.getByTestId("image-view-download"));
    expect(downloads.calls).toEqual([[expect.any(Blob), "photo.png"]]);
    expect(downloads.calls[0][0].type).toBe("image/png");
  });

  it("fits a larger image into the measured panel without enlarging a smaller one", () => {
    window.ResizeObserver =
      FakeResizeObserver as unknown as typeof ResizeObserver;
    view({ bytes: pngHeader(640, 480) });
    const stage = screen.getByTestId("image-view-stage");
    Object.defineProperty(stage, "clientWidth", { value: 232 });
    Object.defineProperty(stage, "clientHeight", { value: 182 });
    act(() => FakeResizeObserver.latest?.fire());
    const image = screen.getByTestId("image-view-image");
    decoded(image, 640, 480);
    // The panel is 200 × 150 inside its padding: a quarter of the image.
    expect(screen.getByTestId("image-view-zoom-level")).toHaveTextContent(
      "31%",
    );
    expect(image.style.width).toBe("200px");
    expect(image.style.height).toBe("150px");
    fireEvent.click(screen.getByTestId("image-view-actual"));
    expect(image.style.width).toBe("640px");
  });

  it("refuses an image whose header declares more than the pixel bound, without decoding", () => {
    view({ bytes: pngHeader(30000, 30000) });
    expect(screen.queryByTestId("image-view-image")).not.toBeInTheDocument();
    const notice = screen.getByTestId("image-view-notice");
    expect(notice).toHaveAttribute("data-reason", "too-large");
    expect(notice).toHaveTextContent("30000 × 30000");
    expect(notice).toHaveTextContent(
      `${Math.round(MAX_IMAGE_PIXELS / 1_000_000)} megapixels`,
    );
    fireEvent.click(screen.getByTestId("image-view-notice-download"));
    expect(downloads.calls).toEqual([[expect.any(Blob), "photo.png"]]);
  });

  it("keeps Download when the browser cannot decode the image", () => {
    const garbage = new Uint8Array([...PNG_SIGNATURE, 1, 2, 3]);
    view({ bytes: garbage, filePath: "misnamed.png" });
    fireEvent.error(screen.getByTestId("image-view-image"));
    const notice = screen.getByTestId("image-view-notice");
    expect(notice).toHaveAttribute("data-reason", "undecodable");
    expect(notice).toHaveTextContent("misnamed.png");
    expect(screen.queryByTestId("image-view-image")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("image-view-notice-download"));
    expect(downloads.calls[0][1]).toBe("misnamed.png");
  });

  it("starts over for a new blob and revokes the old URL", () => {
    let serial = 0;
    vi.mocked(URL.createObjectURL).mockImplementation(() => `blob:${++serial}`);
    const first = view();
    const image = screen.getByTestId("image-view-image");
    decoded(image, 64, 48);
    expect(image).toHaveAttribute("data-status", "loaded");
    const next = pngHeader(32, 32);
    first.rerender(
      <ImageFileView
        filePath="a/photo.png"
        blob={new Blob([next], { type: "image/png" })}
        bytes={next}
        byteSize={1}
      />,
    );
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:1");
    const replaced = screen.getByTestId("image-view-image");
    expect(replaced.getAttribute("src")).toBe("blob:2");
    expect(replaced).toHaveAttribute("data-status", "loading");
    expect(screen.getByTestId("image-view-dimensions")).toHaveTextContent(
      "32 × 32",
    );
    first.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:2");
  });

  it("hides the file name when embedded under the editor's own row", () => {
    view({ embedded: true, bytes: null });
    expect(screen.getByTestId("image-view-file-row")).not.toHaveTextContent(
      "photo.png",
    );
    expect(screen.getByTestId("image-view-dimensions")).toHaveTextContent(
      "6.1 KB",
    );
  });
});
