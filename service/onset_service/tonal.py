"""Independent tonal baseline. Correlations are scores, never probabilities."""
from __future__ import annotations
import re
import numpy as np

NOTES = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
MINOR = ["Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#", "C#"]
MAJOR = ["B", "F#", "C#", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E"]
ENHARMONIC = {"Db": "C#", "D#": "Eb", "Gb": "F#", "G#": "Ab", "A#": "Bb", "Cb": "B", "B#": "C", "Fb": "E", "E#": "F"}
PROFILES = {"major": np.array([6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88]),
            "minor": np.array([6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17])}


def camelot(key):
    if isinstance(key, dict):
        key = key.get("camelot") or key.get("name")
    key = str(key or "").strip().replace("♯", "#").replace("♭", "b")
    m = re.fullmatch(r"0?(1[0-2]|[1-9])([aAbB])", key)
    if m:
        return m[1] + m[2].upper()
    m = re.fullmatch(r"([A-Ga-g][#b]?)\s*(major|minor|maj|min|m)?", key,re.IGNORECASE)
    if not m:
        return None
    note = m[1][0].upper() + m[1][1:]
    note = ENHARMONIC.get(note, note)
    minor = m[2] != 'M' and (m[2] or '').lower() in {"minor", "min", "m"}
    wheel = MINOR if minor else MAJOR
    return str(wheel.index(note) + 1) + ("A" if minor else "B")


def sounding_key(key, rate=1., key_lock=True, transpose=0):
    c = camelot(key)
    if not c or rate <= 0:
        return None
    wheel = MINOR if c[-1] == "A" else MAJOR
    note = wheel[int(c[:-1]) - 1]
    shift = transpose + (0 if key_lock else 12 * np.log2(rate))
    rounded = int(round(shift))
    new = NOTES[(NOTES.index(note) + rounded) % 12]
    return {"camelot": str(wheel.index(new) + 1) + c[-1], "semitones": float(shift),"weak":isinstance(key,dict) and key.get('state')=='weak',
            "detuning_cents": float((shift - rounded) * 100)}


def estimate_key(chroma, tuning_cents=0):
    chroma = np.asarray(chroma, dtype=float)
    v = np.mean(chroma, axis=1) if chroma.ndim == 2 else chroma
    if v.shape != (12,) or not np.all(np.isfinite(v)):
        raise ValueError("Expected twelve finite pitch-class values")
    norm = np.linalg.norm(v - v.mean())
    if norm < 1e-5 or np.max(v) < 1e-5:
        return {"state": "unknown", "source": "chroma-template-v1", "alternatives": [], "reason": "Insufficient tonal evidence"}
    values = []
    for mode, profile in PROFILES.items():
        for shift, note in enumerate(NOTES):
            template = np.roll(profile, shift)
            score = float(np.dot(v - v.mean(), template - template.mean()) / (norm * np.linalg.norm(template - template.mean())))
            name = f"{note} {mode}"
            values.append({"name": name, "camelot": camelot(name), "strength": round(score, 4)})
    values.sort(key=lambda x: -x["strength"])
    weak=norm/max(np.linalg.norm(v),1e-9)<.12 or values[0]['strength']-values[1]['strength']<.03
    return {**values[0], "state": "weak" if weak else "estimated", "source": "chroma-template-v1", "alternatives": values[1:3],
            "margin": round(values[0]["strength"] - values[1]["strength"], 4),
            "confidence": "unverified", "tuning_cents": float(tuning_cents)}


def compatibility(a, b):
    if any(isinstance(k,dict) and (k.get('weak') or k.get('state')=='weak') for k in (a,b)):return None
    a, b = camelot(a), camelot(b)
    if not a or not b:
        return None
    step = (int(b[:-1]) - int(a[:-1])) % 12
    if a == b:
        return 1.
    if (a[-1] == b[-1] and step in (1, 11)) or (a[-1] != b[-1] and step == 0):
        return .85
    return .2
