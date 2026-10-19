"""Build a plain-SQLite master.db with pyrekordbox's schema for testing cuewriter."""
import datetime as dt, sys, uuid
from sqlalchemy import create_engine
from pyrekordbox.db6 import tables, Rekordbox6Database
p, track = sys.argv[1], sys.argv[2]
for t in tables.Base.metadata.tables.values():      # fixture only: relax NOT NULL so sparse rows insert
    for col in t.columns:
        if not col.primary_key:
            col.nullable = True
tables.Base.metadata.create_all(create_engine(f"sqlite:///{p}"))
db = Rekordbox6Database(path=p, unlock=False)
now = dt.datetime.now()
u = lambda: str(uuid.uuid4())
db.session.add_all([
    tables.AgentRegistry(registry_id="localUpdateCount", id_1="localUpdateCount", id_2="", int_1=1000, int_2=0, str_1="", str_2="", date_1=now, date_2=now, text_1="", text_2="", created_at=now, updated_at=now),
    tables.DjmdPlaylist(ID="1", Name="Test PL", Seq=1, Attribute=0, ParentID="root", UUID=u(), created_at=now, updated_at=now),
    tables.DjmdContent(ID="42", FolderPath=track, FileNameL="FISHER.flac", Title="STAY", BPM=12770, UUID=u(), created_at=now, updated_at=now),
    tables.DjmdCue(ID="7", ContentID="42", InMsec=1000, InFrame=150, Kind=0, Color=-1, Comment="my manual cue", UUID=u(), created_at=now, updated_at=now),
    tables.DjmdSongPlaylist(ID="9", PlaylistID="1", ContentID="42", TrackNo=1, UUID=u(), created_at=now, updated_at=now),
])
db.session.commit(); db.close()
print("test db ready")
