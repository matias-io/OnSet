"""Show mode (plan O4): jobs wait at their next checkpoint and nothing writes to rekordbox."""
import threading
import time

import pytest

from onset_service.service import Service


def wait_for(predicate, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(.01)
    return False


@pytest.fixture
def looping(monkeypatch):
    """Analysis that counts its checkpoints until cancelled."""
    from onset_service import analysis
    progress = {"count": 0}
    started = threading.Event()

    def analyse(*args):
        checkpoint = args[3]
        started.set()
        while True:
            checkpoint("fixture", .5)
            progress["count"] += 1
            time.sleep(.005)
    monkeypatch.setattr(analysis, "analyse", analyse)
    return progress, started


def state(service, job_id):
    return next(j for j in service.store.jobs() if j["id"] == job_id)["state"]


def test_show_pauses_running_jobs_and_resumes_them_afterwards(tmp_path, song, looping):
    progress, started = looping
    service = Service(tmp_path / "app")
    try:
        track = service.store.import_track(song)
        job = service.enqueue("analyze", {"track_ids": [track["id"]], "deep": False})
        assert started.wait(10)
        assert service.show_mode(True) == {"show": True, "jobs": [job["id"]]}
        assert state(service, job["id"]) == "paused"
        time.sleep(.3)
        frozen = progress["count"]
        time.sleep(.3)
        assert progress["count"] == frozen, "the job kept working during the show"
        assert service.show_mode(True) == {"show": True, "jobs": []}
        assert service.show_mode(False) == {"show": False, "jobs": [job["id"]]}
        assert wait_for(lambda: progress["count"] > frozen)
        assert state(service, job["id"]) == "running"
    finally:
        service.close()


def test_jobs_queued_during_a_show_wait_for_it_to_end(tmp_path, song, looping):
    progress, started = looping
    service = Service(tmp_path / "app")
    try:
        service.show_mode(True)
        track = service.store.import_track(song)
        job = service.enqueue("analyze", {"track_ids": [track["id"]], "deep": False})
        assert job["state"] == "paused" and job["stage"] == "waiting for the show to end"
        time.sleep(.3)
        assert progress["count"] == 0
        with pytest.raises(ValueError, match="resume when the show ends"):
            service.job_control(job["id"], "resume")
        service.show_mode(False)
        assert wait_for(lambda: progress["count"] > 0)
    finally:
        service.close()


def test_a_job_paused_by_the_dj_during_the_show_stays_paused(tmp_path, song, looping):
    progress, started = looping
    service = Service(tmp_path / "app")
    try:
        track = service.store.import_track(song)
        job = service.enqueue("analyze", {"track_ids": [track["id"]], "deep": False})
        assert started.wait(10)
        service.show_mode(True)
        service.job_control(job["id"], "pause")
        assert service.show_mode(False) == {"show": False, "jobs": []}
        frozen = progress["count"]
        time.sleep(.3)
        assert progress["count"] == frozen and state(service, job["id"]) == "paused"
    finally:
        service.close()


def test_no_rekordbox_writes_or_restores_during_a_show(tmp_path):
    service = Service(tmp_path / "app")
    try:
        service.show_mode(True)
        with pytest.raises(ValueError, match="not written to during a show"):
            service.rb_apply({"id": "anything"})
        with pytest.raises(ValueError, match="not written to during a show"):
            service.restore({"id": "anything"})
    finally:
        service.close()
