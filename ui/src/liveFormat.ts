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
