import {describe, expect, it} from 'vitest';
import {barsAndBeats, carry, clock, cueName, structureLabel, untilNextBeat} from './liveFormat';

describe('live view formatting', () => {
  it('counts down in bars and beats', () => {
    expect(barsAndBeats(32)).toBe('8 bars');
    expect(barsAndBeats(14)).toBe('3 bars 2 beats');
    expect(barsAndBeats(4)).toBe('1 bar');
    expect(barsAndBeats(1)).toBe('1 beat');
    expect(barsAndBeats(0)).toBe('now');
  });
  it('shows track time as m:ss', () => {
    expect(clock(54.2)).toBe('0:54');
    expect(clock(612)).toBe('10:12');
    expect(clock(-1)).toBe('0:00');
  });
  it('calls prepared structure Prep', () => {
    expect(structureLabel('Prepared')).toBe('Prep');
    expect(structureLabel('rekordbox')).toBe('rekordbox');
  });
  it('names cues as rekordbox does', () => {
    expect(cueName(0)).toBe('Memory cue');
    expect(cueName(1)).toBe('Hot cue A');
    expect(cueName(8)).toBe('Hot cue H');
  });
  it('carries a playing track forward at its tempo', () => {
    const at = {playing: true, bpm: 120, playhead_s: 10, bar_phase: 0.5};
    // 120 BPM: a bar lasts 2 s.
    expect(carry(at, 1)).toEqual({playhead_s: 11, bar_phase: 0});
    expect(carry(at, 0.5).bar_phase).toBeCloseTo(0.75);
    expect(carry({...at, playing: false}, 1)).toEqual({playhead_s: 10, bar_phase: 0.5});
  });
  it('times the next beat', () => {
    expect(untilNextBeat(0.5, 120, true)).toBeCloseTo(0.5);
    expect(untilNextBeat(0.6, 120, true)).toBeCloseTo(0.3);
    expect(untilNextBeat(0.6, 120, false)).toBeNull();
  });
});
