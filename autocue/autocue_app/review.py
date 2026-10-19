"""Review queue ordering: where a DJ's listening time changes the most decisions.

Priorities are transparent heuristics with stated reasons, not probabilities."""
from __future__ import annotations
from .evidence import resolve

# Cue D is written as a hot cue; the mix-in/out cues feed transition planning.
HIGH_IMPACT_ROLES = {"D": 2, "A": 1, "E": 1, "F": 1}


def priority(track, reliability=None, set_count=0):
    reasons, score = [], 0.
    if track.get("analysis_state") == "missing":
        return {"track_id": track["id"], "score": 0., "pending_cues": 0, "reviewable": False,
                "reasons": ["Audio file is missing; relocate it before reviewing"]}
    cues = track.get("cues", [])
    pending = [c for c in cues if c.get("review") in {"proposed", "stale"}]
    stale = [c for c in pending if c.get("review") == "stale"]
    if stale:
        score += 2 * len(stale)
        reasons.append(f"{len(stale)} cue(s) need re-review after the analysis or grid changed")
    impact = sum(HIGH_IMPACT_ROLES.get(c["role"], 0) for c in pending)
    if impact:
        score += impact
        reasons.append("Unapproved hot cue D or mix-in/out cues")
    resolved = resolve(track.get("observations", []), reliability)
    for event, value in sorted(resolved.items()):
        if value.get("meaning_uncertain"):
            score += 3
            reasons.append(f"Sources disagree whether {event.replace('_', ' ')} exists")
        elif value.get("timing_uncertainty_bars", 0) > 1:
            score += 2
            reasons.append(f"Sources place {event.replace('_', ' ')} {value['timing_uncertainty_bars']} bars apart")
    analysis = track.get("analysis") or {}
    flags = analysis.get("flags") or []
    if flags:
        score += 1.5 * len(flags)
        reasons.extend(str(f) for f in flags[:3])
    grid = analysis.get("grid_observation") or {}
    if (grid.get("tempo_cv") or 0) > .03:
        score += 2
        reasons.append("Variable tempo; check the beat grid")
    if (grid.get("downbeat_offset_seconds") or 0) > .05:
        score += 2
        reasons.append("Independent downbeats disagree with the grid")
    key = track.get("key")
    if isinstance(key, dict) and key.get("state") in {"weak", "unknown"} or key in (None, ""):
        score += .5
        reasons.append("Key is weak or unknown")
    if track.get("analysis_state") in {"partial", "imported"}:
        score += .5
        reasons.append("Only partial or historical imported analysis")
    if set_count and pending:
        score += min(set_count, 3)
        reasons.append(f"Used in {set_count} saved set(s)")
    if not pending:
        score *= .25
    return {"track_id": track["id"], "title": track.get("title"), "score": round(score, 2),
            "pending_cues": len(pending), "reviewable": bool(analysis), "reasons": reasons}


def queue(tracks, reliability=None, sets=(), include_done=False):
    usage = {}
    for plan in sets:
        if not plan.get("stale"):
            for track_id in plan.get("track_ids", []):
                usage[track_id] = usage.get(track_id, 0) + 1
    rows = [priority(t, reliability, usage.get(t["id"], 0)) for t in tracks]
    if not include_done:
        rows = [r for r in rows if r["reviewable"] and r["pending_cues"]]
    return sorted(rows, key=lambda r: (-r["score"], str(r.get("title") or ""), r["track_id"]))
