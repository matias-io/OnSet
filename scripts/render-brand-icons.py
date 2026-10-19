"""Renders OnSet's app icon (PNG and multi-size ICO) from the mark's geometry.

The geometry matches assets/brand/onset-mark.svg (a 64-unit grid). It is drawn here with
signed distances and 4x4 supersampling, so no SVG renderer is needed.

Usage: python scripts/render-brand-icons.py   (needs numpy)
Writes crates/onset-desktop/icons/icon.png (256 px) and icon.ico (16 to 256 px).
"""
import struct
import zlib
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
BASE = (0x0C, 0x0D, 0x10)
VIOLET = (0xA1, 0x94, 0xFF)
MAGENTA = (0xF4, 0x72, 0xB6)
LINE = [(10, 36), (24, 36), (28.5, 13), (33, 48), (36.5, 29), (40, 40), (43.5, 34),
        (47, 37.5), (50, 36), (54, 36)]
# At 24 px and below the ripples blur together, so the small icon keeps one spike, one dip
# and a single ripple, spread wider.
LINE_SMALL = [(8, 36), (19, 36), (27, 10), (36, 52), (43, 33), (48, 36), (56, 36)]
STROKE = 3.6
DOT = ((28.5, 13), 3.4)
CORNER = 15


def segment_distance(px, py, a, b):
    ax, ay = a
    bx, by = b
    dx, dy = bx - ax, by - ay
    t = np.clip(((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy), 0, 1)
    return np.hypot(px - (ax + t * dx), py - (ay + t * dy))


def render(size):
    ss = 4
    n = size * ss
    # Sample centres in the mark's 64-unit space.
    coords = (np.arange(n) + 0.5) / n * 64
    px, py = np.meshgrid(coords, coords)
    unit = 64 / size  # one output pixel in mark units
    # Small icons get a slightly bolder line so it survives at 16 px.
    stroke = max(STROKE, 1.6 * unit)
    # Rounded square.
    q = np.maximum(np.abs(px - 32) - (32 - CORNER), np.abs(py - 32) - (32 - CORNER))
    qx = np.maximum(np.abs(px - 32) - (32 - CORNER), 0)
    qy = np.maximum(np.abs(py - 32) - (32 - CORNER), 0)
    box = np.hypot(qx, qy) + np.minimum(q, 0) - CORNER
    inside = (box <= 0).astype(float)
    path = LINE_SMALL if size <= 24 else LINE
    line_d = np.min([segment_distance(px, py, a, b) for a, b in zip(path, path[1:])], axis=0)
    line = (line_d <= stroke / 2).astype(float)
    (cx, cy), r = ((27, 10), 3.4) if size <= 24 else DOT
    dot = (np.hypot(px - cx, py - cy) <= max(r, 1.3 * unit)).astype(float)

    rgb = np.zeros((n, n, 3))
    for i in range(3):
        c = BASE[i] * (1 - line) + VIOLET[i] * line
        rgb[..., i] = c * (1 - dot) + MAGENTA[i] * dot
    alpha = inside
    # Average the supersamples, premultiplied, then unpremultiply.
    pre = (rgb * alpha[..., None]).reshape(size, ss, size, ss, 3).mean(axis=(1, 3))
    a = alpha.reshape(size, ss, size, ss).mean(axis=(1, 3))
    out = np.where(a[..., None] > 0, pre / np.maximum(a[..., None], 1e-9), 0)
    return np.dstack([out, a * 255]).round().clip(0, 255).astype(np.uint8)


def png(img):
    h, w, _ = img.shape
    raw = b"".join(b"\x00" + img[y].tobytes() for y in range(h))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(
            ">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    out = ROOT / "crates" / "onset-desktop" / "icons"
    out.mkdir(parents=True, exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [png(render(s)) for s in sizes]
    (out / "icon.png").write_bytes(images[-1])
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries = b""
    for s, data in zip(sizes, images):
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    (out / "icon.ico").write_bytes(header + entries + b"".join(images))
    print("wrote", out / "icon.png", "and", out / "icon.ico")


if __name__ == "__main__":
    main()
