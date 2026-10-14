"""Exercise autocue_rb against the synthetic DB, faking the ANLZ read with madmom's grid."""
import sys
p = "testdb/master.db"
import autocue_rb, autocue
def fake_grid(db, content):
    downbeats, bpm, first = autocue.beat_grid(content.FolderPath)
    phrases = [("Intro", downbeats[0]), ("Up", downbeats[16]), ("Chorus", downbeats[24]), ("Down", downbeats[40]), ("Chorus", downbeats[64]), ("END", downbeats[-1])]
    return downbeats, None, phrases, "High"
autocue_rb.read_grid_and_phrases = fake_grid

for argv in (["x", "--db", p, "--plain", "--playlist", "Test PL", "--dry-run"],
             ["x", "--db", p, "--plain", "--playlist", "Test PL"],
             ["x", "--db", p, "--plain", "--folder", "/home/arcanist/Downloads/2025 Top DJ Lineup", "--prefer-phrases"]):
    print("\n$", " ".join(argv[1:])); sys.argv = argv
    autocue_rb.main()

from pyrekordbox.db6 import tables, Rekordbox6Database
db = Rekordbox6Database(path=p, unlock=False)
print("\ncues in db after two real runs:")
for cue in db.get_cue(ContentID="42").order_by(tables.DjmdCue.InMsec):
    print(f"  {cue.InMsec/1000:7.2f}s frame={cue.InFrame} kind={cue.Kind} color={cue.Color} usn={cue.rb_local_usn} {cue.Comment!r} {cue.UUID[:19]}")
print("localUpdateCount:", db.get_local_usn())
