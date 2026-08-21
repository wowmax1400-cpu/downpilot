#!/usr/bin/env python3
"""Generate DownPilot icons (pure stdlib PNG writer — no PIL required).

Draws a dark rounded square with a purple gradient and a white download
arrow. Writes:
  - src-tauri/icons/{icon.png (512), 32x32.png, 128x128.png}
  - extension/icons/{16.png, 32.png, 48.png, 128.png}
"""
import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def png_chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def write_png(path: str, size: int, pixel_fn) -> None:
    rows = b""
    for y in range(size):
        rows += b"\x00"
        for x in range(size):
            rows += bytes(pixel_fn(x, y))
    png = b"\x89PNG\r\n\x1a\n"
    png += png_chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += png_chunk(b"IDAT", zlib.compress(rows, 9))
    png += png_chunk(b"IEND", b"")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(png)


def in_rounded_rect(x: float, y: float, size: float, margin: float, radius: float) -> bool:
    x0, y0, x1, y1 = margin, margin, size - margin, size - margin
    if x < x0 or x > x1 or y < y0 or y > y1:
        return False
    cx = min(max(x, x0 + radius), x1 - radius)
    cy = min(max(y, y0 + radius), y1 - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius


def make_pixel_fn(size: int):
    s = float(size)
    m = s * 0.06          # margin
    r = s * 0.22          # corner radius
    # Arrow geometry (fractions of size)
    shaft_x0, shaft_x1 = 0.44, 0.56
    shaft_y0, shaft_y1 = 0.40, 0.60
    head_y0, head_y1 = 0.54, 0.74
    head_half = 0.22

    def inside_arrow(x: float, y: float) -> bool:
        # shaft
        if shaft_x0 * s <= x <= shaft_x1 * s and shaft_y0 * s <= y <= shaft_y1 * s:
            return True
        # arrow head (triangle)
        t = (y - head_y0 * s) / (head_y1 * s - head_y0 * s)
        if 0.0 <= t <= 1.0:
            half = head_half * s * t
            return abs(x - s / 2) <= half
        # underline bar
        if head_y1 * s + 0.02 * s <= y <= head_y1 * s + 0.09 * s:
            return abs(x - s / 2) <= 0.20 * s
        return False

    def px(x: int, y: int) -> tuple:
        fx, fy = x + 0.5, y + 0.5
        if not in_rounded_rect(fx, fy, s, m, r):
            return (0, 0, 0, 0)
        # vertical gradient: #1a0f2e top -> #0b0714 bottom, with purple sheen
        t = fy / s
        bg = (
            int(26 + (11 - 26) * t),
            int(15 + (7 - 15) * t),
            int(46 + (20 - 46) * t),
            255,
        )
        if inside_arrow(fx, fy):
            # white -> lavender gradient along the shaft
            at = (fy - 0.40 * s) / (0.83 * s)
            at = max(0.0, min(1.0, at))
            return (
                int(255 - 40 * at),
                int(255 - 70 * at),
                int(255 - 60 * at),
                255,
            )
        # accent glow near the arrow
        if abs(fx - s / 2) < 0.26 * s and 0.32 * s < fy < 0.80 * s:
            g = 0.5
            return (
                int(bg[0] * (1 - g) + 124 * g),
                int(bg[1] * (1 - g) + 58 * g),
                int(bg[2] * (1 - g) + 237 * g),
                255,
            )
        return bg

    return px


def main() -> None:
    sizes = {
        "src-tauri/icons/icon.png": 512,
        "src-tauri/icons/32x32.png": 32,
        "src-tauri/icons/128x128.png": 128,
        "extension/icons/16.png": 16,
        "extension/icons/32.png": 32,
        "extension/icons/48.png": 48,
        "extension/icons/128.png": 128,
    }
    for rel, size in sizes.items():
        path = os.path.join(ROOT, rel)
        write_png(path, size, make_pixel_fn(size))
        print(f"wrote {path} ({size}x{size})")


if __name__ == "__main__":
    main()
