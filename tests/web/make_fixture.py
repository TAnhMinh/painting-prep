"""Writes tests/web/fixture.png: the input of the WebKit smoke test.

A small, lossless, opaque sRGB PNG with no colour chunks (spec §8), so the
browser and the CLI decode identical bytes. Standard library only. The
committed file is the reference; rerun this only to change the fixture.
"""

import pathlib
import struct
import zlib

WIDTH, HEIGHT = 320, 240

# Saturated and everyday colours, so L* is exercised away from the grey axis.
PATCHES = [
    (255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0),
    (0, 255, 255), (255, 0, 255), (224, 172, 105), (141, 85, 36),
    (34, 85, 34), (70, 130, 180), (128, 0, 32), (250, 240, 230),
]


def pixel(x, y):
    if y < 80:
        # Grey ramp across the full range: every bucket of every n is hit.
        g = x * 255 // (WIDTH - 1)
        return (g, g, g)
    if y < 160:
        # One patch per 1/12 of the width.
        return PATCHES[x * len(PATCHES) // WIDTH]
    # Hue ramp over a dark-to-light vertical ramp.
    t = (y - 160) * 255 // (HEIGHT - 161)
    return (x * 255 // (WIDTH - 1), t, 255 - t)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def main():
    raw = bytearray()
    for y in range(HEIGHT):
        raw.append(0)  # filter: none
        for x in range(WIDTH):
            raw.extend(pixel(x, y))
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 2, 0, 0, 0))  # 8-bit RGB
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    out = pathlib.Path(__file__).with_name("fixture.png")
    out.write_bytes(png)
    print(f"wrote {out} ({len(png)} bytes)")


if __name__ == "__main__":
    main()
