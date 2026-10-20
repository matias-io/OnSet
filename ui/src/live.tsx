import {StrictMode, useEffect, useState} from 'react';
import {createRoot} from 'react-dom/client';
import './live.css';
import {Logo} from './Logo';
import {barsAndBeats, clock, cueName, structureLabel} from './liveFormat';

/** One `onset:live` event from OnSet's engine (crates/onset-desktop/src/live.rs). */
export type LiveState = {
  state: 'waiting' | 'idle' | 'unidentified' | 'running' | 'error';
  status: string;
  track: {title: string; artist: string; key: string | null; bpm: number | null; rekordbox_id: number} | null;
  playing: boolean;
  playhead_s: number;
  bpm: number;
  beat_phase: number;
  bar_phase: number;
  section: string | null;
  next_section: string | null;
  beats_to_next_section: number | null;
  drop_countdown_beats: number | null;
  structure_source: string | null;
  next_cue: {slot: number; seconds: number} | null;
};

type Onset = {
  onLive: (callback: (s: LiveState) => void) => () => void;
  setLiveOnTop: (onTop: boolean) => Promise<void>;
  /** Starts the full-screen visuals (`onset.exe` beside the app): `started` or `running`. */
  startVisuals: () => Promise<'started' | 'running'>;
};
const onset = (window as unknown as {onset?: Onset}).onset;

function Live() {
  const [live, setLive] = useState<LiveState | null>(null);
  const [onTop, setOnTop] = useState(false);
  const [visuals, setVisuals] = useState<string | null>(null);
  useEffect(() => onset?.onLive(setLive), []);

  if (!onset) return <main className="live"><p className="live-status">The live view needs the OnSet desktop app.</p></main>;
  if (!live) return <main className="live"><p className="live-status">Starting the live link…</p></main>;

  const beatInBar = Math.min(3, Math.floor(live.bar_phase * 4));
  const t = live.track;
  return (
    <main className="live">
      <header className="live-head">
        <Logo size={20} />
        <span className={`live-dot ${live.state}`} aria-hidden="true" />
        <span className="live-status">{live.status}</span>
        <button type="button" className="live-visuals" title={visuals ?? 'Open the full-screen visuals'}
          onClick={() => {
            onset.startVisuals()
              .then(r => setVisuals(r === 'running' ? 'The visuals are already open' : 'Visuals started'))
              .catch((e: unknown) => setVisuals(String(e)));
          }}>
          Start visuals
        </button>
        <label className="live-ontop">
          <input type="checkbox" checked={onTop}
            onChange={e => {setOnTop(e.target.checked); void onset.setLiveOnTop(e.target.checked);}} />
          Keep on top
        </label>
      </header>

      {t ? (
        <section className="live-track">
          <h1>{t.title}</h1>
          <p>{t.artist}</p>
          <p className="live-meta">
            <span>{live.bpm ? `${live.bpm.toFixed(1)} BPM` : '— BPM'}</span>
            <span>{t.key ?? '— key'}</span>
            <span>{clock(live.playhead_s)}{live.playing ? '' : ' · paused'}</span>
            {live.structure_source && <span className={`live-source ${structureLabel(live.structure_source).toLowerCase()}`}>
              Structure: {structureLabel(live.structure_source)}</span>}
          </p>
        </section>
      ) : <section className="live-track"><h1>No track on the master deck</h1></section>}

      <section className="live-now" aria-label="Where the track is">
        <div className="live-section">
          <span className="live-label">Now</span>
          <strong>{live.section ?? '—'}</strong>
          <span className="live-beats" aria-label={`Beat ${beatInBar + 1} of 4`}>
            {[0, 1, 2, 3].map(i => <i key={i} className={i === beatInBar ? 'on' : ''} />)}
          </span>
        </div>
        <div className="live-section">
          <span className="live-label">Next</span>
          <strong>{live.next_section ?? '—'}</strong>
          {live.beats_to_next_section != null && <span>in {barsAndBeats(live.beats_to_next_section)}</span>}
        </div>
      </section>

      {live.drop_countdown_beats != null && (
        <section className="live-drop">
          <span className="live-label">Drop in</span>
          <strong>{barsAndBeats(live.drop_countdown_beats)}</strong>
        </section>
      )}

      {visuals && <p className="live-cue" role="status">{visuals}</p>}

      {live.next_cue && (
        <p className="live-cue">{cueName(live.next_cue.slot)} in {live.next_cue.seconds.toFixed(1)} s</p>
      )}
    </main>
  );
}

const root = document.getElementById('root');
if (root) createRoot(root).render(<StrictMode><Live /></StrictMode>);
