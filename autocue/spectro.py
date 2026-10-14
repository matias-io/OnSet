#!/usr/bin/env python
"""spectro.py analysis.json OUT_DIR [names...] - render a bar-gridded mel spectrogram per track for visual section reading."""
import json, sys
from pathlib import Path
import numpy as np, librosa, matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

A = json.loads(Path(sys.argv[1]).read_text()); out = Path(sys.argv[2]); out.mkdir(exist_ok=True)
only = sys.argv[3:]
for r in A:
    name = Path(r["path"]).stem
    if only and not any(o.lower() in name.lower() for o in only):
        continue
    y, sr = librosa.load(r["path"], sr=22050, mono=True)
    S = librosa.power_to_db(librosa.feature.melspectrogram(y=y, sr=sr, n_fft=2048, hop_length=512, n_mels=96, fmax=8000), ref=np.max)
    db = np.asarray(r["downbeats"]); n = len(db)
    fig, ax = plt.subplots(figsize=(24, 6), dpi=100)
    t = librosa.frames_to_time(np.arange(S.shape[1]), sr=sr, hop_length=512)
    ax.pcolormesh(t, np.arange(S.shape[0]), S, shading="auto", cmap="magma", vmin=-70, vmax=0)
    for i, x in enumerate(db):
        if i % 4 == 0:
            ax.axvline(x, color="white", lw=0.8 if i % 16 else 1.6, alpha=0.9 if i % 16 == 0 else 0.45)
    ticks = [db[i] for i in range(0, n, 8)]
    ax.set_xticks(ticks); ax.set_xticklabels([f"{i}\n{int(db[i])//60}:{int(db[i])%60:02d}" for i in range(0, n, 8)], fontsize=8)
    ax.set_yticks([0, 12, 24, 48, 72, 95]); ax.set_yticklabels(["0", "150Hz", "400Hz", "1.5k", "4k", "8k"], fontsize=8)
    ax.set_xlabel("bar index (thick line = every 16 bars, thin = every 4)  /  time"); ax.set_title(name[:110], fontsize=10)
    ax.set_xlim(0, t[-1]); fig.tight_layout(); fig.savefig(out / (name + ".png")); plt.close(fig)
    print("wrote", name[:70])
