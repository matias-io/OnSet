import threading
import time
import pytest
from onset_service.service import Service


@pytest.mark.parametrize("shutdown", [True, False])
def test_shutdown_preserves_resumable_job_but_explicit_cancel_stays_cancelled(tmp_path, song, monkeypatch, shutdown):
    from onset_service import analysis
    started=threading.Event()
    def waiting_analysis(*args):
        checkpoint=args[3]
        started.set()
        while True:
            checkpoint("fixture", .25)
            time.sleep(.01)
    monkeypatch.setattr(analysis,"analyse",waiting_analysis)
    service=Service(tmp_path/"app")
    track=service.store.import_track(song)
    job=service.enqueue("analyze",{"track_ids":[track["id"]],"deep":False})
    assert started.wait(10)
    if shutdown:
        service.job_control(job["id"],"pause")
    else:
        service.job_control(job["id"],"cancel")
        deadline=time.monotonic()+5
        while service.store.jobs()[0]["state"]!="cancelled" and time.monotonic()<deadline:
            time.sleep(.01)
        assert service.store.jobs()[0]["state"]=="cancelled"
    service.close()
    reopened=Service(tmp_path/"app")
    try:
        saved=reopened.store.jobs()[0]
        assert saved["id"]==job["id"]
        assert saved["state"]==("paused" if shutdown else "cancelled")
        assert saved["params"]["track_ids"]==[track["id"]]
        if shutdown:assert "resume" in saved["error"]
    finally:reopened.close()


def test_queue_resume_does_not_claim_running_and_duplicate_requests_reuse_job(tmp_path,song,monkeypatch):
    import threading
    from onset_service import analysis
    entered=threading.Event();release=threading.Event()
    def blocked(*args):
        entered.set();release.wait(5);args[3]("fixture",.2)
        raise RuntimeError("fixture end")
    monkeypatch.setattr(analysis,"analyse",blocked)
    service=Service(tmp_path/"app");track=service.store.import_track(song)
    first=service.enqueue("analyze",{"track_ids":[track["id"]],"deep":True})
    assert entered.wait(5)
    params={"track_ids":[track["id"]],"deep":True,"experimental":["edmformer"]}
    queued=service.enqueue("analyze",params)
    assert service.enqueue("analyze",params)["id"]==queued["id"]
    assert service.job_control(queued["id"],"resume")["state"]=="queued"
    service.job_control(first["id"],"cancel")
    release.set();service.close()
    reopened=Service(tmp_path/"app")
    assert next(j for j in reopened.store.jobs() if j["id"]==first["id"])["state"]=="cancelled"
    reopened.close()


def test_checkpointed_paused_job_can_be_cancelled_without_restarting_work(tmp_path):
    service=Service(tmp_path/"app")
    service.store.job({"id":"paused","kind":"analyze","state":"paused","params":{},"completed":[]})
    assert service.job_control("paused","cancel")["state"]=="cancelled"
    assert not service.controls
    service.close()


def test_pause_between_checkpoints_is_not_overwritten_by_completed_track(tmp_path,song,monkeypatch):
    from onset_service import analysis
    entered=threading.Event();release=threading.Event()
    def finishing(*args):
        args[3]("fixture",.1)
        if not entered.is_set():entered.set();release.wait(5)
        return {"state":"complete"}
    monkeypatch.setattr(analysis,"analyse",finishing)
    service=Service(tmp_path/"app")
    monkeypatch.setattr(service.store,"apply_analysis",lambda tid,result:None)
    first=service.store.import_track(song)
    monkeypatch.setattr(service.store,"get",lambda tid:{"id":tid})
    job=service.enqueue("analyze",{"track_ids":[first["id"],"second"],"deep":False})
    assert entered.wait(5)
    service.job_control(job["id"],"pause")
    release.set()
    deadline=time.monotonic()+5
    while first["id"] not in service.store.jobs()[0]["completed"] and time.monotonic()<deadline:
        time.sleep(.01)
    saved=service.store.jobs()[0]
    assert saved["completed"]==[first["id"]] and saved["state"]=="paused"
    service.job_control(job["id"],"cancel");service.close()


def test_full_disk_pauses_job_and_resume_reenqueues(tmp_path,song,monkeypatch):
    import errno
    from onset_service import analysis
    calls=[]
    def full(*args):
        calls.append(1)
        raise OSError(errno.ENOSPC,"No space left on device")
    monkeypatch.setattr(analysis,"analyse",full)
    service=Service(tmp_path/"app")
    a=service.store.import_track(song)
    job=service.enqueue("analyze",{"track_ids":[a["id"],"never-reached"],"deep":False})
    deadline=time.monotonic()+5
    while service.store.jobs()[0]["state"] not in {"paused","failed","partial"} and time.monotonic()<deadline:
        time.sleep(.01)
    saved=service.store.jobs()[0]
    assert saved["state"]=="paused" and "full" in saved["error"] and saved["completed"]==[]
    assert len(calls)==1
    monkeypatch.setattr(analysis,"analyse",lambda *args:{"state":"complete"})
    monkeypatch.setattr(service.store,"apply_analysis",lambda tid,result:None)
    service.job_control(job["id"],"resume")
    deadline=time.monotonic()+5
    while service.store.jobs()[0]["state"] not in {"complete","partial","failed"} and time.monotonic()<deadline:
        time.sleep(.01)
    assert service.store.jobs()[0]["completed"]==[a["id"]]
    service.close()
