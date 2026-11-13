import { describe, expect, it } from "vitest";
import { imageDimensions } from "./imageDimensions";

function bytes(...parts: (number[] | string)[]): Uint8Array {
  const out: number[] = [];
  for (const part of parts) {
    if (typeof part === "string")
      for (const char of part) out.push(char.charCodeAt(0));
    else out.push(...part);
  }
  return new Uint8Array(out);
}

const u16be = (value: number) => [(value >> 8) & 0xff, value & 0xff];
const u32be = (value: number) => [
  (value >>> 24) & 0xff,
  (value >> 16) & 0xff,
  (value >> 8) & 0xff,
  value & 0xff,
];
const u16le = (value: number) => [value & 0xff, (value >> 8) & 0xff];
const u32le = (value: number) => [
  value & 0xff,
  (value >> 8) & 0xff,
  (value >> 16) & 0xff,
  (value >>> 24) & 0xff,
];

// `g5.documents.viewers.images`: the declared size is read before decoding.
describe("imageDimensions", () => {
  it("reads a PNG header, including a size the viewer must refuse", () => {
    const png = (width: number, height: number) =>
      bytes(
        [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
        u32be(13),
        "IHDR",
        u32be(width),
        u32be(height),
        [8, 6, 0, 0, 0],
      );
    expect(imageDimensions(png(64, 48))).toEqual({ width: 64, height: 48 });
    expect(imageDimensions(png(30000, 30000))).toEqual({
      width: 30000,
      height: 30000,
    });
    expect(imageDimensions(png(64, 48).slice(0, 20))).toBeNull();
  });

  it("reads GIF and BMP headers", () => {
    expect(
      imageDimensions(bytes("GIF89a", u16le(16), u16le(16), [0xf1, 0, 0])),
    ).toEqual({ width: 16, height: 16 });
    const bmp = (headerSize: number, size: number[]) =>
      bytes("BM", u32le(0), u32le(0), u32le(54), u32le(headerSize), size);
    expect(
      imageDimensions(bmp(40, [...u32le(640), ...u32le(-480 >>> 0)])),
    ).toEqual({ width: 640, height: 480 });
    expect(imageDimensions(bmp(12, [...u16le(32), ...u16le(16)]))).toEqual({
      width: 32,
      height: 16,
    });
  });

  it("reads the three WebP header kinds", () => {
    const riff = (chunk: string, payload: number[]) =>
      bytes("RIFF", u32le(100), "WEBP", chunk, u32le(payload.length), payload);
    expect(
      imageDimensions(
        riff("VP8 ", [0, 0, 0, 0x9d, 0x01, 0x2a, ...u16le(320), ...u16le(240)]),
      ),
    ).toEqual({ width: 320, height: 240 });
    const width = 799;
    const height = 599;
    const bits = width | (height << 14);
    expect(
      imageDimensions(riff("VP8L", [0x2f, ...u32le(bits), 0, 0, 0, 0, 0, 0])),
    ).toEqual({ width: 800, height: 600 });
    expect(
      imageDimensions(
        riff("VP8X", [0x10, 0, 0, 0, 0xff, 0x0f, 0x00, 0x1f, 0x00, 0x00, 0, 0]),
      ),
    ).toEqual({ width: 4096, height: 32 });
  });

  it("walks JPEG segments to the first frame header", () => {
    const jpeg = bytes(
      [0xff, 0xd8],
      [0xff, 0xe0],
      u16be(16),
      "JFIF",
      [0, 1, 1, 0, 0, 1, 0, 1, 0, 0],
      [0xff, 0xff, 0xc0], // a fill byte before the frame marker
      u16be(17),
      [8],
      u16be(1080),
      u16be(1920),
      [3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1],
    );
    expect(imageDimensions(jpeg)).toEqual({ width: 1920, height: 1080 });
    // Scan data before any frame header means the size is unknown.
    expect(
      imageDimensions(bytes([0xff, 0xd8, 0xff, 0xda], u16be(2), [0, 0])),
    ).toBeNull();
  });

  it("returns null for other or truncated input", () => {
    expect(imageDimensions(new Uint8Array(0))).toBeNull();
    expect(imageDimensions(bytes("<svg xmlns='x'/>"))).toBeNull();
    expect(imageDimensions(bytes([0xff, 0xd8, 0xff]))).toBeNull();
    expect(imageDimensions(bytes("RIFF", u32le(0), "WEBP", "ALPH"))).toBeNull();
  });
});
