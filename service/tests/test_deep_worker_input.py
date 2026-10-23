"""The deep worker hands Demucs stereo audio (its mono path fails in place)."""
import importlib.util
from pathlib import Path

import numpy as np
import soundfile as sf

WORKER = Path(__file__).resolve().parents[1] / "worker" / "deep_worker.py"


def load_worker():
    spec = importlib.util.spec_from_file_location("deep_worker_under_test", WORKER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_mono_audio_gets_a_stereo_copy_under_the_same_name(tmp_path):
    worker = load_worker()
    mono = tmp_path / "audio.wav"
    tone = (0.25 * np.sin(np.linspace(0, 200 * np.pi, 4410))).astype("float32")
    sf.write(mono, tone, 44100, subtype="PCM_16")
    source, scratch = worker.stereo_input(str(mono), tmp_path / "out")
    assert Path(source).name == "audio.wav" and Path(source) != mono
    data, rate = sf.read(source, always_2d=True)
    assert rate == 44100 and data.shape == (4410, 2)
    assert np.array_equal(data[:, 0], data[:, 1])
    assert scratch is not None and Path(source).parent == scratch


def test_stereo_audio_is_used_as_it_is(tmp_path):
    worker = load_worker()
    stereo = tmp_path / "audio.wav"
    sf.write(stereo, np.zeros((441, 2), dtype="float32"), 44100, subtype="PCM_16")
    assert worker.stereo_input(str(stereo), tmp_path / "out") == (str(stereo), None)
