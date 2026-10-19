from __future__ import annotations
import json
import os
import time
import shutil
from pathlib import Path
import numpy as np
from .models import validate_grid
from .util import digest, file_hash, atomic_json
from .tonal import estimate_key
from .structure import infer, scale
from .metrics import measure_resources

ALGORITHM = "native-analysis-3"
RESOLUTION_VERSION = "structure-resolution-3"


def cached_outputs_exist(result):
    return bool(result.get("features",{}).get("path")) and Path(result["features"]["path"]).is_file() and deep_outputs_exist(result.get("deep",{}))


def deep_outputs_exist(deep):
    paths=list(deep.get("stems",{}).values())
    for model in deep.get("models",{}).values():
        data=model.get("data",{})
        if isinstance(data,dict):
            if isinstance(data.get("embeddings"),str):paths.append(data["embeddings"])
            if isinstance(data.get("activations"),dict):paths.append(data["activations"].get("path"))
    return all(path and Path(path).is_file() for path in paths)


@measure_resources
def analyse(profile, home, deep=False, checkpoint=lambda *a: None, experimental=(), settings=None):
    checkpoint("loading audio tools", .01)
    import librosa
    import soundfile as sf
    from autocue_core import load_audio, find_sections, derive_cues
    start = time.monotonic()
    path = Path(profile["path"])
    checkpoint("verifying audio identity", .02)
    if file_hash(path) != profile["fingerprint"]:
        raise ValueError("Audio changed; reimport the file before analysis")
    previous = profile.get("analysis") or {}
    requested_grid = profile.get("overrides", {}).get("grid") or previous.get("downbeats")
    model_signature=None
    if deep:
        from .deep import worker_file
        model_signature=digest([file_hash(worker_file(name)) for name in ['deep_worker.py','requirements-cpu.lock']+
                                [f'requirements-{name}.lock' for name in experimental]])
    native_key = digest([profile["fingerprint"], ALGORITHM, requested_grid, previous.get("beats"),
                         profile.get("overrides", {}).get("bpm"), previous.get("bpm"),
                         previous.get("grid_source"), previous.get("phrase_anchor")])
    cache_key = digest([native_key, deep, list(experimental), model_signature])
    settings = settings or {}
    cache = Path(settings.get("cache_dir") or Path(home) / "features") / cache_key
    cached = cache / "result.json"
    if cached.exists():
        saved = json.loads(cached.read_text(encoding="utf-8"))
        if saved.get("state") == "complete" and saved.get('resolution_version') == RESOLUTION_VERSION and cached_outputs_exist(saved):
            checkpoint("cached", 1.)
            return saved
    cache.mkdir(parents=True, exist_ok=True)
    native_folder = cache.parent / "native-stages" / native_key
    native_folder.mkdir(parents=True, exist_ok=True)
    native_json, native_npz = native_folder / "stage.json", native_folder / "features.npz"
    native_hit = False
    try:
        stage = json.loads(native_json.read_text(encoding="utf-8"))
        if stage["key"] != native_key or file_hash(native_npz) != stage["sha256"]:
            raise ValueError("Native stage identity changed")
        pcm = native_folder / "audio.wav"
        if not pcm.is_file() or file_hash(pcm) != stage["pcm_sha256"]:
            raise ValueError("Native PCM is missing or changed")
        with np.load(native_npz, allow_pickle=False) as data:
            if any(not np.all(np.isfinite(data[k])) for k in data.files):
                raise ValueError("Native stage contains invalid feature values")
            duration,bpm,source,tuning = [stage[k] for k in ("duration","bpm","source","tuning")]
            downbeats,beats,detected_beats,times,chroma = [data[k].copy() for k in ("downbeats","beats","detected_beats","times","chroma")]
            f = {k:data["bar_"+k].tolist() for k in stage["bar_names"]}
            frame_features = {k:data["frame_"+k].copy() for k in stage["frame_names"]}
            onset = data["onset"].copy()
            waveform = data["waveform"].tolist()
            spectrogram = {"values":data["spectrogram"].tolist(),"frequencies":data["frequencies"].tolist(),"duration":duration}
        validate_grid(downbeats,duration)
        native_hit = True
        checkpoint("native features cached", .35)
    except (OSError,ValueError,KeyError,TypeError):
        pass
    if not native_hit:
        checkpoint("decode", .05)
        y, sr = load_audio(path)
        duration = len(y) / sr
        if duration < 2:
            raise ValueError("Track is too short for reliable analysis")
        cache.mkdir(parents=True, exist_ok=True)
        pcm = native_folder / "audio.wav"
        sf.write(pcm, y, sr, subtype="FLOAT")
        y = librosa.resample(y, orig_sr=sr, target_sr=22050) if sr != 22050 else y
        sr, hop = 22050, 512
        checkpoint("grid", .15)
        S = np.abs(librosa.stft(y, n_fft=2048, hop_length=hop))
        checkpoint("harmonic/percussive separation", .18)
        H, P = librosa.decompose.hpss(S)
        checkpoint("rhythm features", .2)
        onset = librosa.onset.onset_strength(S=librosa.amplitude_to_db(P + 1e-8), sr=sr, hop_length=hop)
        tempo, frames = librosa.beat.beat_track(onset_envelope=onset, sr=sr, hop_length=hop, trim=False)
        bpm = float(np.asarray(tempo).ravel()[0])
        detected_beats = librosa.frames_to_time(frames, sr=sr, hop_length=hop)
        grid = profile.get("overrides", {}).get("grid") or previous.get("downbeats")
        source = "manual" if profile.get("overrides", {}).get("grid") else previous.get("grid_source", "imported")
        if grid:
            downbeats = np.array(validate_grid(grid, duration))
            beats = np.asarray(previous.get("beats", detected_beats))
            bpm = float(profile.get("overrides", {}).get("bpm") or previous.get("bpm") or 240 / np.median(np.diff(downbeats)))
        else:
            if len(detected_beats) < 8:
                raise ValueError("Insufficient rhythmic evidence; provide or import a beat grid")
            beats = detected_beats
            downbeats = beats[::4]
            source = "librosa-dp-assumed-downbeat"
        downbeats = downbeats[downbeats < duration]
        validate_grid(downbeats, duration)
        times = librosa.frames_to_time(np.arange(S.shape[1]), sr=sr, hop_length=hop)
        frequency = librosa.fft_frequencies(sr=sr, n_fft=2048)
        power = S ** 2
        def band(lo, hi):
            return 10 * np.log10(np.mean(power[(frequency >= lo) & (frequency < hi)], axis=0) + 1e-10)
        rms = librosa.feature.rms(S=S)[0]
        chroma = librosa.feature.chroma_stft(S=H ** 2, sr=sr, tuning=None)
        tuning = float(librosa.estimate_tuning(S=H, sr=sr) * 100)
        mfcc = librosa.feature.mfcc(S=librosa.power_to_db(librosa.feature.melspectrogram(S=power, sr=sr)), n_mfcc=13)
        centroid = librosa.feature.spectral_centroid(S=S, sr=sr)[0]
        flatness = librosa.feature.spectral_flatness(S=S)[0]
        flux = np.r_[0, np.mean(np.maximum(np.diff(np.log1p(S), axis=1), 0), axis=0)]
        frame_features = {"full": 20 * np.log10(rms + 1e-9), "low": band(20, 150), "mid": band(150, 2000),
                          "high": band(2000, 10000), "onset": onset, "brightness": centroid, "flatness": flatness,
                          "flux": flux, "harmonic": np.mean(H, axis=0), "percussive": np.mean(P, axis=0)}
        low_mask = (frequency >= 20) & (frequency < 150)
        frame_features["low_transient"] = np.mean(P[low_mask], axis=0)
        frame_features["sustained_bass"] = np.mean(H[low_mask], axis=0)
        checkpoint("features", .35)
        edges = np.r_[downbeats, duration]
        f = {name: [] for name in frame_features}
        f.update(chroma=[], mfcc=[], rhythm_pattern=[])
        for a, b in zip(edges, edges[1:]):
            idx = np.where((times >= a) & (times < b))[0]
            if not len(idx):
                idx = np.array([min(len(times) - 1, int(a * sr / hop))])
            for name, values in frame_features.items():
                f[name].append(float(np.median(values[np.minimum(idx, len(values) - 1)])))
            f["chroma"].append(np.mean(chroma[:, idx], axis=1).tolist())
            f["mfcc"].append(np.mean(mfcc[:, idx], axis=1).tolist())
            bins = np.array_split(onset[np.minimum(idx, len(onset) - 1)], 16)
            f["rhythm_pattern"].append([float(np.max(x)) if len(x) else 0 for x in bins])
        waveform = [float(np.max(np.abs(x))) for x in np.array_split(y, min(1600, len(y)))]
        mel = librosa.power_to_db(librosa.feature.melspectrogram(S=power, sr=sr, n_mels=64), ref=np.max)
        stride = max(1, mel.shape[1] // 700)
        spectrogram = {"values": mel[:, ::stride].round(1).tolist(),
                       "frequencies": librosa.mel_frequencies(n_mels=64, fmax=sr / 2).tolist(), "duration": duration}
        temporary_native = native_npz.with_suffix(".tmp")
        with temporary_native.open("wb") as stream:
            np.savez_compressed(stream,downbeats=downbeats,beats=beats,detected_beats=detected_beats,
                times=times,chroma=chroma,onset=onset,waveform=waveform,spectrogram=spectrogram["values"],
                frequencies=spectrogram["frequencies"],**{"bar_"+k:np.asarray(v) for k,v in f.items()},
                **{"frame_"+k:v for k,v in frame_features.items()})
            stream.flush();os.fsync(stream.fileno())
        os.replace(temporary_native,native_npz)
        atomic_json(native_json,{"schema_version":1,"key":native_key,"sha256":file_hash(native_npz),
            "pcm_sha256":file_hash(pcm),"duration":duration,"bpm":bpm,"source":source,"tuning":tuning,
            "bar_names":list(f),"frame_names":list(frame_features)})
    # Keep deep model outputs in their versioned result folder, independently of
    # the shared native feature stage. Copying also avoids hard-link cache damage.
    model_pcm = cache / "audio.wav"
    if deep and (not model_pcm.exists() or file_hash(model_pcm)!=file_hash(pcm)):
        shutil.copyfile(pcm,model_pcm)
    edges = np.r_[downbeats, duration]
    deep_results = {}
    if deep:
        checkpoint("stems/models", .5)
        # Completed deep outputs are reused when only the resolution stage changes,
        # so a structure/version update does not rerun hours of model inference.
        deep_cache = cache / "deep-results.json"
        if deep_cache.exists():
            try:
                saved_deep = json.loads(deep_cache.read_text(encoding="utf-8"))
                if saved_deep.get("pcm_sha256") == file_hash(model_pcm) and deep_outputs_exist(saved_deep["result"]):
                    deep_results = saved_deep["result"]
            except (OSError, ValueError, KeyError):
                deep_results = {}
        elif cached.exists():
            # Results written before deep-results.json existed still carry complete model outputs.
            try:
                older = json.loads(cached.read_text(encoding="utf-8")).get("deep") or {}
                if older.get("models") and all(v.get("state") == "complete" for v in older["models"].values()) and deep_outputs_exist(older):
                    deep_results = older
            except (OSError, ValueError):
                deep_results = {}
        if not deep_results:
            from .deep import run_deep
            deep_results = run_deep(model_pcm, home, checkpoint, experimental, settings.get("model_dir"))
            if deep_results.get("models") and all(v.get("state") == "complete" for v in deep_results["models"].values()):
                atomic_json(deep_cache, {"schema_version": 1, "pcm_sha256": file_hash(model_pcm), "result": deep_results})
        stem_paths = deep_results.get("stems", {})
        if "vocals" in stem_paths:
            vocals, vsr = sf.read(stem_paths["vocals"], always_2d=True)
            vrms = []
            for a, b in zip(edges, edges[1:]):
                seg = vocals[int(a * vsr):int(b * vsr)]
                vrms.append(float(np.sqrt(np.mean(seg ** 2))) if len(seg) else 0)
            original_rms = np.power(10,np.asarray(f["full"])/20)
            f["vocal"] = np.clip(np.asarray(vrms) / np.maximum(original_rms,1e-8), 0, 1).tolist()
    checkpoint("structure", .7)
    structure = infer(f, downbeats, duration)
    harmonic_density=np.asarray(f["harmonic"])/np.maximum(np.asarray(f["harmonic"])+np.asarray(f["percussive"]),1e-8)
    bass_activity=scale(f["low"])
    density=.5*scale(f["onset"])+.5*scale(f["flux"])
    for window in structure["windows"]+structure.get("loop_candidates",[])+structure.get("mix_windows",[]):
        start_bar,end_bar=window["start_bar"],window["end_bar"]
        window.update(tonal_density=float(np.mean(harmonic_density[start_bar:end_bar])),
                      bass_activity=float(np.mean(bass_activity[start_bar:end_bar])),
                      arrangement_density=float(np.mean(density[start_bar:end_bar])),
                      fakeout_risk=end_bar-start_bar<4)
    recurrence = structure.pop("recurrence")
    feature_path = cache / (digest([downbeats.tolist(), deep_results]) + ".npz")
    temporary = str(feature_path) + ".tmp"
    with open(temporary, "wb") as stream:
        np.savez_compressed(stream, **{k: np.array(v) for k, v in f.items()},
                            **{"recurrence_" + k: v for k, v in recurrence.items()},
                            frame_times=times, chroma_frames=chroma, **{"frame_" + k: v for k, v in frame_features.items()})
    os.replace(temporary, feature_path)
    combined = .65 * np.array(f["low"]) + .35 * np.array(f["full"])
    silent_track=bool(np.all(np.asarray(frame_features['full']) < -90))
    baseline = ({'drop':None,'breakdown':None,'second_drop':None,'outro':None,
                 'flags':['No audible signal; musical events and mix cues absent']} if silent_track else
                find_sections(combined, f["high"], phrase_anchor=previous.get("phrase_anchor")))
    checkpoint("tonality/energy", .85)
    key = estimate_key(chroma, tuning)
    key_observations = [key]
    for name, value in deep_results.get("models", {}).get("essentia", {}).get("data", {}).items():
        key_observations.append({"source": "essentia:" + name, **value})
    independent = deep_results.get("models", {}).get("beat_this", {}).get("data")
    local = []
    for section in structure["sections"]:
        idx = (times >= section["start"]) & (times < section["end"])
        local.append({"start": section["start"], "end": section["end"], **estimate_key(chroma[:, idx], tuning)})
    # Raw dimensions retained; library calibration is a separate explicit operation.
    drive = .6 * scale(f["onset"]) + .4 * scale(f["percussive"])
    density = .5 * scale(f["flux"]) + .5 * scale(f["high"])
    intensity = .5 * scale(f["full"]) + .3 * scale(f["low"]) + .2 * scale(f["brightness"])
    raw = .4 * drive + .25 * density + .35 * intensity
    energy = {"source": "interpretable-v1", "confidence": "unverified", "calibration": None, "rating": None,
              "relative_curve": (1 + 9 * raw).round(2).tolist(), "drive": drive.tolist(), "density": density.tolist(),
              "intensity": intensity.tolist(), "tension": scale(np.r_[0, np.diff(density + drive)]).tolist(),
              "contrast": scale(np.abs(np.r_[0, np.diff(intensity)])).tolist(),
              "raw_summary": float(np.median(raw)), "note": "Curve is relative within this track; overall rating requires library calibration"}
    energy["acoustic_summary"] = [float(np.median(f["full"])), float(np.median(f["low"])),
        float(np.median(f["brightness"])), float(np.mean(onset > np.percentile(onset, 75)) * bpm), float(np.median(f["percussive"]))]
    beat_diff = np.diff(beats)
    cv = float(np.std(beat_diff) / np.mean(beat_diff)) if len(beat_diff) else None
    offsets = [float(np.min(np.abs(detected_beats - t))) for t in downbeats] if len(detected_beats) else []
    grid_observation = {"source": source, "tempo_hypotheses": [bpm / 2, bpm, bpm * 2], "tempo_cv": cv,
                        "median_offset_seconds": float(np.median(offsets)) if offsets else None,
                        "downbeat_confidence": "unknown" if source.startswith("librosa") else "unverified",
                        "independent_beats": detected_beats.tolist(), "phrase_lengths": [4, 8, 16, 32]}
    if independent:
        grid_observation["beat_this"] = independent
        independent_db=np.asarray(independent.get("downbeats",[]))
        if len(independent_db):
            grid_observation["downbeat_offset_seconds"] = float(np.median([np.min(abs(independent_db-t)) for t in downbeats]))
    grid_observation["local_bpm"] = (60 / np.maximum(np.diff(detected_beats), 1e-6)).round(2).tolist()
    event_names = {"drop": "first_full_groove", "breakdown": "first_breakdown", "second_drop": "later_drop", "outro": "outro"}
    events = [{"kind": event_names[k], "state": "absent" if silent_track else "estimated" if baseline[k] is not None else "unknown",
               "time": float(downbeats[baseline[k]]) if baseline[k] is not None else None,
               "confidence": "unverified", "source": "energy-baseline", "timing_uncertainty_bars": 2}
              for k in event_names]
    major = sorted(structure["boundaries"], key=lambda b: -b["strength"])
    events += [{"kind": "major_payoff_candidate", "state": "estimated", "time": b["time"], "source": b["source"],
                "confidence": "unverified"} for b in major[:3]]
    result = {"schema_version": 1, "algorithm": ALGORITHM, "resolution_version":RESOLUTION_VERSION,"state": "complete", "path": str(path),
              "duration": duration, "bpm": bpm, "bars": len(downbeats), "analysis_mode": "deep" if deep else "light", "cache_hit": native_hit, "downbeats": downbeats.tolist(),
              "beats": beats.tolist(), "phrase_anchor": previous.get("phrase_anchor"), "grid_source": source,
              "grid_observation": grid_observation, "structure": structure, "events": events,
              "sections": {k: baseline[k] for k in event_names}, "flags": baseline["flags"],
              "energy": combined.tolist(), "energy_hi": f["high"], "energy_profile": energy,
              "vocal_activity": f.get("vocal"),
              "key": key, "key_observations": key_observations, "local_keys": local, "deep": deep_results,
              "features": {"path": str(feature_path), "version": ALGORITHM}, "waveform": waveform,
              "spectrogram": spectrogram,
              "seconds": time.monotonic() - start}
    result["cues"] = derive_cues(result["sections"], downbeats, second_drop=True, end_loop_bars=8,
                                 intro_loop_bars=8, duration=duration)
    if silent_track:result['cues']=[]
    required_models = {"beat_this", "demucs", "essentia", *experimental}
    if deep and (any(deep_results.get("models",{}).get(name,{}).get("state")!="complete" for name in required_models)
                 or any(v.get("state")!="complete" for v in deep_results.get("models",{}).values())):
        result["state"] = "partial"
    atomic_json(cache / "result.json", result)
    checkpoint("complete", 1.)
    return result
