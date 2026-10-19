"""Multi-feature boundaries, recurrence and a duration-aware semi-Markov decoder."""
from __future__ import annotations
import numpy as np

LABELS = ["intro", "groove", "vocal", "breakdown", "build", "drop", "stripped", "outro", "silence", "unknown"]


def scale(x):
    x = np.asarray(x, dtype=float)
    lo, hi = np.percentile(x, [10, 90]) if len(x) else (0, 1)
    return np.clip((x - lo) / max(hi - lo, 1e-6), 0, 1)


def infer(features, downbeats, duration):
    n = len(downbeats)
    energy = scale(features["full"])
    bass = scale(features["low"])
    rhythm = scale(features["onset"])
    high = scale(features["high"])
    vocal_known = "vocal" in features
    vocal = np.asarray(features.get("vocal", np.zeros(n)))
    chroma = np.asarray(features["chroma"])
    mfcc = np.asarray(features["mfcc"])
    rhythm_vectors = np.asarray(features["rhythm_pattern"])
    def recurrence(x):
        x = np.asarray(x)
        x = x / np.maximum(np.linalg.norm(x, axis=1, keepdims=True), 1e-8)
        return np.clip(x @ x.T, -1, 1)
    R = {"harmonic": recurrence(chroma), "timbre": recurrence(mfcc), "rhythm": recurrence(rhythm_vectors)}
    X = np.column_stack([energy, bass, rhythm, high, scale(features["flux"]), scale(features["brightness"])])
    novelty = np.zeros(n)
    for i in range(1, n):
        scores = []
        for width in (1, 2, 4, 8):
            left, right = X[max(0, i - width):i], X[i:min(n, i + width)]
            if len(left) and len(right):
                scores.append(float(np.linalg.norm(np.median(right, axis=0) - np.median(left, axis=0))))
        novelty[i] = np.mean(scores) if scores else 0
    novelty = scale(novelty)
    boundaries = [{"bar": i, "time": float(downbeats[i]), "strength": round(float(novelty[i]), 3),
                   "source": "multifeature-v1", "confidence": "unverified"}
                  for i in range(1, n - 1) if novelty[i] > .45 and novelty[i] >= max(novelty[i - 1], novelty[i + 1])]
    slope = np.r_[0, np.diff(high + rhythm)]
    emissions = np.column_stack([
        .6 * (1 - bass) + .4 * rhythm,
        .6 * bass + .4 * rhythm,
        vocal if vocal_known else np.full(n, -.5),
        .7 * (1 - rhythm) + .3 * (1 - bass),
        .5 * high + .4 * scale(slope) + .1 * (1 - bass),
        .4 * bass + .4 * rhythm + .2 * energy,
        .6 * rhythm + .4 * (1 - high),
        .4 * rhythm + .4 * (1 - high) + .2 * (1 - energy),
        np.clip((-np.asarray(features["full"]) - 45) / 20, 0, 1),
        np.full(n, .25),
    ])
    emissions[:, 0] -= np.linspace(0, 1.5, n)
    emissions[:, 7] -= np.linspace(1.5, 0, n)
    # Relative normalization alone makes digital silence tie with breakdown.
    # Use an absolute floor only for near-zero signal, never as energy ranking.
    silent=np.asarray(features['full']) < -90
    emissions[silent,:]=-2
    emissions[silent,LABELS.index('silence')]=2
    prefix = np.vstack([np.zeros(len(LABELS)), np.cumsum(emissions, axis=0)])
    dp = np.full((n + 1, len(LABELS)), -np.inf)
    back = {}
    dp[0] = 0
    lengths = range(1, min(64, n) + 1)
    for end in range(1, n + 1):
        for state in range(len(LABELS)):
            for length in lengths:
                start = end - length
                if start < 0:
                    break
                prior = -.12 * min(abs(length - target) for target in (4, 8, 16, 32)) - .9
                prior += float(novelty[start]) if start else 0
                candidates = dp[start].copy()
                candidates[state] -= .5
                if state == 0 and start > 0:
                    candidates -= 2
                prev = int(np.argmax(candidates))
                value = candidates[prev] + prefix[end, state] - prefix[start, state] + prior
                if value > dp[end, state]:
                    dp[end, state] = value
                    back[end, state] = (start, prev)
    end, state = n, int(np.argmax(dp[n]))
    segments = []
    while end:
        start, prev = back[end, state]
        label = LABELS[state]
        segments.append({"start": float(downbeats[start]), "end": float(downbeats[end]) if end < n else duration,
                         "start_bar": start, "end_bar": end, "label": label,
                         "confidence": "unverified", "source": "semi-markov-v1"})
        end, state = start, prev
    segments.reverse()
    merged = []
    for s in segments:
        if merged and merged[-1]["label"] == s["label"]:
            merged[-1].update(end=s["end"], end_bar=s["end_bar"])
        else:
            merged.append(s)
    repeated = []
    combined = .4 * R["rhythm"] + .4 * R["timbre"] + .2 * R["harmonic"]
    for i in range(0, n - 7, 4):
        matches = [(float(np.mean(np.diag(combined[i:i + 8, j:j + 8]))), j) for j in range(i + 8, n - 7, 4)]
        if matches:
            strength, j = max(matches)
            if strength > .8:
                repeated.append({"a_bar": i, "b_bar": j, "bars": 8, "similarity": strength})
    windows = []
    for s in merged:
        if s["end_bar"] - s["start_bar"] >= 4 and s["label"] not in {"silence", "unknown", "build"}:
            windows.append({"id": f"{s['start_bar']}-{s['end_bar']}", "start": s["start"], "end": s["end"],
                            "start_bar": s["start_bar"], "end_bar": s["end_bar"], "label": s["label"],
                            "vocals": "unknown" if not vocal_known else bool(np.mean(vocal[s["start_bar"]:s["end_bar"]]) > .3)})
    loops=[]
    for length in (4,8):
        for i in range(0,n-length,4):
            seam=float(combined[i,i+length-1])
            if seam>.7 and np.mean(rhythm[i:i+length])>.3:
                loops.append({"id":f"loop-{i}-{length}","start":float(downbeats[i]),"end":float(downbeats[i+length]),
                              "start_bar":i,"end_bar":i+length,"quality":round(seam,3),"vocals":"unknown" if not vocal_known else bool(np.max(vocal[i:i+length])>.3)})
    return {"sections": merged, "boundaries": boundaries, "repetitions": repeated,"loop_candidates":loops,
            "windows": windows, "recurrence": R, "fingerprint": [s["label"] + ":" + str(s["end_bar"] - s["start_bar"]) for s in merged]}
