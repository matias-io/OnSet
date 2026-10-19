"""Group-aware benchmark assignment and event detection metrics."""
from __future__ import annotations
from .util import digest
import re


def song_group(profile):
    if profile.get("song_group"):
        return profile["song_group"]
    artist, title = profile.get("artist", "").casefold(), profile.get("title", "").casefold()
    if not artist or not title:
        return profile["fingerprint"]
    title = re.sub(r"[\[(][^\])]*(?:mix|remix|edit|version)[^\])]*[\])]", "", title)
    title = re.sub(r"\s+-\s+.*(?:mix|remix|edit|version).*$", "", title)
    return re.sub(r"\W+", " ", artist + " " + title).strip()


def split(profiles):
    records = []
    for p in profiles:
        # User-supplied song group overrides keep edits of one song in one split.
        group = song_group(p)
        bucket = int(digest(group)[:8], 16) % 100
        records.append({"track_id": p["id"], "group": group,
                        "split": "development" if bucket < 60 else ("validation" if bucket < 80 else "test"),
                        "verified": False, "events": {}, "keys": [], "notes": ""})
    return {"schema_version": 1, "records": records}


def event_metrics(predictions, labels, tolerance=0):
    tp = fp = fn = 0
    present=absent=invented=mistimed=0
    for track_id, truth in labels.items():
        predicted = predictions.get(track_id, {})
        for kind, value in truth.items():
            if value.get("state") == "unknown":
                continue
            actual = value.get("bar") if value.get("state") == "present" else None
            detected = predicted.get(kind)
            if actual is None:
                absent+=1;invented+=detected is not None
                fp += detected is not None
            elif detected is None:
                present+=1
                fn += 1
            elif abs(detected - actual) <= tolerance:
                present+=1
                tp += 1
            else:
                present+=1;mistimed+=1
                fp += 1
                fn += 1
    precision = tp / (tp + fp) if tp + fp else 0
    recall = tp / (tp + fn) if tp + fn else 0
    return {"tp": tp, "fp": fp, "fn": fn, "precision": precision, "recall": recall,
            "f1": 2 * precision * recall / (precision + recall) if precision + recall else 0,
            "present_cases":present,"absent_cases":absent,"invented_events":invented,"mistimed_events":mistimed,
            "placement_accuracy":tp/present if present else None,"invented_event_rate":invented/absent if absent else None}


def timing_metrics(predicted, truth, tolerance=.07):
    """Ordered one-to-one matching; duplicated predictions cannot earn extra credit."""
    predicted, truth = sorted(predicted), sorted(truth)
    i=j=0; errors=[]
    while i<len(predicted) and j<len(truth):
        delta=predicted[i]-truth[j]
        if abs(delta)<=tolerance:errors.append(abs(delta));i+=1;j+=1
        elif delta<0:i+=1
        else:j+=1
    tp=len(errors);fp=len(predicted)-tp;fn=len(truth)-tp
    return {"tp":tp,"fp":fp,"fn":fn,"f1":2*tp/max(1,2*tp+fp+fn),
            "mean_error_seconds":sum(errors)/len(errors) if errors else None}


def evaluate(profiles, annotation, subset=None):
    """Evaluate raw observations only against explicitly verified listening annotations."""
    from .tonal import camelot
    import numpy as np
    lookup={p["id"]:p for p in profiles}
    records=[r for r in annotation.get("records",[]) if r.get("verified") and (not subset or r.get("split")==subset)]
    unknown={r["track_id"] for r in records}-lookup.keys()
    if unknown:raise ValueError(f"Unknown benchmark track IDs: {sorted(unknown)}")
    labels={r["track_id"]:r.get("events",{}) for r in records}
    predictions={i:dict((p.get("analysis") or {}).get("sections",{})) for i,p in lookup.items()}
    aliases={'first_full_groove':'drop','first_breakdown':'breakdown','later_drop':'second_drop'}
    for predicted in predictions.values():
        for name,legacy in aliases.items():predicted[name]=predicted.get(legacy)
    report={"schema_version":1,"subset":subset or "all verified", "verified_tracks":len(records),
            "annotation_id":digest(records),
            "events":{str(t):event_metrics(predictions,labels,t) for t in (0,1,2)},
            "beats":[],"downbeats":[],"boundaries":[],"keys":[],"tempo":[],"section_confusion_seconds":{},"resources":[]}
    kinds=sorted({kind for truth in labels.values() for kind in truth})
    report['events_by_type']={kind:{str(t):event_metrics(predictions,{i:{kind:value[kind]} for i,value in labels.items() if kind in value},t) for t in (0,1,2)} for kind in kinds}
    for r in records:
        p=lookup[r["track_id"]];a=p.get("analysis") or {}
        for sample in r.get("grid_samples",[]):
            start,end=float(sample["start"]),float(sample["end"])
            if not 0<=start<end<=p["duration"]:raise ValueError("Benchmark sample outside audio")
            for key in ("beats","downbeats"):
                if key not in sample:continue
                predicted=[t for t in a.get(key,[]) if start<=t<end]
                truth=[t for t in sample[key] if start<=t<end]
                report[key].append({"track_id":p["id"],"start":start,"end":end,**timing_metrics(predicted,truth)})
        if "boundaries" in r:
            boundaries=[s["start"] for s in a.get("structure",{}).get("sections",[])[1:]]
            report["boundaries"].append({"track_id":p["id"],**timing_metrics(boundaries,r["boundaries"],.5)})
        if r.get("keys"):
            accepted={camelot(k) for k in r["keys"]}
            estimate=a.get("key") or {};primary=camelot(estimate)
            alternatives={camelot(k) for k in estimate.get("alternatives",[])} if isinstance(estimate,dict) else set()
            report["keys"].append({"track_id":p["id"],"primary_correct":primary in accepted,
                                   "alternative_correct":bool((alternatives|{primary}) & accepted)})
        if r.get("bpm") and a.get("bpm"):
            ratio=a["bpm"]/r["bpm"]
            report["tempo"].append({"track_id":p["id"],"relative_error":abs(ratio-1),
                                    "half_double_ambiguity":abs(ratio-.5)<.02 or abs(ratio-2)<.04})
        for expected in r.get("sections",[]):
            if expected["label"]=="unknown":continue
            for detected in a.get("structure",{}).get("sections",[]):
                overlap=max(0,min(expected["end"],detected["end"])-max(expected["start"],detected["start"]))
                if overlap:
                    key=expected["label"]+" → "+detected["label"]
                    report["section_confusion_seconds"][key]=report["section_confusion_seconds"].get(key,0)+overlap
        report["resources"].append({"track_id":p["id"],"runtime_seconds":a.get("seconds"),
                                   "resource_observations":a.get("resources"),"worker_resources":a.get("deep",{}).get("resources"),"algorithm":a.get("algorithm")})
    judgments=[]
    for j in annotation.get("energy_judgments",[]):
        if j["a"] not in lookup or j["b"] not in lookup:raise ValueError("Unknown energy benchmark track")
        def rating(i):return (lookup[i].get("analysis") or {}).get("energy_profile",{}).get("rating")
        ra,rb=rating(j["a"]),rating(j["b"])
        if ra is None or rb is None:continue
        predicted="equal" if abs(ra-rb)<.5 else j["a"] if ra>rb else j["b"]
        judgments.append(predicted==j["winner"])
    reviews=[r for r in annotation.get("transition_reviews",[]) if isinstance(r.get("accepted"),bool)]
    report["energy_ranking"]={"comparisons":len(judgments),"agreement":float(np.mean(judgments)) if judgments else None}
    report["transition_review"]={"reviewed":len(reviews),"acceptance_rate":float(np.mean([r["accepted"] for r in reviews])) if reviews else None,
                                  "review_seconds":sum(r.get("review_seconds",0) for r in reviews)}
    report["notes"]=["Unverified annotations are excluded", "Raw detector outputs are evaluated; human section/key corrections are not predictions", "No accuracy conclusion is available for domains without verified labels"]
    return report


def compare_releases(baseline,candidate):
    if baseline.get('subset')!='test' or candidate.get('subset')!='test' or not candidate.get('verified_tracks') or baseline.get('annotation_id')!=candidate.get('annotation_id'):
        raise ValueError('Compare releases on the same verified held-out test annotations')
    def delta(kind,metric):
        a=baseline.get('events_by_type',{}).get(kind,{}).get('0',{}).get(metric)
        b=candidate.get('events_by_type',{}).get(kind,{}).get('0',{}).get(metric)
        return b-a if a is not None and b is not None else None
    groove=delta('first_full_groove','placement_accuracy')
    if groove is None:groove=delta('drop','placement_accuracy')
    breakdown=delta('breakdown','f1')
    if breakdown is None:breakdown=delta('first_breakdown','f1')
    outro=delta('outro','f1')
    a=baseline['events']['0'].get('invented_event_rate');b=candidate['events']['0'].get('invented_event_rate')
    guard=a is not None and b is not None and b<=a
    return {'schema_version':1,'first_groove_improvement':groove,'breakdown_f1_improvement':breakdown,'outro_f1_improvement':outro,
            'no_invented_event_rate_increase':guard,'promotion_targets_met':groove is not None and groove>=.05 and breakdown is not None and outro is not None and min(breakdown,outro)>=.1 and guard,
            'note':'Requires held-out evidence in every target domain; no model is promoted automatically.'}
