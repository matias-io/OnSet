"""EDMFormer runs a track whole when it fits in memory and in equal chunks when it doesn't."""
import importlib.util
from pathlib import Path

WORKER = Path(__file__).resolve().parents[1] / "worker" / "deep_worker.py"


def load_worker():
    spec = importlib.util.spec_from_file_location("deep_worker_edmformer_under_test", WORKER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_a_track_that_fits_runs_whole():
    # Follow (221 s) completed whole; chunking it would cost its labels near the join.
    worker = load_worker()
    assert worker.edmformer_chunks(221.2) == (222, [])
    assert worker.edmformer_chunks(225) == (225, [])


def test_a_longer_track_is_split_into_equal_chunks_that_fit():
    # La Danza (240 s) peaked at 7.1 GB whole and was killed; in two 121 s chunks it peaked at
    # 4.5 GB. Five-minute tracks would need about 11 GB whole.
    worker = load_worker()
    assert worker.edmformer_chunks(240.25) == (121, [121.0])
    assert worker.edmformer_chunks(307) == (154, [154.0])
    length, joins = worker.edmformer_chunks(460)
    assert length <= 225 and joins == [154.0, 308.0] and length * (len(joins) + 1) >= 460
