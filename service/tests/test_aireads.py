"""AI structure reads: the bar-gridded spectrogram image and importing reads as observations."""
import struct
import zlib

import numpy as np
import pytest

from onset_service.aireads import observations, render
from onset_service.service import Service


def grid_analysis(bars=40, seconds_per_bar=2.0):
    frames = 700
    values = np.linspace(-80, 0, 64 * frames).reshape(64, frames)
    return {"downbeats": [i * seconds_per_bar for i in range(bars)], "duration": bars * seconds_per_bar,
            "spectrogram": {"values": values.round(1).tolist(), "frequencies": list(range(64)),
                            "duration": bars * seconds_per_bar}}


def png_size(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    width, height = struct.unpack(">II", data[16:24])
    return width, height


def test_render_gives_every_bar_the_same_width(tmp_path):
    out = tmp_path / "s.png"
    info = render(grid_analysis(bars=40), out, px_per_bar=8)
    data = out.read_bytes()
    assert png_size(data) == (40 * 8, 9 + 64 * 3)
    assert info["bars"] == 40 and info["width"] == 320
    assert info["bar_times"][:3] == [0.0, 2.0, 4.0] and len(info["bar_times"]) == 40
    assert "Each bar is 8 pixels wide" in info["how_to_read"]
    # The image decodes: the IDAT stream inflates to height rows of 1 + 3*width bytes.
    idat = data[data.index(b"IDAT") + 4:data.index(b"IEND") - 8]
    assert len(zlib.decompress(idat)) == info["height"] * (1 + 3 * info["width"])


def test_render_refuses_tracks_without_analysis(tmp_path):
    with pytest.raises(ValueError, match="analyse it first"):
        render({}, tmp_path / "x.png")


def test_observations_replace_only_their_own_source():
    track = {"analysis": {"downbeats": [0, 2, 4, 6, 8]},
             "observations": [{"event": "drop", "source": "rekordbox", "state": "estimated", "bar": 2},
                              {"event": "drop", "source": "claude:spectrogram", "state": "estimated", "bar": 1}]}
    result = observations(track, "claude:spectrogram", "spectrogram-llm",
                          {"drop": {"state": "present", "bar": 3}, "breakdown": {"state": "absent"}})
    mine = {o["event"]: o for o in result if o["source"] == "claude:spectrogram"}
    assert [o["source"] for o in result].count("rekordbox") == 1
    assert mine["drop"]["bar"] == 3 and mine["drop"]["original_time"] == 6 and mine["drop"]["state"] == "estimated"
    assert mine["breakdown"]["state"] == "absent" and mine["breakdown"]["bar"] is None
    assert mine["outro"]["state"] == "unknown" and mine["drop"]["family"] == "spectrogram-llm"


def test_observations_reject_bars_outside_the_grid_and_unknown_states():
    track = {"analysis": {"downbeats": [0, 2, 4]}, "observations": []}
    with pytest.raises(ValueError, match="needs a bar"):
        observations(track, "s", "f", {"drop": {"state": "present", "bar": 9}})
    with pytest.raises(ValueError, match="state must be"):
        observations(track, "s", "f", {"drop": {"state": "maybe"}})


def test_service_renders_and_imports_against_the_current_revision(tmp_path):
    service = Service(tmp_path / "app")
    track = service.store.import_track(str(tmp_path / "missing.wav"))
    track["analysis"] = grid_analysis(bars=24)
    track["revision"] = "r1"
    service.store.save(track)
    image = service.spectrogram_image({"id": track["id"]})
    assert image["revision"] == "r1" and image["bars"] == 24
    assert (tmp_path / "app" / "ai-reads").is_dir()
    with pytest.raises(ValueError, match="analysis changed"):
        service.observations_import({"track_id": track["id"], "revision": "old", "source": "s", "family": "f",
                                     "events": {}})
    stored = service.observations_import({"track_id": track["id"], "revision": "r1", "source": "codex:spectrogram",
                                          "family": "spectrogram-llm",
                                          "events": {"drop": {"state": "present", "bar": 8}}})
    assert [o["event"] for o in stored["observations"]] == ["drop", "breakdown", "second_drop", "outro"]
    assert service.evidence({"id": track["id"]})["resolved_opinions"] is not None
    service.close()
