from pathlib import Path
import sqlite3
import pytest
from autocue_app.store import Store
from autocue_app.rekordbox import prepare,apply,restore,open_db,consistent_backup
from conftest import analysis


def test_rekordbox_import_keeps_duration_and_reports_missing_audio(tmp_path,song,plain_db):
    from autocue_app.service import Service
    db=open_db(plain_db,True)
    original=db.get_content(ID='42');original.Length=24
    from pyrekordbox.db6.tables import DjmdContent
    import datetime as dt
    now=dt.datetime.now()
    db.session.add(DjmdContent(ID='43',FolderPath=str(tmp_path/'missing.wav'),Title='Missing',created_at=now,updated_at=now))
    db.session.commit();db.close();db.engine.dispose()
    events=[];service=Service(tmp_path/'app',events.append)
    result=service.rb_import({'db_path':str(plain_db),'plain':True})
    assert result['imported']==1 and len(result['skipped'])==1 and not result['failed']
    assert service.store.get(result['track_ids'][0])['duration']==pytest.approx(24)
    assert events[-1]['params']['completed']==2
    assert service.store.artifacts('import-report')
    service.close()


def test_approved_writes_idempotence_and_restore(tmp_path,song,plain_db):
    store=Store(tmp_path/'app')
    p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    # No approval means no additions or deletions.
    assert not prepare(store,[p['id']],plain_db,True)['changes']
    for cue in p['cues']:
        store.review_cue(p['id'],cue['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True,comments=True)
    assert len(preview['changes'][0]['operations'])==3
    manifest=apply(store,preview['id'])
    assert not manifest['failed']
    db=open_db(plain_db,True)
    cues=list(db.get_cue(ContentID='42'))
    assert len(cues)==4 and any(c.Comment=='My manual cue' for c in cues)
    assert next(c for c in cues if c.Comment=='I intro loop').BeatLoopSize==0x10000*16+1
    assert db.get_content(ID='42').Commnt.startswith('My original notes')
    db.close();db.engine.dispose()
    assert not prepare(store,[p['id']],plain_db,True,comments=True)['changes']
    restore(store,manifest['id'])
    db=open_db(plain_db,True)
    assert len(list(db.get_cue(ContentID='42')))==1
    db.close();db.engine.dispose();store.close()


def test_manual_changes_after_preview_are_rejected(tmp_path,song,plain_db):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True)
    db=open_db(plain_db,True);db.get_content(ID='42').Commnt='Edited elsewhere';db.commit();db.close();db.engine.dispose()
    with pytest.raises(ValueError,match='changed'):apply(store,preview['id'])
    store.close()


def test_edited_generated_cue_is_preserved_as_conflict(tmp_path,song,plain_db):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    apply(store,prepare(store,[p['id']],plain_db,True)['id'])
    db=open_db(plain_db,True)
    cue=next(c for c in db.get_cue(ContentID='42') if c.Comment=='A intro')
    cue.InMsec=550;db.commit();db.close();db.engine.dispose()
    preview=prepare(store,[p['id']],plain_db,True)
    assert preview['conflicts'] and not preview['changes']
    store.close()


def test_bad_auxiliary_backup_refuses_restore_before_mutation(tmp_path,song,plain_db):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    manifest=apply(store,prepare(store,[p['id']],plain_db,True)['id'])
    bad=tmp_path/'bad.xml';bad.write_text('broken')
    manifest['auxiliary']={'source':str(tmp_path/'master.xml'),'backup':str(bad),'hash':'incorrect'}
    store.artifact('write-manifest',manifest['id'],manifest)
    before=plain_db.read_bytes()
    with pytest.raises(ValueError,match='Playlist backup'):restore(store,manifest['id'])
    assert plain_db.read_bytes()==before
    store.close()


def test_snapshot_contains_wal_commits(tmp_path):
    source=tmp_path/'wal.db'
    conn=sqlite3.connect(source);conn.execute('PRAGMA journal_mode=WAL');conn.execute('CREATE TABLE test(x)');conn.execute('INSERT INTO test VALUES(42)');conn.commit()
    from sqlalchemy import create_engine
    from types import SimpleNamespace
    engine=create_engine(f'sqlite:///{source}')
    dest=tmp_path/'snap.db'
    consistent_backup(SimpleNamespace(engine=engine),dest)
    copy=sqlite3.connect(dest);assert copy.execute('SELECT x FROM test').fetchone()[0]==42
    copy.close();conn.close();engine.dispose()


def test_encrypted_snapshot(tmp_path):
    from sqlalchemy import create_engine
    from sqlcipher3 import dbapi2
    from types import SimpleNamespace
    source=tmp_path/'encrypted.db';key='fixture-key'
    engine=create_engine(f'sqlite+pysqlcipher://:{key}@/{source}',module=dbapi2)
    with engine.begin() as c:c.exec_driver_sql('CREATE TABLE test(x)');c.exec_driver_sql('INSERT INTO test VALUES(7)')
    destination=tmp_path/'encrypted.bak'
    consistent_backup(SimpleNamespace(engine=engine),destination)
    assert destination.read_bytes()[:16]!=b'SQLite format 3\x00'
    c=dbapi2.connect(str(destination));c.execute("PRAGMA key='fixture-key'")
    assert c.execute('SELECT x FROM test').fetchone()[0]==7
    c.close();engine.dispose()


def test_reviewed_playlist_roundtrip_and_restore(tmp_path,song,plain_db):
    from autocue_app.rekordbox import prepare_playlist
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    plan={'schema_version':1,'id':'set-fixture','track_ids':[p['id']],'transition_ids':[],
          'brief':{'target_seconds':24,'tolerance_seconds':0},'stale':False}
    store.artifact('set',plan['id'],plan)
    preview=prepare_playlist(store,plan['id'],'Fixture Set',plain_db,True)
    manifest=apply(store,preview['id'])
    db=open_db(plain_db,True)
    playlist=next(pl for pl in db.get_playlist() if pl.Name=='Fixture Set')
    assert [str(s.ContentID) for s in db.get_playlist_songs(PlaylistID=playlist.ID)]==['42']
    db.close();db.engine.dispose()
    with pytest.raises(ValueError,match='already exists'):prepare_playlist(store,plan['id'],'Fixture Set',plain_db,True)
    restore(store,manifest['id'])
    db=open_db(plain_db,True)
    assert not list(db.get_playlist())
    db.close();db.engine.dispose();store.close()


def test_deletion_only_requires_explicit_review_and_preserves_manual(tmp_path,song,plain_db):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    cue=p['cues'][0];store.review_cue(p['id'],cue['id'],'approved')
    apply(store,prepare(store,[p['id']],plain_db,True)['id'])
    store.review_cue(p['id'],cue['id'],'delete-approved')
    preview=prepare(store,[p['id']],plain_db,True)
    assert preview['changes'][0]['operations'][0]['target'] is None
    apply(store,preview['id'])
    db=open_db(plain_db,True)
    assert [c.Comment for c in db.get_cue(ContentID='42')]==['My manual cue']
    db.close();db.engine.dispose();store.close()


def test_adoption_is_reviewed_local_metadata_only(tmp_path,song,plain_db):
    from autocue_app.rekordbox import prepare_adoption
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved');apply(store,prepare(store,[p['id']],plain_db,True)['id'])
    store.db.execute("DELETE FROM artifacts WHERE kind='written'");store.db.commit()
    assert prepare(store,[p['id']],plain_db,True)['conflicts']
    preview=prepare_adoption(store,p['id'],plain_db,True)
    assert len(preview['cues'])==1 and preview['cues'][0]['Comment']=='A intro'
    before=plain_db.read_bytes();result=apply(store,preview['id'])
    assert result['database_modified'] is False and plain_db.read_bytes()==before
    assert not prepare(store,[p['id']],plain_db,True)['conflicts']
    store.close()


def test_comments_only_commit_and_hot_d_colour(tmp_path,song,plain_db):
    from autocue_core import CUE_SCHEME,HOT_SLOT
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    cue=next(c for c in p['cues'] if c['role']=='D')
    store.review_cue(p['id'],cue['id'],'approved')
    apply(store,prepare(store,[p['id']],plain_db,True,hot=True)['id'])
    preview=prepare(store,[p['id']],plain_db,True,hot=True,comments=True)
    assert preview['changes'] and preview['changes'][0]['operations']==[]
    apply(store,preview['id'])
    db=open_db(plain_db,True)
    d=next(c for c in db.get_cue(ContentID='42') if c.Comment==CUE_SCHEME['D'][0])
    assert d.Kind==HOT_SLOT['D'] and d.Color==CUE_SCHEME['D'][1] and d.InMsec==4000
    assert db.get_content(ID='42').Commnt.startswith('My original notes [Autocue]')
    db.close();db.engine.dispose();store.close()


def test_rekordbox_starting_before_commit_rolls_back(tmp_path,song,plain_db,monkeypatch):
    import autocue_app.rekordbox as rb
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True)
    calls=[]
    def check():
        calls.append(1)
        if len(calls)==3:raise ValueError('Close rekordbox before applying')
    monkeypatch.setattr(rb,'not_running',check)
    with pytest.raises(ValueError,match='Close rekordbox'):apply(store,preview['id'])
    db=open_db(plain_db,True)
    assert [c.Comment for c in db.get_cue(ContentID='42')]==['My manual cue']
    assert db.get_content(ID='42').Commnt=='My original notes'
    assert not store.artifacts('write-manifest')
    db.close();db.engine.dispose();store.close()


def test_failed_track_savepoint_has_no_partial_cues_or_comments(tmp_path,song,plain_db,monkeypatch):
    from pyrekordbox.db6 import Rekordbox6Database
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    for cue in p['cues']:store.review_cue(p['id'],cue['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True,comments=True)
    original=Rekordbox6Database.add
    calls=[]
    def broken_add(self, value):
        calls.append(value)
        if len(calls)==2:raise RuntimeError('Injected second cue failure')
        return original(self,value)
    monkeypatch.setattr(Rekordbox6Database,'add',broken_add)
    manifest=apply(store,preview['id'])
    assert not manifest['written'] and 'Injected' in manifest['failed'][0]['error']
    db=open_db(plain_db,True)
    assert [c.Comment for c in db.get_cue(ContentID='42')]==['My manual cue']
    assert db.get_content(ID='42').Commnt=='My original notes'
    db.close();db.engine.dispose();store.close()


def test_playlist_refuses_rekordbox_starting_before_commit(tmp_path,song,plain_db,monkeypatch):
    import autocue_app.rekordbox as rb
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    plan={'schema_version':1,'id':'playlist-race','track_ids':[p['id']],'transition_ids':[],
          'brief':{'target_seconds':24,'tolerance_seconds':0},'stale':False}
    store.artifact('set',plan['id'],plan)
    preview=rb.prepare_playlist(store,plan['id'],'Interrupted set',plain_db,True)
    calls=[]
    def check():
        calls.append(1)
        if len(calls)==4:raise ValueError('Close rekordbox')
    monkeypatch.setattr(rb,'not_running',check)
    with pytest.raises(ValueError,match='Close rekordbox'):apply(store,preview['id'])
    db=open_db(plain_db,True)
    assert not list(db.get_playlist()) and not list(db.get_playlist_songs())
    db.close();db.engine.dispose();store.close()


def test_encrypted_approved_write_and_complete_restore(tmp_path,song,plain_db,monkeypatch):
    import autocue_app.rekordbox as rb
    from pyrekordbox.db6 import Rekordbox6Database
    from sqlcipher3 import dbapi2
    encrypted=tmp_path/'encrypted-master.db'
    source=dbapi2.connect(str(plain_db))
    source.execute("ATTACH DATABASE ? AS encrypted KEY '402fd-fixture-key'",(str(encrypted),))
    source.execute("SELECT sqlcipher_export('encrypted')")
    source.execute('DETACH DATABASE encrypted');source.close()
    monkeypatch.setattr(rb,'open_db',lambda path=None,plain=False:Rekordbox6Database(path=path,key='402fd-fixture-key',unlock=True))
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    for cue in p['cues']:store.review_cue(p['id'],cue['id'],'approved')
    manifest=apply(store,prepare(store,[p['id']],encrypted,False,comments=True)['id'])
    assert not manifest['failed'] and encrypted.read_bytes()[:16]!=b'SQLite format 3\x00'
    db=rb.open_db(encrypted)
    assert len(list(db.get_cue(ContentID='42')))==4
    db.close();db.engine.dispose()
    restore(store,manifest['id'])
    db=rb.open_db(encrypted)
    assert [c.Comment for c in db.get_cue(ContentID='42')]==['My manual cue']
    assert db.get_content(ID='42').Commnt=='My original notes'
    db.close();db.engine.dispose();store.close()


def test_memory_limit_omits_optional_loop_and_preserves_manual_cues(tmp_path,song,plain_db):
    import datetime as dt
    import uuid
    from pyrekordbox.db6.tables import DjmdCue
    db=open_db(plain_db,True);now=dt.datetime.now()
    for i in range(7):
        db.session.add(DjmdCue(ID=str(100+i),ContentID='42',InMsec=1100+i*100,OutMsec=-1,
            Kind=0,Comment=f'Manual {i}',UUID=str(uuid.uuid4()),created_at=now,updated_at=now))
    db.session.commit();db.close();db.engine.dispose()
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    for cue in p['cues']:store.review_cue(p['id'],cue['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True)
    assert any(c.get('role')=='I' and 'memory limit' in c['reason'] for c in preview['conflicts'])
    assert {o['role'] for o in preview['changes'][0]['operations']}=={'A','D'}
    apply(store,preview['id'])
    db=open_db(plain_db,True);cues=list(db.get_cue(ContentID='42'))
    assert len(cues)==10 and sum('manual' in c.Comment.lower() for c in cues)==8
    db.close();db.engine.dispose();store.close()


@pytest.mark.parametrize('hot',[False,True])
def test_variable_grid_loop_encodes_actual_beats(tmp_path,song,plain_db,hot):
    import numpy as np
    from autocue_core import HOT_SLOT
    data=analysis(song)
    beats=np.r_[0,np.cumsum(np.linspace(.35,.6,48))].tolist()
    data['beats']=beats
    data['cues']=[['L','L end loop',beats[16],beats[32]]]
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],data)
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    preview=prepare(store,[p['id']],plain_db,True,hot=hot)
    assert not preview['conflicts']
    apply(store,preview['id'])
    db=open_db(plain_db,True);loop=next(c for c in db.get_cue(ContentID='42') if c.Comment=='L end loop')
    assert loop.BeatLoopSize==0x10000*16+1
    assert loop.InMsec==round(beats[16]*1000) and loop.OutMsec==round(beats[32]*1000)
    assert loop.Kind==(HOT_SLOT['L'] if hot else 4)
    db.close();db.engine.dispose();store.close()


def test_missing_backup_file_or_future_manifest_refuses_restore_clearly(tmp_path,song,plain_db):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    manifest=apply(store,prepare(store,[p['id']],plain_db,True)['id'])
    before=plain_db.read_bytes()
    store.artifact('write-manifest','future',{**manifest,'id':'future','schema_version':2})
    with pytest.raises(ValueError,match='schema version'):restore(store,'future')
    Path(manifest['backup']).rename(tmp_path/'moved-backup')
    with pytest.raises(ValueError,match='Backup missing'):restore(store,manifest['id'])
    assert plain_db.read_bytes()==before
    store.close()
