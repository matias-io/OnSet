import pytest
from autocue_app.benchmark import timing_metrics,evaluate


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
