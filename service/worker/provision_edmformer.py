"""Fetch pinned official assets into the isolated worker's user-owned model cache."""
import hashlib,json,os,tarfile,urllib.request,fcntl
from pathlib import Path

REVISION="2dd942f2f9e71ffd826346828eeaba1dd3ece56a"
ASSETS={"model.pt":("1412e207645e9a71adc09777714dd251ce7805cada9bf19518d2e455a977e165",417979984),
        "pretrained_msd.pt":("218b483a0256ddef736267425fabb166fd97008983696bb9270def464b47bded",1316802088)}

def fetch(url,destination,expected=None):
    destination.parent.mkdir(parents=True,exist_ok=True)
    def checksum(path):
        h=hashlib.sha256()
        with path.open('rb') as f:
            for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
        return h.hexdigest()
    if destination.exists() and (not expected or checksum(destination)==expected):return checksum(destination)
    temporary=destination.with_suffix(destination.suffix+'.part')
    with urllib.request.urlopen(url,timeout=120) as response,temporary.open('wb') as out:
        while block:=response.read(1024*1024):out.write(block)
    actual=checksum(temporary)
    if expected and actual!=expected:raise RuntimeError('Asset checksum mismatch: '+destination.name)
    temporary.replace(destination);return actual

def provision_unlocked(root):
    root=Path(root);records={}
    for name,(checksum,size) in ASSETS.items():
        target=root/'data/checkpoints'/name
        records[name]={'sha256':fetch(f'https://media.githubusercontent.com/media/25ohms/EDM-98/{REVISION}/data/checkpoints/{name}',target,checksum),'size':size}
    for name in ('data/checkpoints/msd_stats.json','configs/edmformer.yaml'):
        records[name]={'sha256':fetch(f'https://raw.githubusercontent.com/25ohms/EDM-98/{REVISION}/{name}',root/name)}
    music_revision='b83ebedb401bcef639b26b05c0c8bee1dc2dfe71'
    archive=root/'musicfm.tar.gz'
    records['musicfm']={'revision':music_revision,'sha256':fetch(f'https://api.github.com/repos/minzwon/musicfm/tarball/{music_revision}',archive)}
    source=root/'musicfm'
    previous=root/'musicfm-source'
    if previous.exists() and not source.exists():previous.rename(source)
    if not source.exists():
        with tarfile.open(archive) as bundle:
            for member in bundle.getmembers():
                if not member.isfile():continue
                parts=Path(member.name).parts[1:]
                if not parts:continue
                dest=source.joinpath(*parts).resolve()
                if not dest.is_relative_to(source.resolve()):raise ValueError('Invalid model source archive')
                dest.parent.mkdir(parents=True,exist_ok=True)
                with bundle.extractfile(member) as f:dest.write_bytes(f.read())
    temporary=root/'assets.json.tmp';temporary.write_text(json.dumps({'schema_version':1,'edm98_revision':REVISION,'records':records},indent=2));temporary.replace(root/'assets.json')
    print(json.dumps({'assets':str(root),'ready':True}),flush=True)

def provision(root):
    root=Path(root);root.mkdir(parents=True,exist_ok=True)
    with (root/'.provision.lock').open('a') as lock:
        fcntl.flock(lock.fileno(),fcntl.LOCK_EX)
        return provision_unlocked(root)

if __name__=='__main__':
    import sys
    provision(sys.argv[1])
