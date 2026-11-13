#!/usr/bin/env python3
"""Generates the image and PDF fixtures the viewer tests open.

Standard library only, so the files are reproducible from this script:

- transparent.png: 64x48 RGBA, opaque on the left fading to transparent on
  the right, so the checkerboard shows through.
- animated.gif: two 16x16 frames, red then blue, looping.
- large.png: 1600x1200 opaque stripes, larger than the viewer's panel.
- huge-header.png: a header declaring 30000x30000 pixels (900 megapixels)
  over a bogus image chunk; the viewer must refuse it before decoding.
- corrupt.png: a PNG signature followed by garbage.
- scripted.svg: an SVG whose script and onload handler must never run.
- text.pdf: three pages of Helvetica text, including the word "quarterly".
- long.pdf: sixty pages of text.
- protected.pdf: text.pdf's first page under the standard security handler
  (RC4, 40-bit), user and owner password "secret".
- no-text.pdf: one page with a filled rectangle and no text.
- corrupt.pdf: a PDF header followed by garbage.

Run from this directory: python3 generate.py
"""

from __future__ import annotations

import hashlib
import struct
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent


# ---------------------------------------------------------------- PNG ----


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def png(width: int, height: int, rows: list[bytes]) -> bytes:
    raw = b"".join(b"\x00" + row for row in rows)
    return (
        PNG_SIGNATURE
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(raw, 9))
        + png_chunk(b"IEND", b"")
    )


def transparent_png() -> bytes:
    width, height = 64, 48
    rows = []
    for y in range(height):
        row = bytearray()
        for x in range(width):
            alpha = 255 - (x * 255) // (width - 1)
            row += bytes((220, 60 + y * 3, 40, alpha))
        rows.append(bytes(row))
    return png(width, height, rows)


def large_png() -> bytes:
    """1600x1200 opaque stripes, larger than the viewer's panel."""
    width, height = 1600, 1200
    rows = []
    for y in range(height):
        row = bytearray()
        for x in range(width):
            on = ((x // 100) + (y // 100)) % 2 == 0
            row += bytes((30, 90, 200, 255)) if on else bytes((245, 245, 245, 255))
        rows.append(bytes(row))
    return png(width, height, rows)


def huge_header_png() -> bytes:
    return (
        PNG_SIGNATURE
        + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 30000, 30000, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", b"not image data")
        + png_chunk(b"IEND", b"")
    )


# ---------------------------------------------------------------- GIF ----


def lzw_encode(indices: bytes, minimum_code_size: int) -> bytes:
    clear = 1 << minimum_code_size
    end = clear + 1
    code_size = minimum_code_size + 1
    table: dict[bytes, int] = {bytes((i,)): i for i in range(clear)}
    next_code = end + 1
    out_bits: list[tuple[int, int]] = [(clear, code_size)]
    current = b""
    for byte in indices:
        candidate = current + bytes((byte,))
        if candidate in table:
            current = candidate
            continue
        out_bits.append((table[current], code_size))
        if next_code < 4096:
            table[candidate] = next_code
            next_code += 1
            if next_code - 1 == (1 << code_size) and code_size < 12:
                code_size += 1
        else:
            out_bits.append((clear, code_size))
            table = {bytes((i,)): i for i in range(clear)}
            next_code = end + 1
            code_size = minimum_code_size + 1
        current = bytes((byte,))
    if current:
        out_bits.append((table[current], code_size))
    out_bits.append((end, code_size))
    data = bytearray()
    accumulator = 0
    bit_count = 0
    for code, size in out_bits:
        accumulator |= code << bit_count
        bit_count += size
        while bit_count >= 8:
            data.append(accumulator & 0xFF)
            accumulator >>= 8
            bit_count -= 8
    if bit_count:
        data.append(accumulator & 0xFF)
    return bytes(data)


def gif_sub_blocks(data: bytes) -> bytes:
    out = bytearray()
    for offset in range(0, len(data), 255):
        block = data[offset : offset + 255]
        out.append(len(block))
        out += block
    out.append(0)
    return bytes(out)


def animated_gif() -> bytes:
    width = height = 16
    palette = bytes((255, 0, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255))
    out = bytearray(b"GIF89a")
    out += struct.pack("<HHBBB", width, height, 0xF1, 0, 0)  # 4-colour global table
    out += palette
    out += b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00"  # loop forever
    for colour in (0, 1):
        out += b"\x21\xf9\x04\x00" + struct.pack("<H", 50) + b"\x00\x00"  # 0.5 s
        out += b"\x2c" + struct.pack("<HHHHB", 0, 0, width, height, 0)
        out += bytes((2,)) + gif_sub_blocks(
            lzw_encode(bytes((colour,)) * (width * height), 2)
        )
    out += b"\x3b"
    return bytes(out)


# ---------------------------------------------------------------- PDF ----

PDF_PAD = bytes(
    (
        0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56,
        0xFF, 0xFA, 0x01, 0x08, 0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
        0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
    )
)


def rc4(key: bytes, data: bytes) -> bytes:
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) & 0xFF
        s[i], s[j] = s[j], s[i]
    out = bytearray()
    i = j = 0
    for byte in data:
        i = (i + 1) & 0xFF
        j = (j + s[i]) & 0xFF
        s[i], s[j] = s[j], s[i]
        out.append(byte ^ s[(s[i] + s[j]) & 0xFF])
    return bytes(out)


def pdf_escape(text: str) -> bytes:
    return (
        text.encode("latin-1")
        .replace(b"\\", b"\\\\")
        .replace(b"(", b"\\(")
        .replace(b")", b"\\)")
    )


class Encryption:
    """The standard security handler, revision 2 (RC4, 40-bit keys)."""

    def __init__(self, user_password: str, owner_password: str, file_id: bytes):
        self.permissions = -1
        padded_user = (user_password.encode("latin-1") + PDF_PAD)[:32]
        padded_owner = (owner_password.encode("latin-1") + PDF_PAD)[:32]
        owner_key = hashlib.md5(padded_owner).digest()[:5]
        self.o_value = rc4(owner_key, padded_user)
        self.key = hashlib.md5(
            padded_user
            + self.o_value
            + struct.pack("<i", self.permissions)
            + file_id
        ).digest()[:5]
        self.u_value = rc4(self.key, PDF_PAD)

    def object_key(self, number: int, generation: int) -> bytes:
        return hashlib.md5(
            self.key + struct.pack("<I", number)[:3] + struct.pack("<H", generation)
        ).digest()[:10]

    def encrypt(self, number: int, data: bytes) -> bytes:
        return rc4(self.object_key(number, 0), data)

    def dictionary(self) -> bytes:
        return (
            b"<< /Filter /Standard /V 1 /R 2 /Length 40 /P "
            + str(self.permissions).encode()
            + b" /O <"
            + self.o_value.hex().encode()
            + b"> /U <"
            + self.u_value.hex().encode()
            + b"> >>"
        )


def pdf(pages: list[bytes], encryption: Encryption | None = None, file_id: bytes = b"") -> bytes:
    """A PDF whose pages have the given content streams, with Helvetica as /F1."""
    objects: list[bytes] = []

    def add(body: bytes) -> int:
        objects.append(body)
        return len(objects)

    catalog = add(b"")  # filled once the page tree exists
    tree = add(b"")
    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    page_numbers = []
    for content in pages:
        number = len(objects) + 1
        stream = encryption.encrypt(number, content) if encryption else content
        add(b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream")
        page_numbers.append(
            add(
                b"<< /Type /Page /Parent "
                + str(tree).encode()
                + b" 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 "
                + str(font).encode()
                + b" 0 R >> >> /Contents "
                + str(number).encode()
                + b" 0 R >>"
            )
        )
    objects[catalog - 1] = b"<< /Type /Catalog /Pages " + str(tree).encode() + b" 0 R >>"
    objects[tree - 1] = (
        b"<< /Type /Pages /Kids ["
        + b" ".join(str(n).encode() + b" 0 R" for n in page_numbers)
        + b"] /Count "
        + str(len(page_numbers)).encode()
        + b" >>"
    )
    encrypt_ref = b""
    if encryption:
        encrypt_ref = b" /Encrypt " + str(add(encryption.dictionary())).encode() + b" 0 R"
    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for index, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += str(index).encode() + b" 0 obj\n" + body + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 " + str(len(objects) + 1).encode() + b"\n0000000000 65535 f \n"
    for offset in offsets:
        out += f"{offset:010d} 00000 n \n".encode()
    out += (
        b"trailer\n<< /Size "
        + str(len(objects) + 1).encode()
        + b" /Root 1 0 R"
        + encrypt_ref
        + (b" /ID [<" + file_id.hex().encode() + b"> <" + file_id.hex().encode() + b">]" if file_id else b"")
        + b" >>\nstartxref\n"
        + str(xref).encode()
        + b"\n%%EOF\n"
    )
    return bytes(out)


def text_page(number: int, total: int) -> bytes:
    lines = [
        f"Hello G5 page {number} of {total}",
        "The quarterly report is attached.",
        f"Page {number} ends here.",
    ]
    body = b"BT /F1 24 Tf 72 700 Td 30 TL\n"
    for line in lines:
        body += b"(" + pdf_escape(line) + b") Tj T*\n"
    return body + b"ET\n"


def main() -> None:
    (HERE / "transparent.png").write_bytes(transparent_png())
    (HERE / "animated.gif").write_bytes(animated_gif())
    (HERE / "large.png").write_bytes(large_png())
    (HERE / "huge-header.png").write_bytes(huge_header_png())
    (HERE / "corrupt.png").write_bytes(PNG_SIGNATURE + b"this is not a png")
    (HERE / "scripted.svg").write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80" '
        'onload="window.__g5SvgRan = true">\n'
        "  <script>window.__g5SvgRan = true;</script>\n"
        '  <rect x="8" y="8" width="104" height="64" rx="8" fill="#2d7"/>\n'
        '  <text x="60" y="48" font-size="20" text-anchor="middle" fill="#fff">SVG</text>\n'
        "</svg>\n",
        encoding="utf-8",
    )
    (HERE / "text.pdf").write_bytes(pdf([text_page(n, 3) for n in (1, 2, 3)]))
    (HERE / "long.pdf").write_bytes(pdf([text_page(n, 60) for n in range(1, 61)]))
    file_id = hashlib.md5(b"g5 protected fixture").digest()
    (HERE / "protected.pdf").write_bytes(
        pdf([text_page(1, 1)], Encryption("secret", "secret", file_id), file_id)
    )
    (HERE / "no-text.pdf").write_bytes(pdf([b"0.18 0.47 0.84 rg 100 500 300 200 re f\n"]))
    (HERE / "corrupt.pdf").write_bytes(b"%PDF-1.7\n" + b"\x00garbage" * 64)


if __name__ == "__main__":
    main()
