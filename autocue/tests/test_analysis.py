import numpy as np
from autocue_app.store import Store
from autocue_app.analysis import analyse
from conftest import analysis


def test_analysis_uses_grid_and_retains_evidence(tmp_path,song):
    store=Store(tmp_path/'app');p=store.import_track(song)
    store.apply_analysis(p['id'],analysis(song));p=store.get(p['id'])
    result=analyse(p,store.home,deep=False)
    assert result['downbeats']==analysis(song)['downbeats']
    assert result['key']['alternatives'] and result['structure']['sections']
    assert result['energy_profile']['rating'] is None
    assert result['features']['path'].endswith('.npz')
    assert result['spectrogram']['frequencies'][0]==0
    assert not result['deep']
    store.close()


def test_interrupted_deep_analysis_reuses_native_stage_and_repairs_missing_stems(tmp_path,song,monkeypatch):
    import pytest
    import autocue_core
    import autocue_app.deep as deep
    from autocue_app.service import Cancelled
    from pathlib import Path
    store=Store(tmp_path/'app');p=store.import_track(song)
    def interrupted(*args,**kwargs):raise Cancelled()
    monkeypatch.setattr(deep,'run_deep',interrupted)
    with pytest.raises(Cancelled):analyse(p,store.home,deep=True)
    stage=next((store.home/'features'/'native-stages').glob('*/stage.json'))
    assert stage.is_file()
    def no_decode(*args,**kwargs):raise AssertionError('Decoded again after native stage completed')
    monkeypatch.setattr(autocue_core,'load_audio',no_decode)
    stem=tmp_path/'bass.wav';calls=[]
    def completed(*args,**kwargs):
        calls.append(1);stem.write_bytes(b'fixture stem')
        return {'models':{name:{'state':'complete','data':{}} for name in ('beat_this','demucs','essentia')},'stems':{'bass':str(stem)}}
    monkeypatch.setattr(deep,'run_deep',completed)
    resumed=analyse(p,store.home,deep=True)
    assert resumed['cache_hit'] and resumed['state']=='complete' and resumed['analysis_mode']=='deep'
    stem.unlink()
    repaired=analyse(p,store.home,deep=True)
    assert repaired['cache_hit'] and stem.is_file() and len(calls)==2
    store.close()


def test_native_cache_checksum_failure_recomputes_features(tmp_path,song,monkeypatch):
    import json
    from pathlib import Path
    store=Store(tmp_path/'app');p=store.import_track(song)
    first=analyse(p,store.home,deep=False)
    # Remove only the resolved result to exercise the native checkpoint independently.
    feature=Path(first['features']['path']);(feature.parent/'result.json').unlink()
    stage=next((store.home/'features'/'native-stages').glob('*/features.npz'))
    stage.write_bytes(b'interrupted cache write')
    rerun=analyse(p,store.home,deep=False)
    assert not rerun['cache_hit'] and rerun['state']=='complete'
    assert rerun['downbeats']==first['downbeats']
    store.close()


def test_silent_audio_requires_grid_and_never_claims_tonality(tmp_path):
    import pytest
    import soundfile as sf
    path=tmp_path/'silence.wav';sf.write(path,np.zeros(22050*8),22050)
    store=Store(tmp_path/'app');p=store.import_track(path)
    with pytest.raises(ValueError,match='Insufficient rhythmic evidence'):
        analyse(p,store.home,deep=False)
    p['analysis']={'downbeats':[0,2,4,6],'beats':list(np.arange(0,8,.5)),'bpm':120,'grid_source':'manual'}
    result=analyse(p,store.home,deep=False)
    assert result['key']['state']=='unknown'
    assert result['grid_observation']['downbeat_confidence']=='unverified'
    assert result['energy_profile']['rating'] is None
    assert all(np.isfinite(v) for v in result['energy'])
    assert not result['cues'] and all(v is None for v in result['sections'].values())
    assert all(e['state']=='absent' for e in result['events'])
    assert {s['label'] for s in result['structure']['sections']}=={'silence'}
    assert not result['structure']['windows']
    store.close()


def test_changed_audio_refuses_cached_analysis(tmp_path,song):
    import pytest
    store=Store(tmp_path/'app');p=store.import_track(song)
    # Identity must be checked before returning any cached result.
    song.write_bytes(song.read_bytes()+b'changed')
    with pytest.raises(ValueError,match='Audio changed'):
        analyse(p,store.home,deep=False)
    store.close()
