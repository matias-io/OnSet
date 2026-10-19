from pathlib import Path
import json,subprocess,sys
root=Path(__file__).resolve().parent.parent;sys.path.insert(0,str(root))
from onset_service.deep import wsl_path
from onset_service.util import atomic_json
linux_home=subprocess.check_output(['wsl.exe','--exec','sh','-lc','printf %s "$HOME"'],text=True).strip()
name=sys.argv[1]
if name not in ('edmformer','allin1'):raise ValueError('Unknown experimental model')
home=root/'.test-data/experimental-smoke';home.mkdir(parents=True,exist_ok=True)
request={'schema_version':1,'isolated':True,'models':[name], 'audio':wsl_path(root/'.test-data/deep-smoke/fixture.wav'),
         'cache':wsl_path(root/'.test-data/deep-smoke/models'),'output':wsl_path(home/name)}
with (home/(name+'.jsonl')).open('w',encoding='utf-8') as protocol,(home/(name+'.log')).open('w',encoding='utf-8') as log:
    process=subprocess.run(['wsl.exe','--exec',f'{linux_home}/.local/share/onset/experimental/{name}/bin/python',wsl_path(root/'worker/deep_worker.py')],
                           input=json.dumps(request)+'\n',stdout=protocol,stderr=log,text=True,encoding='utf-8')
events=[]
for line in (home/(name+'.jsonl')).read_text(encoding='utf-8').splitlines():
    try:events.append(json.loads(line))
    except ValueError:pass
result=next((e['result'] for e in reversed(events) if e.get('type')=='result'),{'models':{name:{'state':'failed','error':'No result; inspect local log'}}})
atomic_json(home/(name+'-result.json'),result)
print(json.dumps({'exit_code':process.returncode,'models':{k:{'state':v['state'],'error':v.get('error')} for k,v in result['models'].items()}}))
raise SystemExit(0 if all(v['state']=='complete' for v in result['models'].values()) else 1)
