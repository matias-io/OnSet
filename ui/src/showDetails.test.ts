import {describe, expect, it} from 'vitest';
import {EFFECTS, inRotation, toggledRotation, withEffect} from './ShowDetails';

const scenes = [
  {name: 'tunnel', auto: true},
  {name: 'crystal', auto: false},
  {name: 'ring', auto: true},
];

describe('scene rotation', () => {
  it('follows each scene default until the DJ picks', () => {
    expect(scenes.filter(s => inRotation(null, s)).map(s => s.name)).toEqual(['tunnel', 'ring']);
    expect(inRotation(['crystal'], scenes[1])).toBe(true);
    expect(inRotation(['crystal'], scenes[0])).toBe(false);
  });
  it('turning a scene on or off starts from what Auto uses now', () => {
    expect(toggledRotation(null, scenes, 'crystal', true)).toEqual(['tunnel', 'ring', 'crystal']);
    expect(toggledRotation(null, scenes, 'ring', false)).toEqual(['tunnel']);
    expect(toggledRotation(['crystal'], scenes, 'crystal', false)).toEqual([]);
  });
});

describe('effects', () => {
  it('a slider makes the style Custom from the current values', () => {
    const club = {reactivity: 1, shake: 0.2, strobe: false};
    expect(withEffect(club, 'shake', 0.5)).toEqual({preset: 'custom', fx: {reactivity: 1, shake: 0.5, strobe: false}});
    expect(withEffect(club, 'strobe', true).fx.strobe).toBe(true);
  });
  it('lists every effect once, with reactivity and glow going past 1', () => {
    const keys = EFFECTS.flatMap(g => g.items.map(i => i.key));
    expect(new Set(keys).size).toBe(keys.length);
    expect(keys).toHaveLength(13);
    const max = Object.fromEntries(EFFECTS.flatMap(g => g.items.map(i => [i.key, i.max])));
    expect(max.reactivity).toBe(2);
    expect(max.bloom).toBe(1.5);
  });
});
