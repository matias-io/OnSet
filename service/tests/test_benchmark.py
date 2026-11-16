import pytest
from onset_service.benchmark import timing_metrics,evaluate


def test_timing_matching_never_counts_duplicate_beats_twice():
    result=timing_metrics([0,.01,.5,1],[0,.5,1])
    assert (result['tp'],result['fp'],result['fn'])==(3,1,0)


def test_benchmark_excludes_unverified_and_uses_raw_observations():
    p={'id':'a','duration':4,'key':'C# major','overrides':{'key':'C# major'},'analysis':{'key':{'name':'A minor'},'bpm':120,'beats':[0,.5,1], 'downbeats':[0,2], 'sections':{'drop':0},'structure':{'sections':[{'start':0,'end':2,'label':'intro'},{'start':2,'end':4,'label':'groove'}]}}}
    labels={'records':[{'track_id':'a','verified':True,'events':{'drop':{'state':'present','bar':0}},'keys':['8A'],'bpm':120,'boundaries':[2],'grid_samples':[{'start':0,'end':2,'beats':[0,.5,1]}]}]}
    r=evaluate([p],labels)
    assert r['keys'][0]['primary_correct'] and r['events']['0']['f1']==1 and r['beats'][0]['f1']==1
    labels['records'][0]['verified']=False
    assert evaluate([p],labels)['verified_tracks']==0


def test_transition_verdicts_are_kept_one_per_option_and_reported(tmp_path):
    import pytest
    from onset_service.service import Service
    service=Service(tmp_path/'app')
    try:
        service.store.artifact('transition','opt-1',{'id':'opt-1','track_ids':['a','b'],'technique':'filter_swap'})
        first=service.dispatch('transition-rating',{'schema_version':1,'option_id':'opt-1','verdict':'needs-changes','heard':'render','note':'bass clash at +16s'})
        assert first['technique']=='filter_swap' and first['track_ids']==['a','b']
        service.dispatch('transition-rating',{'schema_version':1,'option_id':'opt-1','verdict':'works','heard':'set'})
        ratings=service.dispatch('benchmark',{})['transition_ratings']
        assert [(r['option_id'],r['verdict'],r['heard']) for r in ratings]==[('opt-1','works','set')]
        for bad in ({'schema_version':1,'option_id':'opt-1','verdict':'great','heard':'set'},{'schema_version':1,'option_id':'missing','verdict':'works','heard':'set'},
                    {'schema_version':1,'option_id':'opt-1','verdict':'works','heard':'radio'},{'schema_version':1,'option_id':'opt-1','verdict':'works','heard':'set','score':5}):
            with pytest.raises(ValueError):service.dispatch('transition-rating',bad)
    finally:service.close()
