import numpy as np
import pytest
from onset_service.cues import derive_cues,merge_llm
from onset_service.tonal import camelot,estimate_key,sounding_key
from onset_service.mixplan import bpm_move,score_pair


def test_bar_zero_and_variable_grid():
    db=np.cumsum(np.r_[0,np.linspace(1.5,2.5,79)])
    cues=derive_cues({'drop':32},db,intro_loop_bars=8,end_loop_bars=8,duration=db[-1]+1)
    by={c[0]:c for c in cues}
    assert by['B'][2]==0
    assert by['I'][3]==db[8]
    loop=by['L'];start=int(np.argmin(abs(db-loop[2])))
    assert loop[3]==db[start+8]
    assert derive_cues({'drop':16},db)[1][0]=='C'


def test_invalid_grid_and_loop():
    with pytest.raises(ValueError):derive_cues({},[0,1,1])
    with pytest.raises(ValueError):derive_cues({},[0,2,4],end_loop_bars=-1)


def test_missing_opinion_still_generates_optional():
    r={'sections':{'drop':16,'second_drop':32},'flags':[],'duration':120}
    merge_llm(r,None,np.arange(0,120,2),second_drop=True,end_loop_bars=8)
    assert {'G','L'}<={c[0] for c in r['cues']}


def test_pitch_and_keys():
    assert bpm_move(128,126)[2]==pytest.approx((128/126-1)*100)
    assert score_pair({'bpm':128},{'bpm':128})[0]>0
    assert camelot('A minor')=='8A'
    assert camelot('Db major')==camelot('C# major')=='3B'
    assert camelot('13A') is None
    assert sounding_key('A minor',2,False)['camelot']=='8A'
    assert estimate_key(np.zeros(12))['state']=='unknown'


def test_xml_does_not_import_madmom(tmp_path):
    from tools import cues_xml
    out=tmp_path/'empty.xml'
    cues_xml.write_xml([],out)
    assert 'Entries="0"' in out.read_text()


def test_key_template_transposition_and_weak_tonality():
    from onset_service.tonal import PROFILES,NOTES,compatibility
    for mode,template in PROFILES.items():
        for semitones,note in enumerate(NOTES):
            estimate=estimate_key(np.roll(template,semitones),tuning_cents=-17)
            assert estimate['name']==f'{note} {mode}'
            assert estimate['tuning_cents']==-17
            assert estimate['confidence']=='unverified'
    weak=estimate_key(np.ones(12)+np.eye(12)[0]*.001)
    assert weak['state']=='weak' and compatibility(weak,'C major') is None
    assert sounding_key('A minor',2**(1/12),False)['camelot']==camelot('Bb minor')
    assert sounding_key('A minor',2**(-1/12),False)['camelot']==camelot('Ab minor')
    for values in ([float('nan')]*12,[float('inf')]*12,[0]*11):
        with pytest.raises(ValueError):estimate_key(values)


def test_silent_baseline_never_falls_back_to_bar_zero_groove():
    from onset_service.cues import find_sections
    result=find_sections(np.full(64,-100.))
    assert all(result[k] is None for k in ('drop','breakdown','outro','second_drop'))
