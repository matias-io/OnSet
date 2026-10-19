"""Local-only rehearsal renders. Unsupported gestures remain manual auditions."""
from __future__ import annotations
import json
import subprocess
from pathlib import Path
import numpy as np
from .util import digest, atomic_json, file_hash
from .transitions import validate_option


def render(option, profiles, home, click=False):
    import soundfile as sf
    from scipy.signal import butter, sosfilt
    tracks = {p["id"]: p for p in profiles}
    validate_option(option, tracks)
    for tid in option['track_ids']:
        track=tracks[tid]
        if track.get('fingerprint') and file_hash(track['path']) != track['fingerprint']:
            raise ValueError('Audio changed; reimport and revalidate the transition before previewing')
    if not option.get("preview_supported"):
        raise ValueError("This performance gesture supports manual audition only")
    key = digest([option, click, "preview-v2"])
    output = Path(home) / "previews" / (key + ".wav")
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists() and output.with_suffix(".json").exists():
        return {**json.loads(output.with_suffix(".json").read_text()), "cached": True}
    sr = 44100
    overlap = option["wall_seconds"]
    before, after = 4., 8.
    def decode(track, start, length, rate):
        if length <= 0: return np.zeros((0, 2), dtype=np.float32)
        command = ["ffmpeg", "-v", "error", "-ss", str(max(0, start)), "-t", str(length * rate), "-i", track["path"]]
        if option.get("key_lock", True):
            command += ["-af", f"atempo={rate:.8f}"]
        else:
            command += ["-af", f"aresample={sr},asetrate={sr * rate:.6f},aresample={sr}"]
        command += ["-f", "f32le", "-ac", "2", "-ar", str(sr), "-"]
        raw = subprocess.run(command, check=True, capture_output=True).stdout
        return np.frombuffer(raw, dtype=np.float32).reshape(-1, 2)[:int(round(length*sr))].copy()
    a, b = [tracks[i] for i in option["track_ids"]]
    before = min(before, option["out_start"] / option["out_rate"])
    length = before + overlap + after
    size = int(round(length * sr))
    y = np.zeros((size, 2), dtype=np.float32)
    xa = decode(a, option["out_start"] - before * option["out_rate"], before + overlap, option["out_rate"])
    xb = decode(b, option["in_start"], overlap + after, option["in_rate"])
    start = int(round(before * sr))
    count = int(round(overlap * sr))
    loop=option.get("out_loop")
    if loop and loop["repetitions"]>1:
        clip=decode(a,loop["start"],(loop["end"]-loop["start"])/option["out_rate"],option["out_rate"])
        fade=min(220,len(clip)//4)
        if fade:
            clip[:fade]*=np.linspace(0,1,fade)[:,None]
            clip[-fade:]*=np.linspace(1,0,fade)[:,None]
        xa=np.concatenate([xa[:start],np.tile(clip,(loop["repetitions"],1))])[:start+count]
    xa = xa[:start+count]
    ramp_a, ramp_b = np.ones(len(xa)), np.ones(len(xb))
    technique = option["technique"]
    if count:
        n_a = max(0, min(count, len(xa) - start))
        n_b = min(count, len(xb))
        ramp_a[start:start + n_a] = np.linspace(1, 0, n_a)
        ramp_b[:n_b] = np.linspace(0, 1, n_b)
        if technique == "eq_swap":
            low_a = sosfilt(butter(3, 200, fs=sr, output="sos"), xa, axis=0)
            low_b = sosfilt(butter(3, 200, fs=sr, output="sos"), xb, axis=0)
            switch = count // 2
            xa[start + switch:] -= low_a[start + switch:]
            xb[:switch] -= low_b[:switch]
        elif technique == "filter":
            filtered = sosfilt(butter(3, 1000, btype="highpass", fs=sr, output="sos"), xa, axis=0)
            mix = 1 - ramp_a
            xa = xa * (1 - mix[:, None]) + filtered * mix[:, None]
    y[:min(size, len(xa))] += (xa * ramp_a[:, None])[:size]
    if technique in {"echo_out", "reverb_out"} and len(xa):
        beat = 60 / (option.get("target_bpm") or 120)
        capture = min(len(xa), int(sr * (beat if technique == "echo_out" else .3)))
        dry = np.zeros_like(y); dry[len(xa)-capture:len(xa)] = xa[-capture:]
        delays = tuple(beat*i for i in (1,2,3,4)) if technique == "echo_out" else (.037,.071,.113,.179,.263,.43,.67,1.)
        for i,delay in enumerate(delays):
            d = int(delay*sr)
            if d < size: y[d:] += dry[:-d] * (.55 ** (i+1))
    take = min(len(xb), size - start)
    y[start:start + take] += xb[:take] * ramp_b[:take, None]
    if click:
        source_start=option['out_start']-before*option['out_rate']
        beats_a=(a.get('analysis') or {}).get('beats',[])
        beats_b=(b.get('analysis') or {}).get('beats',[])
        tick_times=[(t-source_start)/option['out_rate'] for t in beats_a if source_start<=t<option['out_end']]
        if loop:
            tick_times=[t for t in tick_times if t<before]
            for repeat in range(loop['repetitions']):
                tick_times.extend(before+repeat*(loop['end']-loop['start'])/option['out_rate']+(t-loop['start'])/option['out_rate'] for t in beats_a if loop['start']<=t<loop['end'])
        tick_times.extend(before+(t-option['in_start'])/option['in_rate'] for t in beats_b if t>=option['in_end'])
        if not tick_times and option.get('target_bpm'):
            spacing=60/option['target_bpm'];tick_times=np.arange(before%spacing,length,spacing).tolist()
        tick = np.sin(2 * np.pi * 1500 * np.arange(441) / sr) * np.linspace(.15, 0, 441)
        for t in sorted(set(t for t in tick_times if 0<=t<length)):
            k = int(t * sr)
            n = min(len(tick), len(y) - k)
            y[k:k + n] += tick[:n, None]
    peak = float(np.max(np.abs(y)))
    gain = min(1., .95 / max(peak, 1e-9))
    temporary = output.with_suffix(".tmp.wav")
    sf.write(temporary, y * gain, sr, subtype="PCM_16")
    temporary.replace(output)
    record = {"schema_version": 1, "path": str(output), "transition_id": option["id"], "gain": gain,
              "duration_seconds": len(y) / sr, "actions": option["actions"], "transition_offset": before,
              "note": "Local rehearsal approximation; audition original tracks and equipment before performance"}
    atomic_json(output.with_suffix(".json"), record)
    return record
