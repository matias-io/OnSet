"""Genre suggestions learned from the DJ's own tags.

Tracks are compared by sound, then each untagged track is offered the genres of the
tagged tracks it most resembles, with those tracks named as the reason. Two
representations exist and are never mixed: the Discogs-EffNet embedding from deep
analysis, and a native timbre/rhythm summary for tracks analysed without it.
Suggestions are unverified until the DJ accepts them."""
from __future__ import annotations
from pathlib import Path
import numpy as np

TEXTURE = ("brightness", "flatness", "flux", "percussive", "low_transient", "sustained_bass")


def user_genres(track):
    """Genres the DJ set by hand; imported or researched tags do not count as training labels."""
    return [g for g in (track.get("overrides", {}).get("genres") or []) if isinstance(g, str) and g.strip()]


def vector(track):
    """(space, vector) for a track, or None when no usable analysis files exist."""
    analysis = track.get("analysis") or {}
    data = (analysis.get("deep") or {}).get("models", {}).get("genre", {}).get("data") or {}
    if data.get("embedding") and Path(data["embedding"]).is_file():
        try:
            return "discogs-effnet", np.load(data["embedding"]).astype(float)
        except (OSError, ValueError):
            pass
    path = (analysis.get("features") or {}).get("path")
    if not path or not Path(path).is_file():
        return None
    try:
        with np.load(path) as f:
            parts = [f["mfcc"].mean(axis=0), f["mfcc"].std(axis=0), f["rhythm_pattern"].mean(axis=0),
                     [float(np.median(f[name])) for name in TEXTURE]]
    except (OSError, KeyError, ValueError):
        return None
    values = np.concatenate([np.asarray(p, dtype=float).ravel() for p in parts])
    return ("native-timbre", values) if np.all(np.isfinite(values)) else None


def suggest(tracks, track_ids=None, neighbours=3, minimum=.5, limit=3):
    vectors = {t["id"]: v for t in tracks if (v := vector(t))}
    by_id = {t["id"]: t for t in tracks}
    labelled = {i: user_genres(by_id[i]) for i in vectors if user_genres(by_id[i])}
    results = []
    for space in sorted({s for s, _ in vectors.values()}):
        ids = [i for i, (s, _) in vectors.items() if s == space]
        matrix = np.stack([vectors[i][1] for i in ids])
        # Standardise each dimension across the library, then compare directions.
        matrix = (matrix - matrix.mean(axis=0)) / np.maximum(matrix.std(axis=0), 1e-9)
        matrix /= np.maximum(np.linalg.norm(matrix, axis=1, keepdims=True), 1e-9)
        similarity = matrix @ matrix.T
        index = {i: n for n, i in enumerate(ids)}
        tagged = [i for i in ids if i in labelled]
        for track_id in ids:
            if track_id in labelled or (track_ids and track_id not in track_ids) or not tagged:
                continue
            scores = {}
            for genre in sorted({g for i in tagged for g in labelled[i]}):
                members = sorted(((float(similarity[index[track_id], index[i]]), i) for i in tagged if genre in labelled[i]),
                                 reverse=True)[:neighbours]
                score = float(np.mean([s for s, _ in members]))
                if score >= minimum:
                    scores[genre] = {"genre": genre, "score": round(score, 3), "examples": [i for _, i in members],
                                     "evidence_tracks": len([1 for i in tagged if genre in labelled[i]])}
            ranked = sorted(scores.values(), key=lambda s: (-s["score"], s["genre"]))[:limit]
            results.append({"track_id": track_id, "space": space, "suggestions": ranked,
                            "confidence": "unverified", "source": "your-tags-nearest-neighbours-v1"})
    return {"suggestions": results, "tagged_tracks": len(labelled),
            "spaces": {space: sum(1 for s, _ in vectors.values() if s == space) for space in {s for s, _ in vectors.values()}},
            "note": "Suggestions compare sound only with tracks you tagged yourself; accept them to make them labels."}
