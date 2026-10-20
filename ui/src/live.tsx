import {StrictMode, useEffect, useState} from 'react';
import {createRoot} from 'react-dom/client';
import './live.css';
import {Logo} from './Logo';
import {barsAndBeats, carry, clock, cueName, structureLabel, untilNextBeat} from './liveFormat';

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
  const [live, setLive] = useState<{state: LiveState; at: number} | null>(null);
  // Redrawn on each beat: between updates the beat is carried forward from the tempo.
  const [, setBeat] = useState(0);
  const [onTop, setOnTop] = useState(false);
  const [visuals, setVisuals] = useState<string | null>(null);
  useEffect(() => onset?.onLive(state => setLive({state, at: performance.now()})), []);
  const elapsed = live ? (performance.now() - live.at) / 1000 : 0;
  const now = live ? carry(live.state, elapsed) : null;
  useEffect(() => {
    if (!live || !now) return;
    const wait = untilNextBeat(now.bar_phase, live.state.bpm, live.state.playing);
    if (wait == null) return;
    const timer = window.setTimeout(() => setBeat(b => b + 1), wait * 1000 + 5);
    return () => window.clearTimeout(timer);
  });

  if (!onset) return <main className="live"><p className="live-status">The live view needs the OnSet desktop app.</p></main>;
  if (!live || !now) return <main className="live"><p className="live-status">Starting the live link…</p></main>;

  const beatInBar = Math.min(3, Math.floor(now.bar_phase * 4));
  const t = live.state.track;
  return (
    <main className="live">
      <header className="live-head">
        <Logo size={20} />
        <span className={`live-dot ${live.state}`} aria-hidden="true" />
        <span className="live-status">{live.state.status}</span>
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
            <span>{live.state.bpm ? `${live.state.bpm.toFixed(1)} BPM` : '— BPM'}</span>
            <span>{t.key ?? '— key'}</span>
            <span>{clock(now.playhead_s)}{live.state.playing ? '' : ' · paused'}</span>
            {live.state.structure_source && <span className={`live-source ${structureLabel(live.state.structure_source).toLowerCase()}`}>
              Structure: {structureLabel(live.state.structure_source)}</span>}
          </p>
        </section>
      ) : <section className="live-track"><h1>No track on the master deck</h1></section>}

      <section className="live-now" aria-label="Where the track is">
        <div className="live-section">
          <span className="live-label">Now</span>
          <strong>{live.state.section ?? '—'}</strong>
          <span className="live-beats" aria-label={`Beat ${beatInBar + 1} of 4`}>
            {[0, 1, 2, 3].map(i => <i key={i} className={i === beatInBar ? 'on' : ''} />)}
          </span>
        </div>
        <div className="live-section">
          <span className="live-label">Next</span>
          <strong>{live.state.next_section ?? '—'}</strong>
          {live.state.beats_to_next_section != null && <span>in {barsAndBeats(live.state.beats_to_next_section)}</span>}
        </div>
      </section>

      {live.state.drop_countdown_beats != null && (
        <section className="live-drop">
          <span className="live-label">Drop in</span>
          <strong>{barsAndBeats(live.state.drop_countdown_beats)}</strong>
        </section>
      )}

      {visuals && <p className="live-cue" role="status">{visuals}</p>}

      {live.state.next_cue && (
        <p className="live-cue">{cueName(live.state.next_cue.slot)} in {Math.max(0, live.state.next_cue.seconds - (now.playhead_s - live.state.playhead_s)).toFixed(0)} s</p>
      )}
    </main>
  );
}

const root = document.getElementById('root');
if (root) createRoot(root).render(<StrictMode><Live /></StrictMode>);
