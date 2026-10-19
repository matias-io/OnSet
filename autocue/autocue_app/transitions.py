from __future__ import annotations
import math
import numpy as np
from .catalogue import CATALOGUE
from .tonal import compatibility, sounding_key
from .util import digest


def beat_times(track):
    a = track.get("analysis") or {}
    db = track.get("overrides", {}).get("grid") or a.get("downbeats", [])
    return np.asarray(db, dtype=float)


def windows(track, outgoing):
    a = track.get("analysis") or {}
    candidates = list(a.get("structure", {}).get("windows", []))
    corrected = track.get("overrides", {}).get("sections")
    if corrected:
        candidates = [{**s, "id": f"manual-section-{i}", "vocals": s.get("vocals", "unknown")}
                      for i, s in enumerate(corrected)]
    if outgoing:
        candidates += [{**w,"label":"loop","loop":True} for w in a.get("structure",{}).get("loop_candidates",[])]
    else:
        candidates += [{"id":f"hook-{h['time']}","start":h["time"],"end":track["duration"],"label":"hook","vocals":"unknown"}
                       for h in track.get("hooks",[]) if h.get("verified")]
    db = beat_times(track)
    duration = track.get("duration", 0)
    for c in track.get("cues", []):
        if c.get("review") == "rejected":
            continue
        if c["role"] in (("F", "E", "G") if outgoing else ("A", "B", "C", "D")):
            t = c["time"]
            candidates.append({"id": c["id"], "start": t, "end": duration, "label": c["role"], "vocals": "unknown"})
    if not candidates and len(db):
        t = float(db[max(0, len(db) - 17)]) if outgoing else float(db[0])
        candidates.append({"id": "grid-fallback", "start": t, "end": duration, "label": "unknown", "vocals": "unknown"})
    ordered = sorted(candidates, key=lambda w: (w["start"], w["id"]), reverse=outgoing)
    natural = [w for w in ordered if not w.get("loop")]
    loops = [w for w in ordered if w.get("loop")]
    return natural[:6] + loops[:2] if loops else natural[:8]


def local_key(track, position):
    if "key" in track.get("overrides",{}):return track.get("key")
    for key in (track.get("analysis") or {}).get("local_keys", []):
        if key["start"] <= position < key["end"]:
            return key
    return track.get("key")


def overlap_harmony(a,b,start_a,end_a,start_b,end_b,rate_a,rate_b,key_lock):
    boundaries=[0.,1.]
    for track,start,end in ((a,start_a,end_a),(b,start_b,end_b)):
        if end>start:
            for key in (track.get("analysis") or {}).get("local_keys",[]):
                if start<key["start"]<end:boundaries.append((key["start"]-start)/(end-start))
    weighted=weight=0.
    for left,right in zip(sorted(set(boundaries)),sorted(set(boundaries))[1:]):
        fraction=(left+right)/2
        ka=sounding_key(local_key(a,start_a+fraction*(end_a-start_a)),rate_a,key_lock)
        kb=sounding_key(local_key(b,start_b+fraction*(end_b-start_b)),rate_b,key_lock)
        score=compatibility(ka,kb)
        if score is not None:weighted+=score*(right-left);weight+=right-left
    return weighted/weight if weight else None


def energy(track, position=0):
    if track.get("energy") is not None:
        return float(track["energy"])
    return (track.get("analysis") or {}).get("energy_profile", {}).get("rating")


def options(a, b, brief=None, limit=3, outgoing_rate=1., min_out_start=0.):
    brief = brief or {}
    if a["id"] == b["id"] or not a.get("duration") or not b.get("duration"):
        return []
    bpm_a, bpm_b = a.get("bpm"), b.get("bpm")
    pitch_limit = brief.get("max_pitch_pct", 6)
    target = bpm_a * outgoing_rate if bpm_a else None
    results = []
    db_a, db_b = beat_times(a), beat_times(b)
    outgoing_windows, incoming_windows = windows(a, True), windows(b, False)
    key_cache = {}
    for technique in CATALOGUE:
        if brief.get("techniques") and technique["id"] not in brief["techniques"]:
            continue
        if technique["skill"] > brief.get("skill", 2) or set(technique["equipment"]) - set(brief.get("equipment", ["eq", "filter", "loop", "echo", "reverb"])):
            continue
        if technique["id"] == "tease" and not any(h.get("verified") for h in b.get("hooks",[])):
            continue
        if technique["id"] == "wordplay" and not set(a.get("themes",[])) & set(b.get("themes",[])):
            continue
        if technique["id"] == "stem_blend" and not (a.get("analysis") or {}).get("deep",{}).get("stems",{}).get("vocals"):
            continue
        beatmatched = technique["beatmatched"] or technique["id"] == "tempo_bridge"
        factor = 1.
        if technique["id"] == "tempo_bridge":
            if not target or not bpm_b:continue
            factor = min((.5,2.),key=lambda f:abs(target/(bpm_b*f)-1))
        rate = target / (bpm_b*factor) if beatmatched and target and bpm_b else 1.
        if beatmatched and (not target or not bpm_b or abs(rate - 1) * 100 > pitch_limit):
            continue
        best = None
        for out in outgoing_windows:
            if out["start"] < min_out_start:
                continue
            for incoming in incoming_windows:
                t_a, t_b = out["start"], incoming["start"]
                if t_a >= a["duration"] or t_b >= b["duration"]:
                    continue
                bars = technique["bars"]
                loop_extra=0.;loop=None
                if out.get("loop") and technique["id"] not in {"loop_blend","loop_roll"}:continue
                if technique["id"] == "tease" and incoming["label"] != "hook":continue
                if technique["id"] == "drop_swap" and (incoming["label"] != "drop" or incoming.get("fakeout_risk")):
                    continue
                if technique["id"] == "double_drop" and (out["label"] != "drop" or incoming["label"] != "drop" or out.get("fakeout_risk") or incoming.get("fakeout_risk")):
                    continue
                if technique["id"] == "breakdown" and out["label"] not in {"breakdown", "E"}:
                    continue
                if bars and len(db_a) and len(db_b):
                    ia = int(np.argmin(abs(db_a - t_a)))
                    ib = int(np.argmin(abs(db_b - t_b)))
                    incoming_bars = int(round(bars/factor))
                    if ib + incoming_bars >= len(db_b):
                        continue
                    end_b = float(db_b[ib + incoming_bars])
                    if out.get("loop"):
                        end_a=out["end"]
                        loop_duration=(end_a-t_a)/outgoing_rate
                        if loop_duration<=0:continue
                        repetitions=int(round(((end_b-t_b)/rate)/loop_duration))
                        if repetitions<1:continue
                        loop_extra=(repetitions-1)*loop_duration
                        loop={"start":t_a,"end":end_a,"repetitions":repetitions,"quality":out.get("quality","unverified")}
                    else:
                        if ia + bars >= len(db_a):continue
                        end_a=float(db_a[ia+bars])
                    wall = (end_a - t_a) / outgoing_rate
                    wall += loop_extra
                    if beatmatched and abs((end_b - t_b) / rate - wall) > max(.1, wall * .01):
                        continue
                    if not beatmatched:
                        wall = min(wall, 4)
                        end_a, end_b = t_a + wall * outgoing_rate, t_b + wall * rate
                elif bars:
                    wall = min(4, (a["duration"] - t_a) / outgoing_rate, b["duration"] - t_b)
                    end_a, end_b = t_a + wall * outgoing_rate, t_b + wall * rate
                else:
                    wall, end_a, end_b = 0., t_a, t_b
                if end_a > a["duration"] or end_b > b["duration"] or wall < 0:
                    continue
                if bars and (end_a > out["end"] + .05 or end_b > incoming["end"] + .05):
                    continue
                def key_for(track, position, playback_rate):
                    cache_key=(track["id"],position,playback_rate)
                    if cache_key not in key_cache:key_cache[cache_key]=sounding_key(local_key(track, position),playback_rate,brief.get("key_lock",True))
                    return key_cache[cache_key]
                ka = key_for(a,t_a,outgoing_rate)
                kb = key_for(b,t_b,rate)
                harmonic = compatibility(ka, kb)
                if wall>8 and ((a.get("analysis") or {}).get("local_keys") or (b.get("analysis") or {}).get("local_keys")):
                    harmonic=overlap_harmony(a,b,t_a,end_a,t_b,end_b,outgoing_rate,rate,brief.get("key_lock",True))
                warnings = []
                if harmonic is None:
                    warnings.append("Key unknown or weak in overlap")
                if "unknown" in (out.get("vocals"), incoming.get("vocals")):
                    warnings.append("Vocal overlap unverified")
                if out.get("vocals") is True and incoming.get("vocals") is True and wall > 4:
                    continue
                tonal_density=min(out.get("tonal_density",.7),incoming.get("tonal_density",.7))
                if technique["beatmatched"] and harmonic is not None and harmonic < .5 and wall > 8 and tonal_density>.25:
                    continue
                if harmonic is not None and harmonic<.5 and wall>8:warnings.append("Low tonal-density overlap permits this option; audition residual harmony")
                bass_overlap=min(out.get("bass_activity",0),incoming.get("bass_activity",0))
                if bass_overlap>.6 and wall>4 and technique["id"] not in {"eq_swap","filter","stem_blend"}:warnings.append("Both windows contain sustained bass; plan a bass handover")
                if technique["id"] in {"drop_swap","double_drop"}:warnings.append("Payoff timing is estimated; verify the drop and any fake-out by listening")
                if abs(rate - 1) > .03:
                    warnings.append("Audition time-stretch quality")
                if loop: warnings.append("Loop extension: audition the seam and exit")
                for tr in (a, b):
                    grid = (tr.get("analysis") or {}).get("grid_observation", {})
                    if grid.get("tempo_cv", 0) and grid["tempo_cv"] > .03:
                        warnings.append("Variable tempo: manual alignment required")
                score = (harmonic if harmonic is not None else .4) * (2 if wall > 8 else .3) + 2 * (1 - min(abs(rate - 1) / .06, 1))
                score += .4 if technique["id"] == "eq_swap" else 0
                score -= .25 * len(warnings)
                ea, eb = energy(a, t_a), energy(b, t_b)
                if ea is not None and eb is not None:
                    score -= abs(eb - ea) * .1
                actions = [{"at": 0, "action": f"Start incoming at {t_b:.3f}s; outgoing at {t_a:.3f}s"}]
                if wall:
                    handovers={"eq_swap":"Exchange bass: remove outgoing lows as incoming lows enter",
                        "filter":"Gradually high-pass the outgoing track while opening the incoming track",
                        "echo_out":"Catch the outgoing phrase in tempo-synced echo, then remove the dry signal",
                        "reverb_out":"Apply reverb to the outgoing phrase and fade its dry signal",
                        "stem_blend":"Keep the outgoing vocal stem over the incoming instrumental; remove the outgoing drums/bass",
                        "tease":"Introduce the verified hook in short phrases before completing the handover",
                        "loop_roll":"Shorten the outgoing loop in beat-aligned steps; release at the next phrase",
                        "tempo_bridge":f"Align the pulses using the {factor:g}× tempo relationship",
                        "double_drop":"Align both drops and manage bass with EQ to avoid two full basslines"}
                    actions += [{"at": wall / 2, "action": handovers.get(technique["id"],"Gradually exchange levels while preserving phrase alignment")},
                                {"at": wall, "action": "Remove outgoing track"}]
                else:
                    actions += [{"at": 0, "action": "Switch at the phrase boundary; check any vocal pickup"}]
                if loop:actions.insert(1,{"at":0,"action":f"Repeat the outgoing loop {loop['repetitions']} times, then exit at the handover"})
                if technique["id"] == "backspin":actions[-1]["action"]="Perform the backspin/brake before the incoming downbeat; rehearse manually"
                if technique["id"] == "wordplay":actions[-1]["action"]="Connect the verified shared theme at these phrase boundaries; audition the actual words or hook"
                option = {"schema_version": 1, "track_ids": [a["id"], b["id"]], "revisions": [a.get("revision"), b.get("revision")],
                          "technique": technique["id"], "name": technique["name"], "out_window": out["id"], "in_window": incoming["id"],
                          "out_start": t_a, "out_end": end_a, "in_start": t_b, "in_end": end_b,
                          "out_rate": outgoing_rate, "in_rate": rate, "pitch_pct": (rate - 1) * 100,
                          "tempo_factor":factor,"out_loop":loop,"loop_extra_seconds":loop_extra,
                          "target_bpm": target if beatmatched else bpm_b, "wall_seconds": wall,
                          "key_lock": brief.get("key_lock", True), "actions": actions, "warnings": sorted(set(warnings)),
                          "score": round(score, 3), "confidence": "unverified", "equipment": technique["equipment"],
                          "skill": technique["skill"], "preview_supported": technique["preview"], "stale": False,
                          "reason": f"{technique['name']}: {bars} bars; tonal compatibility " + ("unknown" if harmonic is None else f"{harmonic:.2f}")}
                option["algorithm"] = "transition-2"
                if best is None or (-option["score"], -option["out_start"],option["in_start"], option["out_window"], option["in_window"]) < (-best["score"],-best["out_start"],best["in_start"], best["out_window"], best["in_window"]):
                    best = option
        if best is not None: results.append(best)
    results.sort(key=lambda o: (-o["score"], o["technique"], o["out_window"], o["in_window"]))
    for result in results[:limit]: result["id"] = digest(result)[:24]
    return results[:limit]


def validate_option(option, tracks):
    a, b = [tracks[x] for x in option["track_ids"]]
    if option.get("stale") or option["revisions"] != [a.get("revision"), b.get("revision")]:
        raise ValueError("Transition references stale analysis")
    for start, end, tr in ((option["out_start"], option["out_end"], a), (option["in_start"], option["in_end"], b)):
        if not math.isfinite(start) or not start <= end <= tr["duration"] or start < 0:
            raise ValueError("Transition outside audio")
    for side in ("out", "in"):
        rate = option[side + "_rate"]
        if not math.isfinite(rate) or rate <= 0:
            raise ValueError("Invalid playback rate")
    wall = (option["out_end"] - option["out_start"]) / option["out_rate"]
    wall += option.get("loop_extra_seconds",0)
    if abs(wall - option["wall_seconds"]) > .05:
        raise ValueError("Transition timing mismatch")
    incoming_wall=(option["in_end"]-option["in_start"])/option["in_rate"]
    if abs(incoming_wall-option["wall_seconds"])>max(.1,option["wall_seconds"]*.01):
        raise ValueError("Incoming timing mismatch")
    return True
