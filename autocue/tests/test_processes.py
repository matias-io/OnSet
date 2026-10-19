import sys,time
import pytest
from autocue_app.processes import run_process


def test_owned_process_can_be_cancelled(tmp_path):
    pidfile=tmp_path/'pid'
    start=time.monotonic()
    class Cancelled(Exception):pass
    def checkpoint(*args):
        if pidfile.exists():raise Cancelled()
    with pytest.raises(Cancelled):
        run_process([sys.executable,'-c',f"import os,time,pathlib;pathlib.Path({str(pidfile)!r}).write_text(str(os.getpid()));time.sleep(60)"],checkpoint,'fixture',10)
    import psutil
    assert not psutil.pid_exists(int(pidfile.read_text()))
    assert time.monotonic()-start<10


def test_process_failure_reports_recoverable_detail():
    with pytest.raises(RuntimeError,match='fixture problem'):
        run_process([sys.executable,'-c',"print('fixture problem');raise SystemExit(2)"],lambda *a:None,'fixture',10)


def test_owned_process_returns_captured_output():
    assert run_process([sys.executable,'-c',"print('/fixture/home')"],lambda *args:None,'fixture',10)=='/fixture/home'


def test_wsl_startup_timeout_is_explicit_unavailable_evidence(tmp_path,monkeypatch):
    import subprocess
    import autocue_app.processes as processes
    from autocue_app.deep import run_deep
    def timeout(*args,**kwargs):raise subprocess.TimeoutExpired('wsl',60)
    monkeypatch.setattr(processes,'run_process',timeout)
    result=run_deep(tmp_path/'audio.wav',tmp_path,lambda *args:None)
    assert set(result['models'])=={'beat_this','demucs','essentia','genre'}
    assert all(v['state']=='unavailable' and 'timed out' in v['error'] for v in result['models'].values())
