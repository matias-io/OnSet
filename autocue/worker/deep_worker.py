"""Optional isolated Linux inference process; stdin request / JSONL events."""
import contextlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys


def main():
    if not os.environ.get("AUTOCUE_WORKER_CHILD"):
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
                    interpreter=Path.home()/".local/share/autocue/experimental"/name/"bin/python"
                    if not interpreter.is_file():raise RuntimeError(f"Set up the isolated {name} runtime before requesting this experimental source")
                    child={**request,"models":[name],"isolated":True}
                    completed=subprocess.run([str(interpreter),__file__],input=json.dumps(child)+'\n',capture_output=True,text=True,
                                             env={**os.environ,"AUTOCUE_WORKER_CHILD":"1"})
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
                    import subprocess
                    subprocess.run([sys.executable, "-m", "demucs", "-n", "htdemucs", "-d", device,
                                    "--out", str(output), request["audio"]], check=True, stdout=sys.stderr)
                    stems = output / "htdemucs" / Path(request["audio"]).stem
                    result["stems"] = {p.stem: str(p) for p in stems.glob("*.wav")}
                    data = {"stems": result["stems"]}
                elif name == "essentia":
                    import essentia.standard as es
                    audio = es.MonoLoader(filename=request["audio"], sampleRate=44100)()
                    data = {profile: dict(zip(("key", "scale", "strength"), es.KeyExtractor(profileType=profile)(audio)))
                            for profile in ("krumhansl", "edma", "bgate")}
                elif name == "edmformer":
                    import warnings,math,soundfile as sf
                    from edm98.inference import create_pipeline
                    assets=Path.home()/".local/share/autocue/experimental/edmformer-assets"
                    os.environ["MUSICFMPATH"]=str(assets/"musicfm")
                    with warnings.catch_warnings(record=True) as observed:
                        prediction = create_pipeline(device=device, persistent_models=False,low_memory=True,hf_cache_dir=cache/"huggingface",checkpoint_path=assets/"data/checkpoints/model.pt",
                             config_path=assets/"configs/edmformer.yaml",musicfm_stat_path=assets/"data/checkpoints/msd_stats.json",
                             musicfm_model_path=assets/"data/checkpoints/pretrained_msd.pt").predict_file(request["audio"])
                    data = {"segments":prediction} if isinstance(prediction,list) else prediction if isinstance(prediction,dict) else vars(prediction)
                    duration=sf.info(request['audio']).duration
                    if not data.get('segments'):raise ValueError('EDMFormer returned no structure segments')
                    for segment in data['segments']:
                        if not all(math.isfinite(segment[k]) for k in ('start','end')) or not 0<=segment['start']<segment['end']<=duration+.1:
                            raise ValueError('EDMFormer returned invalid segment timing')
                    data['warnings']=list(dict.fromkeys(str(w.message) for w in observed if issubclass(w.category,RuntimeWarning)))
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
                                      "data": data, "version": importlib.metadata.version({"beat_this": "beat-this", "edmformer": "edm98"}.get(name, name))}
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
