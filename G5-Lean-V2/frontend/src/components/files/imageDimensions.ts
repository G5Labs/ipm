// =============================================================================
// G5 Files — imageDimensions
// =============================================================================
// The pixel size an image file declares in its header, read before any
// decoding so the image viewer can refuse an image whose decoded size would
// be unreasonable, whatever its byte size (`g5.documents.viewers.images`).
// PNG, JPEG, GIF, BMP and WebP are probed; anything else returns null and
// relies on the browser's own limits.
// =============================================================================

export interface ImageDimensions {
  width: number;
  height: number;
}

function ascii(bytes: Uint8Array, offset: number, text: string): boolean {
  if (offset + text.length > bytes.length) return false;
  for (let index = 0; index < text.length; index += 1) {
    if (bytes[offset + index] !== text.charCodeAt(index)) return false;
  }
  return true;
}

function u16be(bytes: Uint8Array, offset: number): number {
  return (bytes[offset] << 8) | bytes[offset + 1];
}

function u32be(bytes: Uint8Array, offset: number): number {
  return (
    bytes[offset] * 0x1000000 +
    ((bytes[offset + 1] << 16) | (bytes[offset + 2] << 8) | bytes[offset + 3])
  );
}

function u16le(bytes: Uint8Array, offset: number): number {
  return bytes[offset] | (bytes[offset + 1] << 8);
}

function u24le(bytes: Uint8Array, offset: number): number {
  return bytes[offset] | (bytes[offset + 1] << 8) | (bytes[offset + 2] << 16);
}

function i32le(bytes: Uint8Array, offset: number): number {
  return (
    bytes[offset] |
    (bytes[offset + 1] << 8) |
    (bytes[offset + 2] << 16) |
    (bytes[offset + 3] << 24)
  );
}

function dimensions(width: number, height: number): ImageDimensions | null {
  return width > 0 && height > 0 ? { width, height } : null;
}

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

function png(bytes: Uint8Array): ImageDimensions | null {
  if (bytes.length < 24) return null;
  if (PNG_SIGNATURE.some((byte, index) => bytes[index] !== byte)) return null;
  if (!ascii(bytes, 12, "IHDR")) return null;
  return dimensions(u32be(bytes, 16), u32be(bytes, 20));
}

function gif(bytes: Uint8Array): ImageDimensions | null {
  if (bytes.length < 10) return null;
  if (!ascii(bytes, 0, "GIF87a") && !ascii(bytes, 0, "GIF89a")) return null;
  return dimensions(u16le(bytes, 6), u16le(bytes, 8));
}

function bmp(bytes: Uint8Array): ImageDimensions | null {
  if (bytes.length < 22 || !ascii(bytes, 0, "BM")) return null;
  const headerSize = i32le(bytes, 14);
  if (headerSize === 12) return dimensions(u16le(bytes, 18), u16le(bytes, 20));
  if (bytes.length < 26) return null;
  return dimensions(i32le(bytes, 18), Math.abs(i32le(bytes, 22)));
}

function webp(bytes: Uint8Array): ImageDimensions | null {
  if (bytes.length < 30) return null;
  if (!ascii(bytes, 0, "RIFF") || !ascii(bytes, 8, "WEBP")) return null;
  if (ascii(bytes, 12, "VP8 ")) {
    if (bytes[23] !== 0x9d || bytes[24] !== 0x01 || bytes[25] !== 0x2a)
      return null;
    return dimensions(u16le(bytes, 26) & 0x3fff, u16le(bytes, 28) & 0x3fff);
  }
  if (ascii(bytes, 12, "VP8L")) {
    if (bytes[20] !== 0x2f) return null;
    const bits =
      bytes[21] | (bytes[22] << 8) | (bytes[23] << 16) | (bytes[24] << 24);
    return dimensions((bits & 0x3fff) + 1, ((bits >>> 14) & 0x3fff) + 1);
  }
  if (ascii(bytes, 12, "VP8X")) {
    return dimensions(u24le(bytes, 24) + 1, u24le(bytes, 27) + 1);
  }
  return null;
}

/** Start-of-frame markers, which carry the frame size. */
const JPEG_SOF = new Set([
  0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
]);

function jpeg(bytes: Uint8Array): ImageDimensions | null {
  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null;
  let position = 2;
  while (position + 3 < bytes.length) {
    if (bytes[position] !== 0xff) return null;
    let marker = bytes[position + 1];
    // Fill bytes before a marker are allowed.
    while (marker === 0xff && position + 2 < bytes.length) {
      position += 1;
      marker = bytes[position + 1];
    }
    position += 2;
    // Markers without a segment.
    if (
      marker === 0xd8 ||
      marker === 0x01 ||
      (marker >= 0xd0 && marker <= 0xd7)
    )
      continue;
    // Scan data or the end of the image before any frame header.
    if (marker === 0xda || marker === 0xd9) return null;
    if (position + 1 >= bytes.length) return null;
    const length = u16be(bytes, position);
    if (length < 2) return null;
    if (JPEG_SOF.has(marker)) {
      if (position + 6 >= bytes.length) return null;
      return dimensions(u16be(bytes, position + 5), u16be(bytes, position + 3));
    }
    position += length;
  }
  return null;
}

/** The declared size of a PNG, JPEG, GIF, BMP or WebP, else null. */
export function imageDimensions(bytes: Uint8Array): ImageDimensions | null {
  return png(bytes) ?? gif(bytes) ?? bmp(bytes) ?? webp(bytes) ?? jpeg(bytes);
}
