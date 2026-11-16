"""Optional isolated Linux inference process; stdin request / JSONL events."""
import contextlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys

# Essentia's Discogs-EffNet (CC BY-NC-SA 4.0, personal use): 400 Discogs styles plus a
# 1280-value embedding. Files are fetched once and must match these hashes.
EFFNET = {
    "model": ("https://essentia.upf.edu/models/feature-extractors/discogs-effnet/discogs-effnet-bsdynamic-1.onnx",
              "a280825b334797cf677939db8cd5762c0392aedd0ca6415dbc1cd083f045e43c"),
    "metadata": ("https://essentia.upf.edu/models/feature-extractors/discogs-effnet/discogs-effnet-bsdynamic-1.json",
                 "a2e85b2e7372d5f8e0f35bdd6aeae1139f101087d183d0b2fb60b0ea0f01a0ff"),
}


def fetch_verified(url, sha256, target):
    import hashlib
    import urllib.request
    target = Path(target)
    if target.is_file() and hashlib.sha256(target.read_bytes()).hexdigest() == sha256:
        return target
    target.parent.mkdir(parents=True, exist_ok=True)
    import time
    for attempt in range(3):
        try:
            with urllib.request.urlopen(url, timeout=120) as response:
                content = response.read()
            break
        except OSError:
            if attempt == 2:
                raise
            time.sleep(5 * (attempt + 1))
    if hashlib.sha256(content).hexdigest() != sha256:
        raise RuntimeError(f"Checksum mismatch for {url}")
    temporary = target.with_suffix(".tmp")
    temporary.write_bytes(content)
    temporary.replace(target)
    return target


def effnet_patches(audio):
    """Mel patches matching TensorflowPredictEffnetDiscogs: 16 kHz, 512/256 frames, 128-frame patches, hop 62."""
    import numpy as np
    import essentia.standard as es
    mel = es.TensorflowInputMusiCNN()
    frames = [mel(frame) for frame in es.FrameGenerator(audio, frameSize=512, hopSize=256, startFromZero=True)]
    bands = np.asarray(frames, dtype=np.float32)
    if len(bands) < 128:
        bands = np.pad(bands, ((0, 128 - len(bands)), (0, 0)))
    starts = range(0, len(bands) - 128 + 1, 62)
    return np.stack([bands[s:s + 128] for s in starts])


def discogs_styles(audio_path, cache, output, top=10):
    import numpy as np
    import onnxruntime
    import essentia.standard as es
    folder = Path(cache) / "essentia-models"
    model = fetch_verified(*EFFNET["model"], folder / "discogs-effnet-bsdynamic-1.onnx")
    metadata = json.loads(fetch_verified(*EFFNET["metadata"], folder / "discogs-effnet-bsdynamic-1.json").read_text())
    labels = metadata["classes"]
    audio = es.MonoLoader(filename=audio_path, sampleRate=16000, resampleQuality=4)()
    patches = effnet_patches(audio)
    session = onnxruntime.InferenceSession(str(model), providers=["CPUExecutionProvider"])
    feed = session.get_inputs()[0].name
    predictions, embeddings = [], []
    for start in range(0, len(patches), 64):
        outputs = session.run(None, {feed: patches[start:start + 64]})
        by_width = {o.shape[-1]: o for o in outputs}
        predictions.append(by_width[len(labels)])
        embeddings.append(by_width[1280])
    activations = np.concatenate(predictions).mean(axis=0)
    embedding = np.concatenate(embeddings).mean(axis=0)
    if not np.all(np.isfinite(activations)) or not np.all(np.isfinite(embedding)):
        raise ValueError("Genre model returned non-finite values")
    path = Path(output) / "discogs-effnet-embedding.npy"
    np.save(path, embedding.astype(np.float32))
    order = np.argsort(-activations)[:top]
    return {"styles": [{"label": labels[i], "score": round(float(activations[i]), 4)} for i in order],
            "embedding": str(path), "patches": int(len(patches)), "model": "discogs-effnet-bsdynamic-1",
            "taxonomy": "Discogs styles", "license": "CC BY-NC-SA 4.0"}


def edmformer_chunks(duration, limit=225):
    """Chunk length (s) and join times for EDMFormer. edm98 runs up to TIME_DUR = 420 s through
    MuQ and MusicFM in one pass, and their memory grows with the square of the length: 240 s
    peaked at 7.1 GB and was killed in WSL's default 7.8 GB (La Danza, Ray Of Solar, Joya,
    2026-10-23). A track up to `limit` runs whole, as the model expects; a longer one is split
    into equal chunks no longer than `limit`. Boundaries survive a join, but the model labels a
    chunk's start as a track's start, so labels just after a join are less reliable."""
    import math
    count = max(1, math.ceil(duration / limit))
    length = math.ceil(duration / count)
    return length, [float(length * k) for k in range(1, count)]


def stereo_input(audio, workdir):
    """Demucs 4.0.1 widens mono input to stereo as a view of one channel and then subtracts
    in place (separate.py:171, `wav -= ref.mean()`), which torch refuses: La Danza, Lock In
    and Ray Of Solar got no stems on 2026-10-23. Mono audio gets a real two-channel copy,
    under the same file name so the stems folder keeps the track's name."""
    import numpy as np
    import soundfile as sf
    if sf.info(audio).channels != 1:
        return audio, None
    data, rate = sf.read(audio, always_2d=True)
    folder = Path(workdir) / "stereo-input"
    folder.mkdir(parents=True, exist_ok=True)
    copy = folder / Path(audio).name
    sf.write(copy, np.repeat(data, 2, axis=1), rate, subtype="PCM_16")
    return str(copy), folder


def main():
    if not os.environ.get("ONSET_WORKER_CHILD"):
        try: os.setsid()
        except OSError: pass
    print(json.dumps({"type": "ready", "pid": os.getpid(), "group": os.getpgrp()}), flush=True)
    request = json.loads(sys.stdin.readline())
    if request.get("schema_version") != 1:
        raise ValueError("Unsupported worker protocol")
    output = Path(request["output"])
    output.mkdir(parents=True, exist_ok=True)
    cache = Path(request["cache"])
    cache.mkdir(parents=True, exist_ok=True)
    os.environ["HF_HOME"] = str(cache / "huggingface")
    os.environ["TORCH_HOME"] = str(cache / "torch")
    import torch
    import socket
    socket.setdefaulttimeout(60)
    torch.set_num_threads(min(4,os.cpu_count() or 1))
    os.environ["PATH"] = str(Path(sys.executable).parent) + os.pathsep + os.environ.get("PATH", "")
    device = "cuda" if torch.cuda.is_available() else "cpu"
    result = {"device": device, "models": {}, "stems": {}}
    for name in request["models"]:
        print(json.dumps({"type": "progress", "model": name}), flush=True)
        try:
            checkpoint = output / (name + ".json")
            if checkpoint.exists():
                cached = json.loads(checkpoint.read_text())
                if cached.get("state") == "complete" and cached.get('worker_format')==2:
                    result["models"][name] = cached
                    if name == "demucs":
                        result["stems"] = cached["data"]["stems"]
                    print(json.dumps({"type": "result", "result": result}), flush=True)
                    continue
            # Reserve stdout for the transport, including model-library output.
            with contextlib.redirect_stdout(sys.stderr):
                if name in {"edmformer","allin1"} and not request.get("isolated"):
                    import subprocess
                    interpreter=Path.home()/".local/share/onset/experimental"/name/"bin/python"
                    if not interpreter.is_file():raise RuntimeError(f"Set up the isolated {name} runtime before requesting this experimental source")
                    child={**request,"models":[name],"isolated":True}
                    completed=subprocess.run([str(interpreter),__file__],input=json.dumps(child)+'\n',capture_output=True,text=True,
                                             env={**os.environ,"ONSET_WORKER_CHILD":"1"})
                    messages=[]
                    for line in completed.stdout.splitlines():
                        try: messages.append(json.loads(line))
                        except ValueError:pass
                    final=next((e["result"] for e in reversed(messages) if e.get("type")=="result"),None)
                    if not final:raise RuntimeError("Isolated model failed: "+completed.stderr[-600:])
                    observation=final["models"].get(name,{"state":"failed","error":"Missing isolated result"})
                    result["models"][name]=observation
                    with contextlib.redirect_stdout(sys.__stdout__):
                        print(json.dumps({"type":"result","result":result}),flush=True)
                    continue
                elif name == "beat_this":
                    from beat_this.inference import File2Beats
                    beats, downbeats = File2Beats(checkpoint_path="final0", device=device, dbn=False)(request["audio"])
                    data = {"beats": beats.tolist(), "downbeats": downbeats.tolist()}
                elif name == "demucs":
                    import shutil
                    import subprocess
                    source, scratch = stereo_input(request["audio"], output)
                    try:
                        # FLAC keeps the stems lossless at about 60 % of the 16-bit WAV size.
                        subprocess.run([sys.executable, "-m", "demucs", "-n", "htdemucs", "-d", device, "--flac",
                                        "--out", str(output), source], check=True, stdout=sys.stderr)
                    finally:
                        if scratch:
                            shutil.rmtree(scratch, ignore_errors=True)
                    stems = output / "htdemucs" / Path(request["audio"]).stem
                    result["stems"] = {p.stem: str(p) for p in stems.glob("*.flac")}
                    data = {"stems": result["stems"]}
                elif name == "essentia":
                    import essentia.standard as es
                    audio = es.MonoLoader(filename=request["audio"], sampleRate=44100)()
                    data = {profile: dict(zip(("key", "scale", "strength"), es.KeyExtractor(profileType=profile)(audio)))
                            for profile in ("krumhansl", "edma", "bgate")}
                elif name == "genre":
                    data = discogs_styles(request["audio"], cache, output)
                elif name == "edmformer":
                    import warnings,math,soundfile as sf
                    import edm98.inference.pipeline as edm_pipeline
                    from edm98.inference import create_pipeline
                    assets=Path.home()/".local/share/onset/experimental/edmformer-assets"
                    os.environ["MUSICFMPATH"]=str(assets/"musicfm")
                    duration=sf.info(request['audio']).duration
                    edm_pipeline.TIME_DUR,joins=edmformer_chunks(duration)
                    with warnings.catch_warnings(record=True) as observed:
                        prediction = create_pipeline(device=device, persistent_models=False,low_memory=True,hf_cache_dir=cache/"huggingface",checkpoint_path=assets/"data/checkpoints/model.pt",
                             config_path=assets/"configs/edmformer.yaml",musicfm_stat_path=assets/"data/checkpoints/msd_stats.json",
                             musicfm_model_path=assets/"data/checkpoints/pretrained_msd.pt").predict_file(request["audio"])
                    data = {"segments":prediction} if isinstance(prediction,list) else prediction if isinstance(prediction,dict) else vars(prediction)
                    if not data.get('segments'):raise ValueError('EDMFormer returned no structure segments')
                    for segment in data['segments']:
                        if not all(math.isfinite(segment[k]) for k in ('start','end')) or not 0<=segment['start']<segment['end']<=duration+.1:
                            raise ValueError('EDMFormer returned invalid segment timing')
                    data['warnings']=list(dict.fromkeys(str(w.message) for w in observed if issubclass(w.category,RuntimeWarning)))
                    data['chunk_joins']=joins
                    if joins:data['warnings'].append(f"Analysed in {len(joins)+1} chunks to fit in memory; labels just after {', '.join(f'{j:.0f}s' for j in joins)} are less reliable")
                elif name == "allin1":
                    import logging
                    logging.getLogger('natten.functional').setLevel(logging.ERROR)
                    import allin1
                    prediction = allin1.analyze(request["audio"], device=device,multiprocess=False,
                         demix_dir=output/'demix',spec_dir=output/'spec',include_activations=True, include_embeddings=True)
                    import numpy as np
                    embedding_path = output / "allin1-embeddings.npz"
                    np.savez_compressed(embedding_path, embeddings=prediction.embeddings)
                    activation_path=output/'allin1-activations.npz'
                    np.savez_compressed(activation_path,**prediction.activations)
                    data = {"segments": [vars(s) for s in prediction.segments], "embeddings": str(embedding_path),
                            "activations": {'path':str(activation_path),'shapes':{k:list(v.shape) for k,v in prediction.activations.items()}}}
                else:
                    raise ValueError("Unknown model")
            result["models"][name] = {"state": "complete", "worker_format":2,"experimental": name in {"edmformer", "allin1"},
                                      "data": data, "version": importlib.metadata.version({"beat_this": "beat-this", "edmformer": "edm98", "genre": "onnxruntime"}.get(name, name))}
            temporary = checkpoint.with_suffix(".tmp")
            temporary.write_text(json.dumps(result["models"][name],allow_nan=False, default=lambda x: x.tolist() if hasattr(x, "tolist") else str(x)))
            temporary.replace(checkpoint)
        except Exception as exc:
            result["models"][name] = {"state": "failed", "error": str(exc)[:600], "experimental": True}
        print(json.dumps({"type": "result", "result": result}, default=lambda x: x.tolist() if hasattr(x, "tolist") else str(x)), flush=True)
    import hashlib
    manifest_path = cache / "asset-manifest.json"
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
    for asset in cache.rglob("*"):
        if asset.is_file() and asset.suffix in {".pt", ".pth", ".th", ".ckpt", ".safetensors", ".pb", ".bin"}:
            key = str(asset.relative_to(cache)); stat = asset.stat()
            if manifest.get(key, {}).get("size") == stat.st_size and manifest[key].get("mtime") == stat.st_mtime:
                continue
            h = hashlib.sha256()
            with asset.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""): h.update(chunk)
            manifest[key] = {"sha256": h.hexdigest(), "size": stat.st_size, "mtime": stat.st_mtime}
    temporary_manifest=manifest_path.with_suffix('.tmp');temporary_manifest.write_text(json.dumps(manifest, indent=2));temporary_manifest.replace(manifest_path)
    import resource
    result['resources']={'peak_rss_bytes':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*1024,
                         'child_peak_rss_bytes':resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss*1024,'device':device}
    print(json.dumps({'type':'result','result':result},default=lambda x:x.tolist() if hasattr(x,'tolist') else str(x)),flush=True)


if __name__ == "__main__":
    main()
