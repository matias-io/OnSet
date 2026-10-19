import shutil
import pytest
from onset_service.store import Store
from onset_service.util import match_opinion
from conftest import analysis


def test_audio_duration_is_available_before_analysis(tmp_path,song):
    store=Store(tmp_path/'app')
    imported=store.import_track(song)
    assert imported['analysis'] is None
    assert imported['duration']==pytest.approx(24)
    imported['duration']=0;store.save(imported)
    assert store.import_track(song)['duration']==pytest.approx(24)
    store.close()


def test_identity_reviews_undo_and_staleness(tmp_path,song):
    store=Store(tmp_path/'app')
    p=store.import_track(song)
    alias=tmp_path/'renamed.wav';shutil.copyfile(song,alias)
    assert store.import_track(alias)['id']==p['id']
    p=store.apply_analysis(p['id'],analysis(song))
    original=p['revision'];cue=p['cues'][0]
    p=store.review_cue(p['id'],cue['id'],'approved')
    assert p['cues'][0]['review']=='approved'
    store.edit(p['id'],{'key':'A minor','notes':'Keep this'})
    updated=analysis(song);updated['bpm']=122
    p=store.apply_analysis(p['id'],updated)
    assert p['revision']!=original and p['key']=='A minor' and p['notes']=='Keep this'
    assert p['review_state']=='stale' and p['cues'][0]['review']=='proposed'
    with pytest.raises(ValueError):store.undo(p['id'])
    store.close()


def test_duplicate_basename_is_never_silently_matched():
    opinions={'/a/song.wav':{'drop':'1:00'},'/b/song.wav':{'drop':'2:00'}}
    assert match_opinion(opinions,'C:/music/song.wav') is None
    assert match_opinion(opinions,'/a/song.wav')['drop']=='1:00'


def test_saved_sets_reappear_after_restart_and_show_stale_evidence(tmp_path,song):
    from onset_service.service import Service
    home=tmp_path/'app';service=Service(home)
    track=service.store.import_track(song)
    service.store.apply_analysis(track['id'],analysis(song))
    result=service.plan({'brief':{'schema_version':1,'target_seconds':24,'tolerance_seconds':1}})
    assert result['plans']
    service.close();service=Service(home)
    saved=service.dispatch('sets')
    assert saved and not saved[0].get('stale')
    assert service.import_plan(saved[0])['id']==service.import_plan(saved[0])['id']
    service.store.edit(track['id'],{'key':'C minor'})
    assert service.dispatch('sets')[0]['stale']
    service.close()


def test_changed_audio_alias_does_not_replace_original_decisions(tmp_path,song):
    store=Store(tmp_path/'app')
    original=store.import_track(song)
    alias=tmp_path/'alias.wav';shutil.copyfile(song,alias)
    assert store.import_track(alias)['id']==original['id']
    original=store.apply_analysis(original['id'],analysis(song))
    store.review_cue(original['id'],original['cues'][0]['id'],'approved',.5)
    import soundfile as sf
    audio,sr=sf.read(alias);sf.write(alias,audio*.8,sr)
    changed=store.import_track(alias)
    assert changed['id']!=original['id']
    kept=store.get(original['id'])
    assert kept['locations']==[str(song.resolve())]
    assert kept['cues'][0]['review']=='approved' and kept['cues'][0]['time']==.5
    assert store.import_track(song)['id']==original['id']
    assert store.import_track(alias)['id']==changed['id']
    assert len(store.tracks())==2
    store.close()


def test_cancelled_jobs_recover(tmp_path):
    from onset_service.service import Service
    store=Store(tmp_path/'app')
    store.job({'id':'j','kind':'analyze','state':'running','params':{},'completed':[]})
    store.close()
    service=Service(tmp_path/'app')
    assert service.store.jobs()[0]['state']=='paused'
    service.close()


def test_profile_interchange_preserves_corrections_without_importing_approval(tmp_path,song):
    first=Store(tmp_path/'first');p=first.import_track(song);p=first.apply_analysis(p['id'],analysis(song))
    p=first.edit(p['id'],{'notes':'A personal note','key':'D minor'})
    p=first.review_cue(p['id'],p['cues'][0]['id'],'approved',.5)
    second=Store(tmp_path/'second');imported=second.import_profile(p)
    assert imported['id']==p['id']
    assert imported['notes']=='A personal note' and imported['key']=='D minor'
    assert imported['cues'][0]['time']==.5 and imported['cues'][0]['review']=='proposed'
    imported['fingerprint']='wrong'
    third=Store(tmp_path/'third')
    with pytest.raises(ValueError,match='identity'):third.import_profile(imported)
    first.close();second.close();third.close()


def test_energy_calibration_requires_data_and_uses_judgments(tmp_path):
    from onset_service.service import Service
    service=Service(tmp_path/'app')
    with pytest.raises(ValueError,match='at least three'):service.calibrate_energy({})
    ids=[]
    for i in range(3):
        p=service.store.import_track(str(tmp_path/f'missing-{i}.wav'))
        p['analysis']={'energy_profile':{'raw_summary':i,'acoustic_summary':[float(i)]*5}}
        service.store.save(p);ids.append(p['id'])
    service.energy_judgment({'a':ids[0],'b':ids[2],'winner':ids[2]})
    service.calibrate_energy({})
    assert service.store.get(ids[0])['analysis']['energy_profile']['judgment_count']==1
    assert service.store.get(ids[2])['analysis']['energy_profile']['rating']>service.store.get(ids[0])['analysis']['energy_profile']['rating']
    service.close()


def test_bulk_review_has_one_undo_and_rejects_stale_revision(tmp_path,song):
    store=Store(tmp_path/'app');p=store.import_track(song);p=store.apply_analysis(p['id'],analysis(song))
    ids=[c['id'] for c in p['cues']]
    reviewed=store.review_bulk(p['id'],ids,'approved',p['revision'])
    assert all(c['review']=='approved' for c in reviewed['cues'])
    assert all(c['review']=='proposed' for c in store.undo(p['id'])['cues'])
    with pytest.raises(ValueError,match='Analysis changed'):store.review_bulk(p['id'],ids,'approved','old')
    store.close()


def test_resource_measurements_do_not_invalidate_analysis(tmp_path,song):
    store=Store(tmp_path/'app');p=store.import_track(song);value=analysis(song)
    value['resources']={'peak_process_rss_bytes':100};p=store.apply_analysis(p['id'],value)
    store.review_cue(p['id'],p['cues'][0]['id'],'approved')
    value['resources']['peak_process_rss_bytes']=200
    unchanged=store.apply_analysis(p['id'],value)
    assert unchanged['revision']==p['revision'] and unchanged['cues'][0]['review']=='approved'
    store.close()


def test_missing_profile_relocation_requires_exact_hash(tmp_path,song):
    import shutil
    a=Store(tmp_path/'first');p=a.import_track(song);p=a.apply_analysis(p['id'],analysis(song));a.close()
    moved=tmp_path/'moved.wav';shutil.move(song,moved)
    b=Store(tmp_path/'second');p2=b.import_profile(p)
    assert p2['id']==p['id'] and p2['analysis_state']=='missing'
    wrong=tmp_path/'wrong.wav';wrong.write_bytes(b'wrong')
    with pytest.raises(ValueError,match='exact audio'):b.relocate(p['id'],wrong)
    linked=b.relocate(p['id'],moved)
    assert linked['path']==str(moved) and linked['revision']==p['revision']
    b.close()


def test_fallback_discovery_does_not_save_a_selection(tmp_path,song):
    from onset_service.service import Service
    import soundfile as sf
    service=Service(tmp_path/'app')
    one=service.store.import_track(song);service.store.apply_analysis(one['id'],analysis(song))
    samples,sr=sf.read(song);other=tmp_path/'other.wav';sf.write(other,samples*.7,sr)
    two=service.store.import_track(other);service.store.apply_analysis(two['id'],analysis(other))
    result=service.plan({'brief':{'schema_version':1,'target_seconds':40,'tolerance_seconds':20}})
    plan=next(p for p in result['plans'] if len(p['track_ids'])==2)
    before={p['id'] for p in service.store.artifacts('set')}
    proposals=service.transition_alternatives({'id':plan['id'],'index':0})
    assert proposals
    assert {p['id'] for p in service.store.artifacts('set')}==before
    chosen=service.import_plan(proposals[0]['plan'])
    assert chosen['id'] in {p['id'] for p in service.store.artifacts('set')}
    assert service.import_plan(chosen)['id']==chosen['id']
    service.close()


def test_grid_seed_does_not_block_historical_analysis_import(tmp_path,song):
    from conftest import analysis
    store=Store(tmp_path/'app');p=store.import_track(song)
    p.update(key='8A',analysis={'downbeats':[], 'grid_source':'rekordbox'})
    store.save(p)
    imported=store.import_track(song,analysis(song))
    assert imported['analysis_state']=='imported' and imported['revision']
    assert imported['key']=='8A' and imported['analysis_source']=='legacy-import'
    store.review_cue(imported['id'],imported['cues'][0]['id'],'approved')
    repeated=store.import_track(song,analysis(song))
    assert repeated['cues'][0]['review']=='approved'
    store.close()


def test_key_edits_require_recognisable_notation(tmp_path,song):
    store=Store(tmp_path/'app');p=store.import_track(song)
    with pytest.raises(ValueError,match='Camelot'):store.edit(p['id'],{'key':'H dorian'})
    assert store.edit(p['id'],{'key':'F#m'})['key']=='F#m'
    assert store.edit(p['id'],{'key':'08A'})['key']=='08A'
    store.close()


def test_unsupported_files_and_malformed_workspace_report_failures_without_partial_import(tmp_path,song):
    import json
    from onset_service.service import Service
    service=Service(tmp_path/'app')
    other=tmp_path/'notes.db';other.write_bytes(b'not audio')
    result=service.import_files([str(other)])
    assert result['imported']==0 and 'Unsupported file type' in result['failed'][0]['error']
    profile=service.store.import_track(song)
    exported={'schema_version':1,'kind':'workspace','tracks':[{**profile,'id':'portable-track'}],'transitions':[{'schema_version':2,'id':'t'}]}
    fresh=Service(tmp_path/'second')
    bad=tmp_path/'bad-workspace.json';bad.write_text(json.dumps(exported),encoding='utf-8')
    result=fresh.import_files([str(bad)])
    assert 'schema version' in result['failed'][0]['error'] and not fresh.store.tracks()
    exported['transitions']=[{'schema_version':1}]
    bad.write_text(json.dumps(exported),encoding='utf-8')
    assert 'missing id' in fresh.import_files([str(bad)])['failed'][0]['error'] and not fresh.store.tracks()
    service.close();fresh.close()


def test_set_brief_setting_roundtrips_and_is_validated(tmp_path):
    from onset_service.service import Service
    service=Service(tmp_path/'app')
    saved=service.settings({'key':'set_brief','value':{'schema_version':1,'target_seconds':5400,'mood':'warm'}})
    assert saved['target_seconds']==5400 and saved['mood']=='warm' and saved['tolerance_seconds']==120
    reopened_value=service.settings({})['set_brief']
    assert reopened_value==saved
    for bad in ('text',{'schema_version':2},{'loudness':1},{'smoothness':2},{'target_seconds':-1}):
        with pytest.raises(ValueError):service.settings({'key':'set_brief','value':bad})
    service.close()
