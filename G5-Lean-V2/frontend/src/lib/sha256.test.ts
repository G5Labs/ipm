// The script SHA-256 used where WebCrypto is missing gives the digests the
// server and WebCrypto give (`g5.documents.comments`).

import { afterEach, describe, expect, it, vi } from "vitest";
import { paddedLength, sha256 } from "./sha256";
import { sha256Utf8 } from "./commentAnchors";

const hex = (bytes: Uint8Array) =>
  Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
const utf8 = (text: string) => new TextEncoder().encode(text);

describe("sha256", () => {
  it("gives the FIPS 180-4 test vectors", () => {
    expect(hex(sha256(utf8("")))).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    expect(hex(sha256(utf8("abc")))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    expect(
      hex(
        sha256(
          utf8("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        ),
      ),
    ).toBe("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
  });

  it("agrees with WebCrypto across block boundaries and multibyte text", async () => {
    const texts: string[] = [];
    for (const length of [1, 54, 55, 56, 57, 63, 64, 65, 119, 120, 128, 1000]) {
      texts.push("x".repeat(length));
    }
    texts.push(
      "target line",
      "inserted one\ninserted two\n",
      "日本語 ✓ 🙂\r\n",
    );
    for (const text of texts) {
      const expected = new Uint8Array(
        await globalThis.crypto.subtle.digest("SHA-256", utf8(text)),
      );
      expect(hex(sha256(utf8(text)))).toBe(hex(expected));
    }
  });
});

describe("paddedLength", () => {
  it("pads to whole blocks, past the 32-bit range too", () => {
    expect(paddedLength(0)).toBe(64);
    expect(paddedLength(55)).toBe(64);
    expect(paddedLength(56)).toBe(128);
    expect(paddedLength(2 ** 31 - 72)).toBe(2 ** 31);
    expect(paddedLength(2 ** 32 - 72)).toBe(2 ** 32);
  });
});

describe("sha256Utf8 without WebCrypto", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("computes the same digest on a page that is not a secure context", async () => {
    const withWebCrypto = await sha256Utf8("target line");
    vi.stubGlobal("crypto", {});
    await expect(sha256Utf8("target line")).resolves.toBe(withWebCrypto);
  });
});
