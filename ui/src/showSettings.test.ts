import {describe, expect, it} from 'vitest';
import {chosenMonitor, monitorChoice, sceneTitle} from './ShowSettings';

const screens = [
  {name: '\\.\DISPLAY1', width: 2256, height: 1504, primary: true},
  {name: '\\.\DISPLAY2', width: 1920, height: 1080, primary: false},
];

describe('show screen choice', () => {
  it('names a projector by its Windows name and the laptop as primary', () => {
    expect(monitorChoice(screens[1])).toEqual({kind: 'name_contains', value: '\\.\DISPLAY2'});
    expect(monitorChoice(screens[0])).toEqual({kind: 'primary'});
  });
  it('finds the saved screen, or none when it is unplugged', () => {
    expect(chosenMonitor({kind: 'primary'}, screens)).toBe(0);
    expect(chosenMonitor({kind: 'name_contains', value: 'display2'}, screens)).toBe(1);
    expect(chosenMonitor({kind: 'index', value: 1}, screens)).toBe(1);
    expect(chosenMonitor({kind: 'name_contains', value: 'DISPLAY3'}, screens)).toBe(-1);
  });
  it('titles scenes', () => {
    expect(sceneTitle('zerog')).toBe('Zero-G');
    expect(sceneTitle('crystal')).toBe('Crystal');
  });
});
