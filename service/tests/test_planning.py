import copy
import pytest
from onset_service.transitions import options, validate_option, windows
from onset_service.planning import build_plans,validate_plan
from onset_service.benchmark import event_metrics


def track(i,bpm=120,key='A minor'):
    return {'id':i,'title':i,'revision':'r1','duration':120,'bpm':bpm,'key':key,'genres':[],'cues':[],
            'analysis':{'downbeats':list(range(0,120,2)),
                        'structure':{'windows':[{'id':'intro','start':0,'end':64,'label':'intro','vocals':False},
                                                {'id':'outro','start':80,'end':120,'label':'outro','vocals':False}]}}}


def test_transition_deck_pitch_and_timing():
    a,b=track('a'),track('b',118)
    b['analysis']['downbeats']=[x*120/118 for x in b['analysis']['downbeats']]
    values=options(a,b)
    assert values
    matching=[v for v in values if v['technique'] in {'eq_swap','long_blend','quick_blend'}]
    assert matching and matching[0]['pitch_pct']==pytest.approx((120/118-1)*100)
    assert matching[0]['wall_seconds']==pytest.approx((matching[0]['in_end']-matching[0]['in_start'])/matching[0]['in_rate'])


def test_key_mismatch_allows_cut():
    values=options(track('a'),track('b',key='C# major'),limit=18)
    assert any(v['technique']=='cut' for v in values)
    assert not any(v['technique']=='long_blend' for v in values)


def test_plan_duration_constraints_and_unknown_ids():
    profiles=[track('a'),track('b'),track('c')]
    brief={'target_seconds':200,'tolerance_seconds':50,'must_play':['a','b'],'start':'a'}
    result=build_plans(profiles,brief)
    assert result['plans']
    lookup={o['id']:o for o in result['transitions']}
    p=result['plans'][0]
    assert validate_plan(p,profiles,lookup,p['brief'])['valid']
    bad=copy.deepcopy(p);bad['transition_ids'][0]='invented'
    with pytest.raises(ValueError):validate_plan(bad,profiles,lookup)
    impossible=build_plans(profiles,{'target_seconds':10,'tolerance_seconds':0,'must_play':['a','b']})
    assert not impossible['plans'] and impossible['conflicts']


def test_metrics_include_inventions_and_misses():
    metrics=event_metrics({'a':{'drop':4,'outro':20}},{'a':{'drop':{'state':'present','bar':4},'breakdown':{'state':'present','bar':12},'outro':{'state':'absent'}}})
    assert (metrics['tp'],metrics['fp'],metrics['fn'])==(1,1,1)


def test_loop_extension_is_included_in_performed_duration():
    a,b=track('a'),track('b')
    a['analysis']['structure']['windows']=[]
    a['analysis']['structure']['loop_candidates']=[{'id':'loop','start':104,'end':112,'bars':4,'quality':.8}]
    opts=options(a,b,{'techniques':['loop_blend'],'equipment':['loop','eq'],'skill':3})
    assert opts
    o=opts[0]
    assert o['out_loop']['repetitions']>1
    assert validate_option(o,{'a':a,'b':b})
    assert o['wall_seconds']==pytest.approx((o['out_end']-o['out_start'])+o['loop_extra_seconds'])
    plan={'schema_version':1,'track_ids':['a','b'],'transition_ids':[o['id']]}
    result=validate_plan(plan,[a,b],{o['id']:o})
    assert result['duration_seconds']==pytest.approx(o['out_end']+o['loop_extra_seconds']+b['duration']-o['in_end'])
    bad=copy.deepcopy(o);bad['in_end']+=1
    with pytest.raises(ValueError,match='Incoming timing'):validate_option(bad,{'a':a,'b':b})


def test_loops_do_not_displace_every_natural_window():
    a=track('a')
    a['analysis']['structure']['loop_candidates']=[{'id':str(i),'start':110+i/10,'end':119} for i in range(20)]
    assert any(w['id']=='outro' for w in windows(a,True))


def test_half_time_bridge_keeps_deck_pitch_within_limit():
    a,b=track('a',120),track('b',60)
    b['analysis']['downbeats']=list(range(0,120,4))
    values=options(a,b,{'techniques':['tempo_bridge'],'equipment':['tempo'],'skill':3})
    assert values and values[0]['tempo_factor']==2
    assert values[0]['pitch_pct']==0
    assert validate_option(values[0],{'a':a,'b':b})


def test_first_groove_cue_is_not_assumed_to_be_a_drop():
    a,b=track('a'),track('b')
    b['cues']=[{'id':'d','role':'D','time':4,'review':'approved'}]
    values=options(a,b,{'techniques':['drop_swap'],'skill':3})
    assert not values


def test_manual_key_override_wins_and_entire_overlap_is_checked():
    from onset_service.transitions import local_key,overlap_harmony
    a,b=track('a'),track('b')
    b['analysis']['local_keys']=[{'start':0,'end':16,'name':'A minor'},{'start':16,'end':120,'name':'C# major'}]
    assert overlap_harmony(a,b,0,32,0,32,1,1,True)<.7
    b['overrides']={'key':'D minor'};b['key']='D minor'
    assert local_key(b,4)=='D minor'


def test_stale_single_track_set_and_key_lock_mismatch_are_rejected():
    a,b=track('a'),track('b')
    with pytest.raises(ValueError,match='stale'):validate_plan({'schema_version':1,'track_ids':['a'],'transition_ids':[],'stale':True},[a],{})
    o=options(a,b)[0]
    plan={'schema_version':1,'track_ids':['a','b'],'transition_ids':[o['id']]}
    with pytest.raises(ValueError,match='key-lock'):validate_plan(plan,[a,b],{o['id']:o},{'target_seconds':200,'tolerance_seconds':100,'key_lock':False})


@pytest.mark.parametrize('bpms',[(120,120,120),(120,118,122)])
def test_multitrack_adviser_context_and_import_preserve_constraints(tmp_path, song, bpms):
    from onset_service.service import Service
    service=Service(tmp_path/'app',lambda e:None)
    profiles=[]
    for i in range(3):
        path=tmp_path/f'track-{i}.wav';path.write_bytes(song.read_bytes()+bytes([i]))
        p=service.store.import_track(path)
        generated=track(p['id'],bpms[i]);generated['analysis']['downbeats']=[x*120/bpms[i] for x in generated['analysis']['downbeats'] if x*120/bpms[i]<120]
        generated['analysis']['structure']['windows'][1]['start']=40*240/bpms[i]
        p.update({k:v for k,v in generated.items() if k!='id'})
        service.store.save(p);profiles.append(p)
    ids=[p['id'] for p in profiles]
    brief={'track_ids':ids,'must_play':ids,'target_seconds':300,'tolerance_seconds':80}
    if len(set(bpms))>1:brief['techniques']=['quick_blend']
    context=service.ai_context({'track_ids':ids,'brief':brief})
    pair=[o for o in context['transitions'] if o['track_ids']==ids[:2]]
    assert len({o['technique'] for o in pair})>=(2 if len(set(bpms))==1 else 1)
    assert context['validated_routes']
    exposed={o['id'] for o in context['transitions']}
    assert all(set(route['transition_ids'])<=exposed for route in context['validated_routes'])
    if len(set(bpms))>1:assert any(o['out_rate']!=1 for o in context['transitions'])
    assert all(o['actions'] and 'pitch_pct' in o for o in pair)
    local=service.plan({'brief':brief})
    assert local['plans']
    proposal=local['plans'][0]
    saved=service.import_plan(proposal)
    assert len(saved['track_ids'])==3 and len(saved['transition_ids'])==2
    assert service.validate_plan(saved)['valid']
    # Validation and the saved-set list look up only the transitions a plan names; loading
    # every stored transition took close to a minute per set on a real library.
    real_artifacts=service.store.artifacts
    def no_full_transition_load(kind,*args,**kwargs):
        assert kind!='transition','loaded every stored transition'
        return real_artifacts(kind,*args,**kwargs)
    service.store.artifacts=no_full_transition_load
    assert service.validate_plan(saved)['valid']
    assert any(s['id']==saved['id'] and not s.get('stale') for s in service.saved_sets())
    service.store.artifacts=real_artifacts
    service.store.edit(ids[1],{'key':'C# major'})
    with pytest.raises(ValueError,match='stale|changed'):
        service.validate_plan(service.store.artifact('set',saved['id']))
    service.close()


def test_late_entry_searches_later_exit_windows():
    # B's intro outranks its outro as an exit, but the set enters B after the intro starts.
    a,b,c=track('a'),track('b'),track('c')
    b['analysis']['structure']['windows'][1]['vocals']='unknown'
    assert all(o['out_start']==0 for o in options(b,c))
    later=options(b,c,min_out_start=32.)
    assert later and all(o['out_start']>=32 for o in later)
    result=build_plans([a,b,c],{'target_seconds':300,'tolerance_seconds':200,'must_play':['a','b','c'],'start':'a','finish':'c'})
    assert result['plans'] and result['plans'][0]['track_ids']==['a','b','c']
    lookup={o['id']:o for o in result['transitions']}
    assert validate_plan(result['plans'][0],[a,b,c],lookup)['valid']


def test_unsatisfiable_briefs_explain_specific_causes():
    profiles=[track('a'),track('b'),track('c')]
    excluded=build_plans(profiles,{'target_seconds':200,'tolerance_seconds':50,'start':'a','excluded':['a']})
    assert any('Opening track' in c for c in excluded['conflicts'])
    short=build_plans(profiles,{'target_seconds':3600,'tolerance_seconds':60})
    assert not short['plans'] and any('shorter than the requested minimum' in c for c in short['conflicts'])
    pinned=build_plans(profiles,{'target_seconds':3600,'tolerance_seconds':60,'pinned':[{'track_id':'missing','position':0}]})
    assert any('Pinned track missing' in c for c in pinned['conflicts'])
