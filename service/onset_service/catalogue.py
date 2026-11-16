"""Technique definitions shared by the planner, UI and exported instructions."""
DATA = [
 ("long_blend", "Long blend", 16, ["eq"], 1, True, True),
 ("quick_blend", "Quick blend", 4, ["eq"], 1, True, True),
 ("eq_swap", "EQ / bass swap", 16, ["eq"], 1, True, True),
 ("filter", "Filter transition", 8, ["filter"], 1, True, True),
 ("cut", "Cut / dropmix", 0, [], 1, False, True),
 ("drop_swap", "Drop swap", 0, [], 2, False, True),
 ("double_drop", "Double drop", 8, ["eq"], 3, True, False),
 ("breakdown", "Breakdown blend", 16, ["eq"], 2, True, True),
 ("echo_out", "Echo out", 2, ["echo"], 1, False, True),
 ("reverb_out", "Reverb out", 2, ["reverb"], 2, False, True),
 ("loop_blend", "Loop-assisted blend", 16, ["loop", "eq"], 2, True, True),
 ("loop_roll", "Shortening loop", 4, ["loop"], 3, True, False),
 ("stem_blend", "Acapella / stem blend", 16, ["stems", "eq"], 3, True, False),
 ("tease", "Tease / hot-cue exchange", 8, ["hot_cues"], 3, True, False),
 ("backspin", "Backspin / brake", 0, [], 2, False, False),
 ("tempo_bridge", "Tempo bridge", 8, ["tempo"], 3, False, False),
 ("fade", "Fade / end-to-start", 2, [], 1, False, True),
 ("wordplay", "Wordplay / thematic", 0, [], 2, False, False),
]

CATALOGUE = [{"schema_version": 1, "id": i, "name": name, "bars": bars, "equipment": equipment,
              "skill": skill, "beatmatched": beat, "preview": preview} for i, name, bars, equipment, skill, beat, preview in DATA]
