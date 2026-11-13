import { describe, expect, it, vi } from "vitest";

import { randomUuid } from "./randomUuid";

describe("randomUuid", () => {
  it("uses the native browser implementation when it is available", () => {
    const native = "11111111-2222-4333-8444-555555555555";
    const randomUUID = vi.fn(() => native);
    const source = {
      getRandomValues: vi.fn(),
      randomUUID,
    } as unknown as Crypto;

    expect(randomUuid(source)).toBe(native);
    expect(randomUUID).toHaveBeenCalledOnce();
  });

  it("mints a version-4 UUID when randomUUID is unavailable over HTTP", () => {
    const source = {
      getRandomValues: (bytes: Uint8Array) => {
        bytes.set(Array.from({ length: 16 }, (_, index) => index));
        return bytes;
      },
    } as unknown as Crypto;

    expect(randomUuid(source)).toBe("00010203-0405-4607-8809-0a0b0c0d0e0f");
  });
});
