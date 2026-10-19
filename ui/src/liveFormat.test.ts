import {describe, expect, it} from 'vitest';
import {barsAndBeats, clock, cueName, structureLabel} from './liveFormat';

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
});
