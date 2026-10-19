"""Deterministic beam search and a validator shared by local and AI plans."""
from __future__ import annotations
from dataclasses import asdict
import numpy as np
from .models import SetBrief
from .transitions import options, validate_option, energy
from .util import digest


def validate_plan(plan, profiles, transition_map, brief=None):
    tracks = {t["id"]: t for t in profiles}
    if plan.get("schema_version") != 1:
        raise ValueError("Unsupported SetPlan schema")
    if plan.get("stale"): raise ValueError("Set is stale; revalidate against current analysis")
    ids = plan.get("track_ids", [])
    if not ids or len(set(ids)) != len(ids) or any(i not in tracks for i in ids):
        raise ValueError("Set has missing, duplicate or unknown tracks")
    if plan.get("revisions") and any(plan["revisions"].get(i) != tracks[i].get("revision") for i in ids):
        raise ValueError("Set references a changed analysis revision")
    links = plan.get("transition_ids", [])
    if len(links) != len(ids) - 1:
        raise ValueError("Each adjacent pair needs a transition")
    entries, rates, played = [0.], [1.], 0.
    for index, link in enumerate(links):
        option = transition_map.get(link)
        if not option or option["track_ids"] != ids[index:index + 2]:
            raise ValueError("Unknown transition or mismatched pair")
        validate_option(option, tracks)
        if option["out_start"] < entries[-1] or abs(option["out_rate"] - rates[-1]) > 1e-6:
            raise ValueError("Transition conflicts with previous entry or playback rate")
        played += (option["out_end"] - entries[-1]) / rates[-1] + option.get("loop_extra_seconds",0)
        entries.append(option["in_end"])
        rates.append(option["in_rate"])
    played += (tracks[ids[-1]]["duration"] - entries[-1]) / rates[-1]
    if brief:
        b = SetBrief.parse(brief) if isinstance(brief, dict) else brief
        if set(b.must_play) - set(ids) or set(ids) & set(b.excluded):
            raise ValueError("Set violates must-play/excluded constraints")
        if b.track_ids and set(ids) - set(b.track_ids):
            raise ValueError("Set contains a track outside the candidate collection")
        if b.start and ids[0] != b.start or b.finish and ids[-1] != b.finish:
            raise ValueError("Opening/closing preference violated")
        for pin in b.pinned:
            if pin.get("track_id") and (pin["position"] >= len(ids) or ids[pin["position"]] != pin["track_id"]):
                raise ValueError("Pinned track position violated")
            if pin.get("transition_id") and pin["transition_id"] not in links:
                raise ValueError("Pinned transition missing")
        for link in links:
            t = transition_map[link]
            if abs(t["pitch_pct"]) > b.max_pitch_pct + 1e-6 or t["skill"] > b.skill:
                raise ValueError("Transition violates pitch or skill limit")
            if t.get("key_lock",True) != b.key_lock: raise ValueError("Transition violates key-lock setting")
            if set(t["equipment"]) - set(b.equipment) or b.techniques and t["technique"] not in b.techniques:
                raise ValueError("Transition violates technique/equipment requirements")
        if abs(played - b.target_seconds) > b.tolerance_seconds:
            raise ValueError(f"Duration {played:.1f}s is outside requested tolerance")
    return {"valid": True, "duration_seconds": played, "entries": entries, "rates": rates}


def diagnose(brief, all_tracks, tracks):
    """Specific reasons a brief cannot be satisfied, checked before and after the search."""
    reasons = []
    for role, track_id in (("Opening", brief.start), ("Closing", brief.finish)):
        if track_id and track_id not in tracks:
            reasons.append(f"{role} track {all_tracks[track_id].get('title') or track_id} is excluded, unanalysed or outside the genre filter")
    for pin in brief.pinned:
        track_id = pin.get("track_id")
        if not track_id:
            continue
        if track_id not in tracks:
            reasons.append(f"Pinned track {track_id} is unknown or outside the filtered candidates")
        elif pin.get("position", 0) >= len(tracks):
            reasons.append(f"Pinned position {pin['position'] + 1} exceeds the {len(tracks)} available candidates")
    if brief.start and brief.start == brief.finish and len(tracks) > 1:
        reasons.append("Opening and closing tracks are the same; a set cannot repeat a track")
    longest = sum(t["duration"] for t in tracks.values()) / max(.01, 1 - brief.max_pitch_pct / 100)
    if tracks and longest < brief.target_seconds - brief.tolerance_seconds:
        reasons.append(f"All candidates together last at most {longest / 60:.1f} minutes, shorter than the requested minimum")
    return reasons


def build_plans(profiles, raw_brief):
    brief = SetBrief.parse(raw_brief)
    all_tracks = {t["id"]: t for t in profiles}
    wanted = set(brief.track_ids or all_tracks)
    unknown = (wanted | set(brief.must_play) | {i for i in (brief.start, brief.finish) if i}) - all_tracks.keys()
    if unknown:
        raise ValueError(f"Unknown brief track IDs: {sorted(unknown)}")
    tracks = {i: t for i, t in all_tracks.items() if i in wanted and i not in brief.excluded and t.get("duration", 0) > 0}
    if brief.genres:
        tracks = {i: t for i, t in tracks.items() if {g.casefold() for g in t.get("genres", [])} & {g.casefold() for g in brief.genres}}
    if set(brief.must_play) - tracks.keys():
        raise ValueError("Must-play tracks are unavailable or outside the filtered collection")
    starts = [brief.start] if brief.start else sorted(tracks)
    if not starts:
        return {"plans": [], "transitions": [], "conflicts": diagnose(brief, all_tracks, tracks) + ["No analysed candidate tracks"]}
    nodes = [{"ids": [i], "links": [], "entry": 0., "rate": 1., "elapsed": 0., "score": 0., "techniques": []} for i in starts if i in tracks]
    nodes = [n for n in nodes if all(not p.get("track_id") or p["position"] != 0 or p["track_id"] == n["ids"][0] for p in brief.pinned)]
    transitions, cache, finals = {}, {}, []
    def total(node):
        return node["elapsed"] + (tracks[node["ids"][-1]]["duration"] - node["entry"]) / node["rate"]
    def rank(node):
        missing = len(set(brief.must_play) - set(node["ids"]))
        return abs(total(node) - brief.target_seconds) + missing * 120 - node["score"] * 3
    for depth in range(len(tracks)):
        for node in nodes:
            if not set(brief.must_play) - set(node["ids"]) and (not brief.finish or node["ids"][-1] == brief.finish):
                finals.append(node)
        next_nodes = []
        for node in nodes:
            current = tracks[node["ids"][-1]]
            if brief.finish and current["id"] == brief.finish:
                continue
            if node["elapsed"] > brief.target_seconds + brief.tolerance_seconds:
                continue
            for candidate in tracks.values():
                if candidate["id"] in node["ids"]:
                    continue
                key = (current["id"], candidate["id"], round(node["rate"], 6))
                if key not in cache:
                    cache[key] = options(current, candidate, brief.json(), outgoing_rate=node["rate"])
                choices = cache[key]
                if any(o["out_start"] < node["entry"] for o in choices):
                    # The unrestricted top options include windows already played; search later windows instead.
                    late = key + (round(node["entry"], 3),)
                    if late not in cache:
                        cache[late] = options(current, candidate, brief.json(), outgoing_rate=node["rate"], min_out_start=node["entry"])
                    choices = cache[late]
                for option in choices:
                    if option["out_start"] < node["entry"]:
                        continue
                    transitions[option["id"]] = option
                    next_ids = node["ids"] + [candidate["id"]]
                    if any(p.get("track_id") and p["position"] == len(next_ids) - 1 and p["track_id"] != candidate["id"] for p in brief.pinned):
                        continue
                    elapsed = node["elapsed"] + (option["out_end"] - node["entry"]) / node["rate"] + option.get("loop_extra_seconds",0)
                    position = min(elapsed / brief.target_seconds, 1)
                    expected = float(np.interp(position, np.linspace(0, 1, len(brief.energy_arc)), brief.energy_arc))
                    rating = energy(candidate)
                    penalty = abs(rating - expected) * .2 if rating is not None else .2
                    penalty += brief.variety * .3 if option["technique"] in node["techniques"][-2:] else 0
                    familiarity = candidate.get("familiarity")
                    if familiarity is not None: penalty += .5 * abs(float(familiarity) - (1-brief.exploration))
                    mood_tags = candidate.get("mood_tags",[])
                    if brief.mood and mood_tags:
                        desired = set(brief.mood.casefold().split())
                        actual = set(" ".join(mood_tags).casefold().split())
                        penalty += .5 * (1-len(desired & actual)/max(1,len(desired)))
                    next_nodes.append({"ids": next_ids, "links": node["links"] + [option["id"]], "entry": option["in_end"],
                                       "rate": option["in_rate"], "elapsed": elapsed, "score": node["score"] + option["score"] - penalty,
                                       "techniques": node["techniques"] + [option["technique"]]})
        if not next_nodes:
            break
        next_nodes.sort(key=lambda n: (rank(n), n["ids"], n["links"]))
        nodes = next_nodes[:128]
    finals.sort(key=lambda n: (rank(n), n["ids"], n["links"]))
    plans, errors = [], []
    seen = set()
    for node in finals:
        signature = tuple(node["ids"])
        if signature in seen:
            continue
        p = {"schema_version": 1, "track_ids": node["ids"], "transition_ids": node["links"], "brief": brief.json(),
             "revisions": {i:tracks[i].get("revision") for i in node["ids"]},
             "duration_seconds": total(node), "score": node["score"], "stale": False,
             "explanation": "Balances valid transition options, requested duration and the energy arc.",
             "omitted": [i for i in tracks if i not in node["ids"]]}
        p["omission_reasons"] = {i:"Not selected by this alternative's duration, energy and transition tradeoffs" for i in p["omitted"]}
        p["preference_evidence"] = {"mood_known":sum(bool(tracks[i].get("mood_tags")) for i in node["ids"]),
                                    "familiarity_known":sum(tracks[i].get("familiarity") is not None for i in node["ids"])}
        p["id"] = digest(p)[:24]
        try:
            validate_plan(p, profiles, transitions, brief)
        except ValueError as exc:
            errors.append(str(exc))
            continue
        seen.add(signature)
        plans.append(p)
        if len(plans) == 3:
            break
    conflicts = [] if plans else diagnose(brief, all_tracks, tracks) + ["No valid set meets all constraints. Increase duration tolerance, allow more techniques, or change candidate tracks."]
    if errors and not plans:
        conflicts += list(dict.fromkeys(errors))[:3]
    return {"plans": plans, "transitions": list(transitions.values()), "conflicts": conflicts}
