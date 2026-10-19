"""Event-specific reliability; correlated observations share one evidence family."""
from collections import defaultdict
import math


def learn_reliability(examples):
    counts=defaultdict(lambda:[0,0])
    for row in examples:
        if not row.get('verified'): continue
        group=(row['source'],row['event']);counts[group][1]+=1
        if row['truth_state']=='absent': counts[group][0]+=row.get('predicted_bar') is None
        elif row.get('truth_bar') is not None and row.get('predicted_bar') is not None:
            counts[group][0]+=abs(row['truth_bar']-row['predicted_bar'])<=1
    result={}
    for (source,event),(success,n) in counts.items():
        z=1.96;p=success/n
        lower=(p+z*z/(2*n)-z*math.sqrt(p*(1-p)/n+z*z/(4*n*n)))/(1+z*z/n)
        result[source+':'+event]={'examples':n,'correct':success,'weight':max(.05,lower),'calibrated':False}
    return result


def resolve(observations,reliability=None,tolerance_bars=1):
    reliability=reliability or {};groups=defaultdict(list)
    for o in observations:
        if o.get('state') in {'estimated','present','absent'}: groups[o['event']].append(o)
    results={}
    for event,items in groups.items():
        def support(candidate):
            families={}
            for item in items:
                if candidate.get('bar') is None or item.get('bar') is None:
                    agrees=candidate.get('state')=='absent' and item.get('state')=='absent'
                else: agrees=abs(candidate['bar']-item['bar'])<=tolerance_bars
                if agrees:
                    family=item.get('family',item['source'])
                    weight=reliability.get(item['source']+':'+event,{}).get('weight',.5)
                    families[family]=max(families.get(family,0),weight)
            return sum(families.values())
        best=max(items,key=lambda o:(support(o),o.get('source')=='energy-baseline'))
        results[event]={**best,'support':support(best),'confidence':'unverified','alternatives':[o for o in items if o is not best],
                        'meaning_uncertain':len({o.get('state') for o in items})>1,
                        'timing_uncertainty_bars':max((abs(best['bar']-o['bar']) for o in items if best.get('bar') is not None and o.get('bar') is not None),default=0)}
    return results
