import React from "react";
import { render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  base64Bytes,
  fileBlob,
  fileBytes,
  formatByteSize,
  useObjectUrl,
} from "./fileBytes";

describe("file bytes", () => {
  it("decodes base64 and encodes text", () => {
    expect(Array.from(base64Bytes("AAEC/w=="))).toEqual([0, 1, 2, 255]);
    expect(Array.from(fileBytes({ content: "hé", encoding: "utf-8" }))).toEqual(
      [104, 195, 169],
    );
    expect(
      Array.from(fileBytes({ content: "AAEC", encoding: "base64" })),
    ).toEqual([0, 1, 2]);
  });

  it("types blobs by the caller's type or the read's encoding", () => {
    expect(fileBlob({ content: "AAEC", encoding: "base64" }).type).toBe(
      "application/octet-stream",
    );
    expect(
      fileBlob({ content: "AAEC", encoding: "base64" }, "image/png").type,
    ).toBe("image/png");
    expect(fileBlob({ content: "x", encoding: "utf-8" }).type).toBe(
      "text/plain;charset=utf-8",
    );
  });

  it("formats sizes", () => {
    expect(formatByteSize(512)).toBe("512 B");
    expect(formatByteSize(1536)).toBe("1.5 KB");
    expect(formatByteSize(32 * 1024 * 1024)).toBe("32.0 MB");
  });
});

// `g5.documents.viewers`: an object URL lives as long as its blob is shown.
describe("useObjectUrl", () => {
  const create = vi.mocked(URL.createObjectURL);
  const revoke = vi.mocked(URL.revokeObjectURL);
  afterEach(() => {
    create.mockClear();
    revoke.mockClear();
  });

  const Shown: React.FC<{ blob: Blob | null }> = ({ blob }) => {
    const url = useObjectUrl(blob);
    return <output data-testid="url">{url ?? "none"}</output>;
  };

  it("creates a URL per blob and revokes it when replaced or unmounted", () => {
    let serial = 0;
    create.mockImplementation(() => `blob:${++serial}`);
    const first = new Blob(["a"]);
    const second = new Blob(["b"]);
    const view = render(<Shown blob={first} />);
    expect(view.getByTestId("url").textContent).toBe("blob:1");
    expect(create).toHaveBeenCalledTimes(1);
    expect(create).toHaveBeenLastCalledWith(first);

    view.rerender(<Shown blob={first} />);
    expect(create).toHaveBeenCalledTimes(1);

    view.rerender(<Shown blob={second} />);
    expect(revoke).toHaveBeenCalledWith("blob:1");
    expect(view.getByTestId("url").textContent).toBe("blob:2");

    view.rerender(<Shown blob={null} />);
    expect(revoke).toHaveBeenCalledWith("blob:2");
    expect(view.getByTestId("url").textContent).toBe("none");

    view.rerender(<Shown blob={first} />);
    expect(view.getByTestId("url").textContent).toBe("blob:3");
    view.unmount();
    expect(revoke).toHaveBeenCalledWith("blob:3");
    expect(revoke).toHaveBeenCalledTimes(3);
  });
});
