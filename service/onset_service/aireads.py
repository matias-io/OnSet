"""Structure reads by AI providers: a bar-gridded spectrogram image for them to look at, and
importing what they report as observations beside the other sources.

The image is drawn from the analysis's stored 64-band mel spectrogram, so no audio is decoded
and nothing needs matplotlib. Every bar is the same number of pixels wide, so a reader can
count bars exactly: thin lines every 4 bars, bright lines every 16, numbered on the top strip.
"""
from __future__ import annotations

import struct
import zlib
from pathlib import Path

import numpy as np

EVENTS = ("drop", "breakdown", "second_drop", "outro")
STATES = {"present": "estimated", "absent": "absent", "unknown": "unknown"}

# 3x5 pixel digits for the bar numbers on the header strip.
_DIGITS = {
    "0": ["111", "101", "101", "101", "111"], "1": ["010", "110", "010", "010", "111"],
    "2": ["111", "001", "111", "100", "111"], "3": ["111", "001", "111", "001", "111"],
    "4": ["101", "101", "111", "001", "001"], "5": ["111", "100", "111", "001", "111"],
    "6": ["111", "100", "111", "101", "111"], "7": ["111", "001", "001", "001", "001"],
    "8": ["111", "101", "111", "101", "111"], "9": ["111", "101", "111", "001", "111"],
}
# Dark to bright, roughly magma.
_COLOURS = np.array([[0, 0, 4], [59, 15, 112], [140, 41, 129], [222, 73, 104], [254, 159, 109],
                     [252, 253, 191]], dtype=float)


def _colour(level):
    """Maps 0..1 to RGB along the palette."""
    x = np.clip(level, 0, 1) * (len(_COLOURS) - 1)
    i = np.minimum(x.astype(int), len(_COLOURS) - 2)
    t = (x - i)[..., None]
    return (_COLOURS[i] * (1 - t) + _COLOURS[i + 1] * t).astype(np.uint8)


def _png(rgb):
    """Encodes an H x W x 3 uint8 array as PNG."""
    h, w, _ = rgb.shape
    raw = b"".join(b"\x00" + rgb[y].tobytes() for y in range(h))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def _text(img, x, y, s, colour=(255, 255, 255)):
    for k, ch in enumerate(s):
        for r, row in enumerate(_DIGITS[ch]):
            for c, bit in enumerate(row):
                if bit == "1" and 0 <= y + r < img.shape[0] and 0 <= x + 4 * k + c < img.shape[1]:
                    img[y + r, x + 4 * k + c] = colour


def render(analysis, out: Path, px_per_bar=8, band_px=3, header=9):
    """Draws the bar-gridded spectrogram and returns how to read it."""
    spec = analysis.get("spectrogram") or {}
    values = np.asarray(spec.get("values") or [], dtype=float)
    downbeats = np.asarray(analysis.get("downbeats") or [], dtype=float)
    duration = float(spec.get("duration") or analysis.get("duration") or 0)
    if values.ndim != 2 or values.size == 0 or len(downbeats) < 2 or duration <= 0:
        raise ValueError("This track has no spectrogram and bar grid yet; analyse it first")
    bands, frames = values.shape
    bars = len(downbeats)
    edges = np.append(downbeats, min(duration, downbeats[-1] + (downbeats[-1] - downbeats[-2])))
    # One column per pixel: the frame nearest that pixel's time within its bar.
    cols = []
    for b in range(bars):
        times = edges[b] + (np.arange(px_per_bar) + 0.5) / px_per_bar * (edges[b + 1] - edges[b])
        cols.append(np.clip((times / duration * frames).astype(int), 0, frames - 1))
    cols = np.concatenate(cols)
    level = (values[::-1, cols] - values.min()) / max(1e-9, values.max() - values.min())
    body = np.repeat(_colour(level), band_px, axis=0)
    img = np.zeros((header + body.shape[0], body.shape[1], 3), dtype=np.uint8)
    img[header:] = body
    for b in range(0, bars, 4):
        x = b * px_per_bar
        img[header:, x] = (255, 255, 255) if b % 16 == 0 else (110, 110, 120)
        if b % 16 == 0:
            img[:header, x] = (255, 255, 255)
            _text(img, x + 2, 2, str(b))
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(_png(img))
    return {"path": str(out), "bars": bars, "px_per_bar": px_per_bar, "header_px": header,
            "width": int(img.shape[1]), "height": int(img.shape[0]), "duration": duration,
            "bar_times": [round(float(t), 2) for t in downbeats],
            "how_to_read": f"Each bar is {px_per_bar} pixels wide, starting at bar 0 on the left. "
                           f"Thin grey lines every 4 bars, white lines every 16 bars, numbered on the "
                           f"top strip. Low frequencies are at the bottom."}


def observations(track, source, family, events, confidence="unverified", notes=""):
    """Turns a provider's event read into observations, replacing that source's earlier ones.

    `events` maps drop/breakdown/second_drop/outro to {"state": present|absent|unknown, "bar"}.
    A present event needs a bar inside the grid; absent and unknown keep no bar.
    """
    downbeats = list((track.get("analysis") or {}).get("downbeats") or [])
    kept = [o for o in track.get("observations", []) if o.get("source") != source]
    for event in EVENTS:
        read = (events or {}).get(event) or {"state": "unknown"}
        state = read.get("state")
        if state not in STATES:
            raise ValueError(f"{event}: state must be present, absent or unknown")
        bar = read.get("bar")
        if state == "present":
            if not isinstance(bar, int) or not 0 <= bar < len(downbeats):
                raise ValueError(f"{event}: a present event needs a bar between 0 and {len(downbeats) - 1}")
        else:
            bar = None
        kept.append({"event": event, "source": source, "family": family, "state": STATES[state],
                     "original_time": downbeats[bar] if bar is not None else None, "bar": bar,
                     "confidence": confidence, "notes": str(notes)[:500]})
    return kept
