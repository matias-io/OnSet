"""Publish local, reproducible engineering evidence without changing the music library."""
from pathlib import Path
import json
import subprocess
import sys
import uuid
import xml.etree.ElementTree as ET

root=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(root))
from onset_service.service import Service
from onset_service.util import atomic_json,utcnow

out=root/'.artifacts';out.mkdir(exist_ok=True)
test=subprocess.run([sys.executable,'-X','utf8','-m','pytest','-q','-p','no:cacheprovider','--basetemp',str(root/'.test-data'/('quality-tests-'+uuid.uuid4().hex[:12])),'--junitxml',str(out/'test-results.xml')],cwd=root,capture_output=True,text=True,encoding='utf-8')
tree=ET.parse(out/'test-results.xml');suites=list(tree.getroot().iter('testsuite'))
service=Service(root/'.test-data/quality-doctor')
try:diagnostics=service.doctor()
finally:service.close()
previous=out/'quality-report.json'
report={}
if previous.exists():
    report=json.loads(previous.read_text(encoding='utf-8'))
    history=out/'quality-history';history.mkdir(exist_ok=True)
    atomic_json(history/(uuid.uuid4().hex+'.json'),report)
report.update({'schema_version':1,'created':utcnow(),'release':json.loads((root/'package.json').read_text())['version'],'python_tests':{'exit_code':test.returncode,'tests':sum(int(s.get('tests',0)) for s in suites),'failures':sum(int(s.get('failures',0))+int(s.get('errors',0)) for s in suites),'output':test.stdout[-2500:]},'diagnostics':diagnostics,'acceptance_status':'engineering checks only; production gates in docs/IMPLEMENTATION_STATUS.md'})
deep=root/'.test-data/deep-smoke/result.json'
if deep.exists():report['deep_smoke']={k:v.get('state') for k,v in json.loads(deep.read_text())['deep']['models'].items()}
playback=root/'.test-data/desktop-smoke/playback.json'
if playback.exists():report['historical_unready_playback_attempt']=json.loads(playback.read_text())
for name,key in [('desktop-smoke/packaged-playback.json','historical_0_1_packaged_playback'),('planning-smoke.json','planning_smoke'),('planning-full.json','full_library_planning'),('ui-acceptance.json','computer_use_acceptance')]:
    saved=root/'.test-data'/name
    if saved.exists():report[key]=json.loads(saved.read_text())
desktop=next((p for p in [out/'desktop-citations-final.json',out/'desktop-tests-0.2.json'] if p.exists()),None)
if desktop is not None:
    d=json.loads(desktop.read_text());report['desktop_tests']={k:d.get(k) for k in ['numTotalTests','numPassedTests','numFailedTests','success']}
atomic_json(out/'quality-report.json',report)
print(json.dumps(report,indent=2))
raise SystemExit(test.returncode)
