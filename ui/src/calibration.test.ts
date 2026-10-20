import {describe,expect,it} from 'vitest';
import {calibrationProgress,plainLine} from './calibration';

// Lines as crates/onset-transport/src/memory/calibrator.rs reports them.
const opening=[
  'rekordbox 7.2.18 (pid 26984), module at 0x7ff6a0000000',
  'This takes a minute or two: with a track paused, play it, pause it, play it again.',
  'Step 1: load a track on any deck and leave it paused, at least a minute before any loop or the end of the track (anywhere else is fine; the start is not needed).',
  '  recording what runs with the deck paused...',
  '  scanning for playback positions (two passes over rekordbox\'s memory)...',
];

describe('calibration progress',()=>{
  it('keeps the deck paused while the first scan runs',()=>{
    expect(calibrationProgress(opening)).toEqual({step:1,seen:false,scanning:true});
  });
  it('asks for play only once the paused scan is done',()=>{
    const lines=[...opening,'  765 counter(s) run while paused (rekordbox\'s own clocks); these are ignored','Step 2: press play on that deck.'];
    expect(calibrationProgress(lines)).toEqual({step:2,seen:false,scanning:false});
    expect(calibrationProgress([...lines,'  scanning for playback positions (two passes over rekordbox\'s memory)...']).scanning).toBe(true);
  });
  it('marks a step seen, then moves on',()=>{
    const lines=[...opening,'Step 2: press play on that deck.','  12 counter(s) confirmed; the largest group reads 117.8 s','Step 3: pause that deck (press play/pause once). (waiting up to 240 s)','  seen'];
    expect(calibrationProgress(lines)).toEqual({step:3,seen:true,scanning:false});
  });
  it('finishes on its own after the second play',()=>{
    const lines=['Step 4: press play on that deck again. (waiting up to 240 s)','  seen','  4 counter(s) resumed from the paused value: absolute positions'];
    expect(calibrationProgress(lines).step).toBe(5);
  });
  it('drops the calibrator\'s own wait limit',()=>{
    expect(plainLine('Step 3: pause that deck (press play/pause once). (waiting up to 240 s)')).toBe('Step 3: pause that deck (press play/pause once).');
  });
});
