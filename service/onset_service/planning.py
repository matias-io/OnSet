"""Deterministic beam search and a validator shared by local and AI plans."""
from __future__ import annotations
from dataclasses import asdict
import numpy as np
from .models import SetBrief
from .transitions import options, validate_option, energy
from .util import digest

# Techniques that change the sound abruptly rather than overlapping phrases.
ABRUPT = {"cut", "drop_swap", "double_drop", "backspin", "echo_out", "reverb_out", "fade", "tempo_bridge"}


def smoothness_value(option, a, b):
    """+1 for a long, level overlap; -1 for an abrupt change with a large energy jump."""
    abrupt = option["technique"] in ABRUPT
    value = -.5 if abrupt else .5 * min(option["wall_seconds"] / 30, 1)
    ea, eb = energy(a), energy(b)
    if ea is not None and eb is not None:
        value -= .5 * min(abs(eb - ea) / 3, 1)
    return value


def walk(ids, links, transition_map, tracks):
    """Entry position, rate and elapsed performance time at the end of an existing route."""
    entry, rate, elapsed = 0., 1., 0.
    for index, link in enumerate(links):
        option = transition_map.get(link)
        if not option or option["track_ids"] != ids[index:index + 2]:
            raise ValueError("Kept part of the set references an unknown or mismatched transition")
        validate_option(option, tracks)
        if option["out_start"] < entry or abs(option["out_rate"] - rate) > 1e-6:
            raise ValueError("Kept part of the set has conflicting entry points or rates")
        elapsed += (option["out_end"] - entry) / rate + option.get("loop_extra_seconds", 0)
        entry, rate = option["in_end"], option["in_rate"]
    return entry, rate, elapsed


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
        for index, link in enumerate(links):
            t = transition_map[link]
            if (t.get("out_transpose", 0) != b.transpose.get(ids[index], 0) or
                    t.get("in_transpose", 0) != b.transpose.get(ids[index + 1], 0)):
                raise ValueError("Transition was calculated for a different transpose setting")
            if abs(t["pitch_pct"]) > b.max_pitch_pct + 1e-6 or t["skill"] > b.skill:
                raise ValueError("Transition violates pitch or skill limit")
            if t.get("key_lock",True) != b.key_lock: raise ValueError("Transition violates key-lock setting")
            if set(t["equipment"]) - set(b.equipment) or b.techniques and t["technique"] not in b.techniques:
                raise ValueError("Transition violates technique/equipment requirements")
        if abs(played - b.target_seconds) > b.tolerance_seconds:
            raise ValueError(f"Duration {played:.1f}s is outside requested tolerance")
    return {"valid": True, "duration_seconds": played, "entries": entries, "rates": rates}


def diagnose(brief, all_tracks, tracks, known=None):
    """Specific reasons a brief cannot be satisfied, checked before and after the search."""
    reasons = []
    known = known or {}
    for pin in brief.pinned:
        link = pin.get("transition_id")
        if link and link not in known:
            reasons.append(f"Pinned transition {link} is unknown; recalculate transitions and pin again")
        elif link and set(known[link]["track_ids"]) - tracks.keys():
            reasons.append(f"Pinned transition {link} uses a track outside the filtered candidates")
    if brief.playlists and not tracks:
        reasons.append("No analysed tracks belong to the selected playlists; reimport from rekordbox to refresh playlist membership")
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


def refresh_prefix(kept, links, known, tracks, brief):
    """Recalculate kept transitions that no longer fit the brief's key shifts or the preceding timing.

    Keeps the same technique where it is still available. Returns the new links and the recalculated pairs."""
    entry, rate, refreshed, changed = 0., 1., [], []
    for index, link in enumerate(links):
        a, b = tracks[kept[index]], tracks[kept[index + 1]]
        option = known.get(link)
        fits = (option and option["track_ids"] == [a["id"], b["id"]] and option["out_start"] >= entry
                and abs(option["out_rate"] - rate) <= 1e-6
                and option.get("out_transpose", 0) == brief.transpose.get(a["id"], 0)
                and option.get("in_transpose", 0) == brief.transpose.get(b["id"], 0))
        if not fits:
            choices = options(a, b, brief.json(), limit=18, outgoing_rate=rate, min_out_start=entry)
            if not choices:
                raise ValueError(f"No valid transition from {a.get('title') or a['id']} to {b.get('title') or b['id']} "
                                 "with the current key shifts; change the shift or keep fewer tracks")
            same = [o for o in choices if option and o["technique"] == option["technique"]]
            option = (same or choices)[0]
            known[option["id"]] = option
            changed.append(index)
        refreshed.append(option["id"])
        entry, rate = option["in_end"], option["in_rate"]
    return refreshed, changed


def build_plans(profiles, raw_brief, known=None, prefix=None, cancelled=lambda progress=None: None):
    """Search sets. `known` holds stored transitions for pins/prefixes; `prefix` keeps the opening route.

    `cancelled(progress)` is called throughout the search with an estimated 0–1 fraction and may raise to stop it."""
    brief = SetBrief.parse(raw_brief)
    known = dict(known or {})
    all_tracks = {t["id"]: t for t in profiles}
    wanted = set(brief.track_ids or all_tracks)
    unknown = (wanted | set(brief.must_play) | {i for i in (brief.start, brief.finish) if i}) - all_tracks.keys()
    if unknown:
        raise ValueError(f"Unknown brief track IDs: {sorted(unknown)}")
    tracks = {i: t for i, t in all_tracks.items() if i in wanted and i not in brief.excluded and t.get("duration", 0) > 0}
    if brief.genres:
        tracks = {i: t for i, t in tracks.items() if {g.casefold() for g in t.get("genres", [])} & {g.casefold() for g in brief.genres}}
    if brief.playlists:
        wanted_lists = {name.casefold() for name in brief.playlists}
        tracks = {i: t for i, t in tracks.items() if {n.casefold() for n in t.get("rekordbox_playlists", [])} & wanted_lists}
    forced = {}
    for pin in brief.pinned:
        link = pin.get("transition_id")
        if not link:
            continue
        option = known.get(link)
        if not option or set(option["track_ids"]) - tracks.keys():
            return {"plans": [], "transitions": [], "conflicts": diagnose(brief, all_tracks, tracks, known)}
        if option["track_ids"][0] in forced:
            return {"plans": [], "transitions": [], "conflicts": ["Two pinned transitions leave the same track"]}
        forced[option["track_ids"][0]] = option
    if prefix:
        kept = prefix.get("track_ids", [])
        if not kept or set(kept) - tracks.keys() or len(set(kept)) != len(kept):
            raise ValueError("Kept tracks must be distinct and available in the filtered candidates")
    if set(brief.must_play) - tracks.keys():
        raise ValueError("Must-play tracks are unavailable or outside the filtered collection")
    starts = [brief.start] if brief.start else sorted(tracks)
    if not starts:
        return {"plans": [], "transitions": [], "conflicts": diagnose(brief, all_tracks, tracks) + ["No analysed candidate tracks"]}
    nodes = [{"ids": [i], "links": [], "entry": 0., "rate": 1., "elapsed": 0., "score": 0., "techniques": []} for i in starts if i in tracks]
    nodes = [n for n in nodes if all(not p.get("track_id") or p["position"] != 0 or p["track_id"] == n["ids"][0] for p in brief.pinned)]
    transitions, cache, finals = {}, {}, []
    if prefix:
        kept, kept_links = prefix["track_ids"], prefix.get("transition_ids", [])
        if len(kept_links) != len(kept) - 1:
            raise ValueError("Kept part of the set needs one transition per adjacent pair")
        kept_links, recalculated = refresh_prefix(kept, kept_links, known, tracks, brief)
        entry, rate, elapsed = walk(kept, kept_links, known, tracks)
        for link in kept_links:
            transitions[link] = known[link]
        nodes = [{"ids": list(kept), "links": list(kept_links), "entry": entry, "rate": rate, "elapsed": elapsed,
                  "score": sum(known[l]["score"] for l in kept_links), "techniques": [known[l]["technique"] for l in kept_links]}]
    pinned_links = {p["transition_id"] for p in brief.pinned if p.get("transition_id")}
    for link in pinned_links:
        transitions[link] = known[link]
    def total(node):
        return node["elapsed"] + (tracks[node["ids"][-1]]["duration"] - node["entry"]) / node["rate"]
    def rank(node):
        missing = len(set(brief.must_play) - set(node["ids"])) + len(pinned_links - set(node["links"]))
        return abs(total(node) - brief.target_seconds) + missing * 120 - node["score"] * 3
    # Estimated set length in tracks, for progress only.
    mean = sum(t["duration"] for t in tracks.values()) / max(1, len(tracks))
    expected = max(1, min(len(tracks), round(brief.target_seconds / max(1., mean * .8))) - len(nodes[0]["ids"]) + 1) if nodes else 1
    for depth in range(len(tracks)):
        cancelled(min(.95, depth / expected))
        for node in nodes:
            if (not set(brief.must_play) - set(node["ids"]) and not pinned_links - set(node["links"])
                    and (not brief.finish or node["ids"][-1] == brief.finish)):
                finals.append(node)
        next_nodes = []
        for node_index, node in enumerate(nodes):
            cancelled(min(.95, (depth + node_index / max(1, len(nodes))) / expected))
            current = tracks[node["ids"][-1]]
            if brief.finish and current["id"] == brief.finish:
                continue
            if node["elapsed"] > brief.target_seconds + brief.tolerance_seconds:
                continue
            for candidate in tracks.values():
                if candidate["id"] in node["ids"]:
                    continue
                cancelled(None)
                if current["id"] in forced:
                    pinned = forced[current["id"]]
                    if candidate["id"] != pinned["track_ids"][1] or abs(pinned["out_rate"] - node["rate"]) > 1e-6:
                        continue
                    choices = [pinned]
                elif any(o["track_ids"][1] == candidate["id"] for o in forced.values()):
                    # A pinned transition's incoming track may only follow its pinned outgoing track.
                    continue
                else:
                    key = (current["id"], candidate["id"], round(node["rate"], 6))
                    if key not in cache:
                        cache[key] = options(current, candidate, brief.json(), outgoing_rate=node["rate"])
                    choices = cache[key]
                if current["id"] not in forced and any(o["out_start"] < node["entry"] for o in choices):
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
                    penalty -= .4 * (brief.smoothness - brief.contrast) * smoothness_value(option, current, candidate)
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
             "explanation": "Balances valid transition options, requested duration and the energy arc." +
                            (f" Keeps the first {len(prefix['track_ids'])} tracks and replans the rest." if prefix else "") +
                            (f" Recalculated {len(recalculated)} kept transition(s) for the current key shifts." if prefix and recalculated else ""),
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
    conflicts = [] if plans else diagnose(brief, all_tracks, tracks, known) + ["No valid set meets all constraints. Increase duration tolerance, allow more techniques, or change candidate tracks."]
    if errors and not plans:
        conflicts += list(dict.fromkeys(errors))[:3]
    return {"plans": plans, "transitions": list(transitions.values()), "conflicts": conflicts}
