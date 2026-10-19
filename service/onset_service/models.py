"""Versioned JSON boundary types and validation independent of model runtimes."""
from __future__ import annotations
import math
from dataclasses import dataclass, field, asdict
from typing import Any


def validate_grid(times, duration=None):
    values = [float(t) for t in times]
    if len(values) < 2 or any(not math.isfinite(t) or t < 0 for t in values):
        raise ValueError("Grid needs at least two finite nonnegative positions")
    if any(b <= a for a, b in zip(values, values[1:])):
        raise ValueError("Grid must be strictly increasing")
    if duration is not None and values[-1] > duration + .05:
        raise ValueError("Grid extends past the audio")
    return values


@dataclass
class SetBrief:
    track_ids: list[str] = field(default_factory=list)
    target_seconds: float = 3600
    tolerance_seconds: float = 120
    must_play: list[str] = field(default_factory=list)
    excluded: list[str] = field(default_factory=list)
    start: str | None = None
    finish: str | None = None
    max_pitch_pct: float = 6
    key_lock: bool = True
    equipment: list[str] = field(default_factory=lambda: ["eq", "filter", "loop", "echo", "reverb"])
    techniques: list[str] = field(default_factory=list)
    skill: int = 2
    energy_arc: list[float] = field(default_factory=lambda: [4, 6, 8, 7])
    genres: list[str] = field(default_factory=list)
    mood: str = ""
    variety: float = .5
    exploration: float = .5
    pinned: list[dict[str, Any]] = field(default_factory=list)
    smoothness: float = .5
    contrast: float = .5
    playlists: list[str] = field(default_factory=list)
    transpose: dict[str, float] = field(default_factory=dict)
    schema_version: int = 1

    @classmethod
    def parse(cls, value):
        if value.get("schema_version", 1) != 1:
            raise ValueError("Unsupported SetBrief schema")
        unexpected = set(value) - cls.__dataclass_fields__.keys()
        if unexpected:
            raise ValueError(f"Unknown brief fields: {sorted(unexpected)}")
        result = cls(**value)
        if not math.isfinite(result.target_seconds) or result.target_seconds <= 0:
            raise ValueError("Target duration must be positive")
        if not 0 <= result.max_pitch_pct <= 50 or result.tolerance_seconds < 0:
            raise ValueError("Invalid pitch limit or duration tolerance")
        if not math.isfinite(result.tolerance_seconds) or not 0 <= result.variety <= 1 or not 0 <= result.exploration <= 1:
            raise ValueError("Tolerance must be finite; variety and exploration must be from 0 to 1")
        if result.skill not in (1,2,3): raise ValueError("Skill must be 1, 2 or 3")
        if any(not isinstance(v,(int,float)) or isinstance(v,bool) or not 0 <= v <= 1 for v in (result.smoothness, result.contrast)):
            raise ValueError("Smoothness and contrast must be from 0 to 1")
        if not isinstance(result.playlists,list) or any(not isinstance(v,str) or not v.strip() for v in result.playlists):
            raise ValueError("Playlists must be a list of names")
        if not isinstance(result.transpose,dict) or any(not isinstance(v,(int,float)) or isinstance(v,bool) or not -12 <= v <= 12 or v != int(v)
                                                        for v in result.transpose.values()):
            raise ValueError("Transpose values must be whole semitones from -12 to 12")
        for pin in result.pinned:
            if pin.get("transition_id") is not None and (not isinstance(pin["transition_id"],str) or not pin["transition_id"]):
                raise ValueError("Pinned transitions need a transition ID")
        for pin in result.pinned:
            if pin.get("track_id") and (not isinstance(pin.get("position"),int) or pin["position"] < 0):
                raise ValueError("Pinned track positions must be nonnegative integers")
        if set(result.must_play) & set(result.excluded):
            raise ValueError("A must-play track is excluded")
        if not result.energy_arc or any(not 1 <= e <= 10 for e in result.energy_arc):
            raise ValueError("Energy arc must contain values from 1 to 10")
        return result

    def json(self):
        return asdict(self)
