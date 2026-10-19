from pathlib import Path
import sys
import numpy as np
import soundfile as sf
from onset_service.store import Store
from onset_service.analysis import analyse
from onset_service.util import atomic_json

home=Path('.test-data/deep-smoke').resolve();home.mkdir(parents=True,exist_ok=True)
sr=22050;t=np.arange(sr*24)/sr
y=.08*np.sin(2*np.pi*220*t)+.3*np.exp(-np.mod(t,.5)*40)*np.sin(2*np.pi*60*t)
path=home/'fixture.wav';sf.write(path,y,sr)
store=Store(home);p=store.import_track(path)
p['analysis']={'downbeats':np.arange(0,24,2).tolist(),'beats':np.arange(0,24,.5).tolist(),'bpm':120,'grid_source':'fixture'}
store.save(p)
last=[None]
def progress(stage,v):
    if stage!=last[0]:print(stage,round(v*100),flush=True);last[0]=stage
a=analyse(p,home,True,progress,sys.argv[1:])
store.apply_analysis(p['id'],a)
atomic_json(home/'result.json',a)
print({name:value['state'] for name,value in a['deep']['models'].items()})
store.close()

raise SystemExit(0 if a["state"]=="complete" and all(v["state"]=="complete" for v in a["deep"]["models"].values()) else 1)
