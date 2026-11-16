/** "8 bars", "3 bars 2 beats", "2 beats"; "now" at zero. */
export function barsAndBeats(beats: number): string {
  const bars = Math.floor(beats / 4), rest = beats % 4;
  const b = bars ? `${bars} bar${bars === 1 ? '' : 's'}` : '';
  const r = rest ? `${rest} beat${rest === 1 ? '' : 's'}` : '';
  return [b, r].filter(Boolean).join(' ') || 'now';
}

/** m:ss of track time. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** rekordbox's naming: slot 0 is a memory cue, 1..8 are hot cues A..H. */
export function cueName(slot: number): string {
  return slot === 0 ? 'Memory cue' : `Hot cue ${String.fromCharCode(64 + slot)}`;
}

/** Whose structure the show follows, in OnSet's words: prepared structure comes from Prep. */
export function structureLabel(source: string): string {
  return source === 'Prepared' ? 'Prep' : source;
}

/** Where a playing track is `elapsed` seconds after an update: the playhead and the bar
 * phase move on at the tempo. Updates come only on changes and every half second, so the
 * screen carries the beat itself in between. */
export function carry(at: {playing: boolean; bpm: number; playhead_s: number; bar_phase: number}, elapsed: number) {
  if (!at.playing || !(at.bpm > 0) || elapsed <= 0) return {playhead_s: at.playhead_s, bar_phase: at.bar_phase};
  const bars = elapsed * at.bpm / 240;
  return {playhead_s: at.playhead_s + elapsed, bar_phase: (at.bar_phase + bars) % 1};
}

/** Seconds until the next beat starts, from the bar phase and tempo; null when stopped. */
export function untilNextBeat(barPhase: number, bpm: number, playing: boolean): number | null {
  if (!playing || !(bpm > 0)) return null;
  const beats = barPhase * 4;
  return (Math.floor(beats) + 1 - beats) * 60 / bpm;
}
