import subprocess
import numpy as np


def test_ffmpeg_decode_never_shares_the_services_stdin(tmp_path, monkeypatch):
    # Attached to the app's RPC pipe, ffmpeg did not return after a 2-second file and the
    # job sat in `communicate` for good; it now gets no stdin, -nostdin and a time limit.
    from onset_service import cues
    clip = tmp_path / 'clip.m4a'
    clip.write_bytes(b'not audio')
    seen = {}
    def fake_run(cmd, **kw):
        seen['cmd'] = cmd
        seen.update(kw)
        return subprocess.CompletedProcess(cmd, 0, stdout=np.zeros(4, dtype=np.float32).tobytes(), stderr=b'')
    monkeypatch.setattr(cues.shutil, 'which', lambda name: 'ffmpeg')
    monkeypatch.setattr(cues.subprocess, 'run', fake_run)
    y, sr = cues.load_audio(clip)
    assert len(y) == 4 and sr == 44100
    assert '-nostdin' in seen['cmd']
    assert seen['stdin'] is subprocess.DEVNULL
    assert seen['timeout'] == cues.DECODE_TIMEOUT_S
