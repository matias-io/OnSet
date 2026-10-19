import datetime as dt
import uuid
import numpy as np
import pytest
import soundfile as sf


@pytest.fixture
def song(tmp_path):
    sr=22050
    t=np.arange(sr*24)/sr
    y=.08*np.sin(2*np.pi*220*t)+.06*np.sin(2*np.pi*261.625*t)+.05*np.sin(2*np.pi*329.628*t)
    beat=np.mod(t,.5)
    y+=.3*np.exp(-beat*40)*np.sin(2*np.pi*60*t)
    y[(t>8)&(t<12)]*=.12
    p=tmp_path/'music.wav'
    sf.write(p,y,sr)
    return p


@pytest.fixture
def plain_db(tmp_path,song,monkeypatch):
    from sqlalchemy import create_engine
    from pyrekordbox.db6 import tables,Rekordbox6Database
    import pyrekordbox.db6.database as database
    monkeypatch.setattr(database,'get_rekordbox_pid',lambda:0)
    monkeypatch.setattr(database,'get_config',lambda *a,**k:{})
    import pyrekordbox.utils as utils
    monkeypatch.setattr(utils,'get_rekordbox_pid',lambda:0)
    for table in tables.Base.metadata.tables.values():
        for col in table.columns:
            if not col.primary_key:col.nullable=True
    p=tmp_path/'master.db'
    engine=create_engine(f'sqlite:///{p}')
    tables.Base.metadata.create_all(engine)
    engine.dispose()
    db=Rekordbox6Database(path=p,unlock=False)
    now=dt.datetime.now()
    db.session.add_all([
        tables.AgentRegistry(registry_id='localUpdateCount',id_1='localUpdateCount',id_2='',int_1=1000,int_2=0,str_1='',str_2='',date_1=now,date_2=now,text_1='',text_2='',created_at=now,updated_at=now),
        tables.DjmdContent(ID='42',FolderPath=str(song),FileNameL=song.name,Title='Fixture',BPM=12000,UUID=str(uuid.uuid4()),Commnt='My original notes',created_at=now,updated_at=now),
        tables.DjmdCue(ID='7',ContentID='42',InMsec=1000,OutMsec=-1,Kind=0,Comment='My manual cue',UUID=str(uuid.uuid4()),created_at=now,updated_at=now),
    ])
    db.session.commit();db.close();db.engine.dispose()
    return p


def analysis(path,duration=24):
    return {'path':str(path),'duration':duration,'bpm':120,'bars':12,'downbeats':list(np.arange(0,duration,2)),
            'beats':list(np.arange(0,duration,.5)),'energy':[-10.]*12,'energy_hi':[-10.]*12,
            'sections':{'drop':2,'breakdown':6,'outro':10,'second_drop':8},'flags':[],
            'cues':[['A','A intro',0,None],['D','D drop',4,None],['I','I intro loop',0,8]]}
