import json
import sqlite3
import pytest
from onset_service.store import Store
from onset_service.service import Service
from onset_service import workspace
from conftest import analysis


def test_consistent_backup_preserves_history_undo_jobs_and_rollback(tmp_path,song):
    service=Service(tmp_path/'app');s=service.store
    track=s.import_track(song);track=s.apply_analysis(track['id'],analysis(song))
    track=s.edit(track['id'],{'notes':'Before snapshot'})
    s.job({'id':'job','kind':'analyze','state':'running','completed':[],'params':{'track_ids':[track['id']]}})
    saved=service.dispatch('workspace-backup')
    expected=workspace.counts(s.db)
    s.edit(track['id'],{'notes':'After snapshot'})
    proposed=service.dispatch('workspace-restore-preview',{'id':saved['id']})
    assert proposed['before']['edits']>proposed['after']['edits']
    with pytest.raises(ValueError,match='Approve'):service.dispatch('workspace-restore',{'id':saved['id'],'token':proposed['token']})
    result=service.dispatch('workspace-restore',{'id':saved['id'],'token':proposed['token'],'approved':True})
    assert workspace.counts(s.db)==expected
    assert s.get(track['id'])['notes']=='Before snapshot'
    assert s.jobs()[0]['state']=='paused'
    assert s.undo(track['id'])['notes']==''
    assert workspace.verified_backup(s,result['rollback_backup'])[0]['reason']=='before-restore'
    assert workspace.health(s)['healthy']
    service.close()


def test_preview_is_bound_to_edits_even_when_record_counts_do_not_change(tmp_path,song):
    s=Store(tmp_path/'app');t=s.import_track(song);saved=workspace.backup(s)
    proposed=workspace.preview(s,saved['id'])
    t['notes']='Unpreviewed edit';s.save(t)
    with pytest.raises(ValueError,match='changed after'):workspace.restore(s,saved['id'],proposed['token'])
    assert s.get(t['id'])['notes']=='Unpreviewed edit'
    s.close()


def test_missing_or_altered_backup_is_rejected(tmp_path,song):
    s=Store(tmp_path/'app');s.import_track(song);saved=workspace.backup(s)
    path=s.home/'workspace-backups'/(saved['id']+'.db');path.write_bytes(b'changed')
    with pytest.raises(ValueError,match='checksum'):workspace.preview(s,saved['id'])
    with pytest.raises(ValueError,match='Invalid'):workspace.preview(s,'../outside')
    s.close()


def test_track_loss_is_detected_after_restart_and_blocks_normal_operations(tmp_path,song):
    home=tmp_path/'app';s=Store(home);s.import_track(song);s.close()
    c=sqlite3.connect(home/'workspace.db');c.execute('DELETE FROM tracks');c.commit();c.close()
    service=Service(home)
    status=service.dispatch('workspace-health')
    assert not status['healthy'] and status['last_known_counts']['tracks']==1
    with pytest.raises(ValueError,match='recovery review'):service.dispatch('import',{'paths':[str(song)]})
    saved=service.dispatch('workspace-backups')[0]
    proposed=service.dispatch('workspace-restore-preview',{'id':saved['id']})
    service.dispatch('workspace-restore',{'id':saved['id'],'token':proposed['token'],'approved':True})
    assert len(service.dispatch('tracks'))==1
    service.close()


def test_backup_contains_committed_wal_pages_and_never_credentials(tmp_path,song):
    s=Store(tmp_path/'app');s.import_track(song);(s.home/'accounts.enc').write_bytes(b'private')
    saved=workspace.backup(s)
    c=sqlite3.connect(s.home/'workspace-backups'/(saved['id']+'.db'))
    assert c.execute('select count(*) from tracks').fetchone()[0]==1
    assert c.execute('pragma integrity_check').fetchone()[0]=='ok';c.close()
    assert b'private' not in (s.home/'workspace-backups'/(saved['id']+'.db')).read_bytes()
    s.close()


def test_active_worker_prevents_workspace_restore_preview(tmp_path,song):
    service=Service(tmp_path/'app');service.store.import_track(song)
    saved=service.dispatch('workspace-backup')
    service.controls['fixture']={'active':True}
    with pytest.raises(ValueError,match='worker'):service.dispatch('workspace-restore-preview',{'id':saved['id']})
    service.controls.clear();service.close()


def test_replaced_open_database_is_detected_without_writing_new_marker(tmp_path,song,monkeypatch):
    s=Store(tmp_path/'app');s.import_track(song)
    marker=(s.home/'workspace-state.json').read_bytes()
    monkeypatch.setattr(workspace,'identity',lambda path:[-1,-1])
    assert not workspace.health(s)['healthy']
    with pytest.raises(ValueError,match='recovery review'):workspace.guard(s)
    s.close()
    assert (tmp_path/'app'/'workspace-state.json').read_bytes()==marker


def test_portable_snapshot_exports_all_history_and_rejects_live_database_target(tmp_path,song):
    service=Service(tmp_path/'app');s=service.store
    t=s.import_track(song);s.apply_analysis(t['id'],analysis(song));s.edit(t['id'],{'notes':'Keep history'})
    target=tmp_path/'history.db'
    result=service.export({'kind':'workspace-backup','path':str(target)})
    c=sqlite3.connect(target)
    assert c.execute('select count(*) from revisions').fetchone()[0]==result['counts']['revisions']
    assert c.execute('select count(*) from edits').fetchone()[0]==1;c.close()
    with pytest.raises(ValueError,match='outside active'):service.export({'kind':'workspace-backup','path':str(s.db_path)})
    service.close()


def test_legacy_autocue_folder_moves_once_with_its_database(tmp_path):
    from onset_service.util import migrate_legacy_home
    legacy,home=tmp_path/'Autocue',tmp_path/'Onset'/'Onset'/'data'/'workspace'
    legacy.mkdir()
    for name in ('autocue.db','autocue.db-wal','autocue.db-shm','onset-accounts.dpapi'):(legacy/name).write_text(name)
    (legacy/'features').mkdir()
    assert migrate_legacy_home(legacy,home)
    assert not legacy.exists() and (home/'features').is_dir() and (home/'onset-accounts.dpapi').exists()
    assert [(home/f'workspace.db{s}').read_text() for s in ('','-wal','-shm')]==['autocue.db','autocue.db-wal','autocue.db-shm']
    legacy.mkdir();(legacy/'autocue.db').write_text('newer')
    assert not migrate_legacy_home(legacy,home) and (legacy/'autocue.db').exists()


def test_a_workspace_opened_by_path_adopts_its_legacy_database(tmp_path, monkeypatch):
    import sqlite3
    from onset_service.store import Store
    from onset_service.util import adopt_legacy_database
    folder = tmp_path / 'given-workspace'
    folder.mkdir()
    legacy = sqlite3.connect(folder / 'autocue.db')
    legacy.execute('CREATE TABLE tracks(id TEXT PRIMARY KEY, fingerprint TEXT UNIQUE, profile TEXT NOT NULL)')
    legacy.execute("INSERT INTO tracks VALUES('t1','f1','{}')")
    legacy.execute('PRAGMA user_version=1')
    legacy.commit()
    legacy.close()
    monkeypatch.setenv('ONSET_WORKSPACE', str(folder))
    store = Store()
    assert store.db_path == folder / 'workspace.db' and not (folder / 'autocue.db').exists()
    assert store.db.execute('SELECT count(*) FROM tracks').fetchone()[0] == 1
    store.close()
    (folder / 'autocue.db').write_text("stale copy")
    assert not adopt_legacy_database(folder) and (folder / 'autocue.db').read_text() == "stale copy"


def test_a_live_service_keeps_a_second_one_off_its_workspace(tmp_path, monkeypatch):
    import json, os
    import pytest
    from onset_service import workspace
    from onset_service.store import Store
    home = tmp_path / 'home'
    first = Store(home)
    assert json.loads((home / workspace.PID_FILE).read_text(encoding='utf-8'))['pid'] == os.getpid()
    # The same process may reopen its workspace (the CLI and these tests do).
    Store(home).close()
    # Another process's live service on this workspace: refused, and its record kept.
    (home / workspace.PID_FILE).write_text(json.dumps({'pid': 4242, 'started': 'earlier'}), encoding='utf-8')
    monkeypatch.setattr(workspace, 'command_line', lambda pid: 'python -u -m onset_service --home x rpc' if pid == 4242 else None)
    with pytest.raises(workspace.WorkspaceInUse):
        Store(home)
    assert json.loads((home / workspace.PID_FILE).read_text(encoding='utf-8'))['pid'] == 4242
    # A record whose process is gone, or is some other program now, is stale and replaced.
    monkeypatch.setattr(workspace, 'command_line', lambda pid: 'notepad.exe')
    Store(home).close()
    assert not (home / workspace.PID_FILE).exists()
    first.close()


def test_a_stalled_job_writes_every_threads_stack_to_the_workspace_once(tmp_path):
    from datetime import datetime, timedelta, timezone
    from onset_service.service import Service, STALL_LOG, stalled_jobs
    service = Service(tmp_path / 'home')
    try:
        job = {'id': 'j1', 'kind': 'analyze', 'state': 'running', 'stage': 'loading audio tools', 'progress': 0.01,
               'params': {}, 'completed': []}
        service.store.job(job)
        saved = service.store.jobs()[0]
        soon = datetime.now(timezone.utc) + timedelta(seconds=60)
        later = datetime.now(timezone.utc) + timedelta(seconds=600)
        assert stalled_jobs([saved], soon) == [] and [j['id'] for j in stalled_jobs([saved], later)] == ['j1']
        assert service.log_stalls(soon) == []
        assert service.log_stalls(later) == ['j1']
        assert service.log_stalls(later) == [], 'one entry per stalled checkpoint'
        text = (tmp_path / 'home' / STALL_LOG).read_text(encoding='utf-8')
        assert "stalled at 'loading audio tools'" in text and 'thread MainThread' in text and 'test_workspace.py' in text
        done = dict(saved, state='complete'); service.store.job(done)
        assert stalled_jobs(service.store.jobs(), later) == []
    finally:
        service.close()


def test_the_service_loads_the_audio_stack_before_serving(tmp_path):
    import sys
    from onset_service.service import Service, WARM_MODULES
    service = Service(tmp_path / 'home')
    try:
        assert service.imports_ready.wait(300), 'warm-up did not finish'
        missing = [m for m in WARM_MODULES if m not in sys.modules]
        assert not missing, missing
        import librosa
        before = set(sys.modules)
        for name in librosa.__all__:
            if name != 'display':
                getattr(librosa, name)
        assert set(sys.modules) == before, sorted(set(sys.modules) - before)[:8]
        assert 'librosa.display' not in sys.modules, 'display needs matplotlib and is not used'
    finally:
        service.close()


def test_light_requests_answer_while_the_warm_up_runs(tmp_path, monkeypatch):
    import time
    from onset_service import service as service_module
    from onset_service.service import Service
    monkeypatch.setattr(service_module, 'warm_imports', lambda: (time.sleep(1.0), 1.0)[1])
    events = []
    service = Service(tmp_path / 'home', notify=events.append)
    try:
        started = time.monotonic()
        assert service.dispatch('tracks') == [] and service.dispatch('jobs') == []
        assert time.monotonic() - started < 0.5, 'DB-only requests must not wait for the warm-up'
        assert not service.imports_ready.is_set()
        assert service.dispatch('sets') == []
        assert service.imports_ready.is_set(), 'a request that may import the numeric stack waits'
        stages = [e['params']['stage'] for e in events if e.get('method') == 'service-startup']
        assert stages == ['loading audio tools', 'ready']
    finally:
        service.close()
