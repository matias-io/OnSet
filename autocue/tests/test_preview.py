import numpy as np
import soundfile as sf
import pytest
from autocue_app.preview import render


def test_local_cut_preview_duration_clipping_and_cache(tmp_path,song):
    tracks=[{'id':i,'path':str(song),'revision':'r','duration':24} for i in ('a','b')]
    option={'schema_version':1,'id':'cut-test','track_ids':['a','b'],'revisions':['r','r'],
            'out_start':4.,'out_end':4.,'in_start':8.,'in_end':8.,'out_rate':1.,'in_rate':1.,
            'wall_seconds':0.,'key_lock':True,'technique':'cut','actions':[],
            'preview_supported':True,'target_bpm':120}
    result=render(option,tracks,tmp_path,click=True)
    y,sr=sf.read(result['path'])
    assert len(y)/sr==12
    assert np.max(np.abs(y))<=.951 and np.max(np.abs(y))>0
    assert result['transition_offset']==4
    assert render(option,tracks,tmp_path,click=True)['cached']
    assert render(option,tracks,tmp_path,click=True)['duration_seconds']==12


def test_tempo_adjusted_outgoing_never_resumes_after_handover(tmp_path):
    sr=44100;t=np.arange(sr*12)/sr
    outgoing=tmp_path/'out.wav';incoming=tmp_path/'in.wav'
    sf.write(outgoing,.2*np.sin(2*np.pi*220*t),sr);sf.write(incoming,np.zeros_like(t),sr)
    tracks=[{'id':'a','path':str(outgoing),'revision':'r','duration':12},{'id':'b','path':str(incoming),'revision':'r','duration':12}]
    option={'schema_version':1,'id':'rate-test','track_ids':['a','b'],'revisions':['r','r'],'out_start':2.4,'out_end':4.8,'in_start':0,'in_end':2,'out_rate':1.2,'in_rate':1.,'wall_seconds':2.,'key_lock':True,'technique':'quick_blend','actions':[],'preview_supported':True}
    result=render(option,tracks,tmp_path)
    y,sr=sf.read(result['path']);handover=result['transition_offset']+2
    assert np.max(abs(y[int((handover+.1)*sr):]))<.0001
    assert np.max(abs(y[:int(handover*sr)]))>.01


@pytest.mark.parametrize('technique',['long_blend','quick_blend','eq_swap','filter','cut','drop_swap','breakdown','echo_out','reverb_out','loop_blend','fade'])
def test_supported_preview_families_are_finite_and_timed(tmp_path,song,technique):
    tracks=[{'id':i,'path':str(song),'revision':'r','duration':24} for i in ('a','b')]
    wall=0. if technique in {'cut','drop_swap'} else 4.
    option={'schema_version':1,'id':technique,'track_ids':['a','b'],'revisions':['r','r'],
            'out_start':4.,'out_end':4.+wall,'in_start':0.,'in_end':wall,'out_rate':1.,'in_rate':1.,
            'wall_seconds':wall,'key_lock':True,'technique':technique,'actions':[{'at':0,'action':'Fixture entry'}],
            'preview_supported':True,'target_bpm':120}
    if technique=='loop_blend':
        option.update(out_end=6.,loop_extra_seconds=2.,out_loop={'start':4.,'end':6.,'repetitions':2})
    result=render(option,tracks,tmp_path,click=True)
    y,sr=sf.read(result['path'])
    assert len(y)/sr==pytest.approx(12+wall)
    assert np.all(np.isfinite(y)) and 0<np.max(abs(y))<=.951
    assert result['transition_offset']==4 and result['actions']==option['actions']


def test_preview_refuses_changed_audio_even_with_a_cached_render(tmp_path,song):
    from autocue_app.util import file_hash
    tracks=[{'id':i,'path':str(song),'fingerprint':file_hash(song),'revision':'r','duration':24} for i in ('a','b')]
    option={'schema_version':1,'id':'identity','track_ids':['a','b'],'revisions':['r','r'],
            'out_start':4.,'out_end':4.,'in_start':0.,'in_end':0.,'out_rate':1.,'in_rate':1.,
            'wall_seconds':0.,'key_lock':True,'technique':'cut','actions':[],'preview_supported':True}
    render(option,tracks,tmp_path)
    song.write_bytes(song.read_bytes()+b'changed')
    with pytest.raises(ValueError,match='Audio changed'):render(option,tracks,tmp_path)
