import {describe,it,expect} from 'vitest';
import {canonicalKey,matchesFilters,emptyFilters,analysisSelection} from './library';
describe('library evidence filters',()=>{
 it('normalizes Camelot and enharmonic notation',()=>{expect(canonicalKey('A minor')).toBe('8A');expect(canonicalKey('Db major')).toBe(canonicalKey('C# major'));expect(canonicalKey('8a')).toBe('8A');});
 it('does not turn missing measurements into known matches',()=>{const t={bpm:null,key:null,genres:[],analysis:null};expect(matchesFilters(t,{...emptyFilters,energyMin:'4'})).toBe(false);expect(matchesFilters(t,{...emptyFilters,vocals:'unknown'})).toBe(true);expect(matchesFilters(t,{...emptyFilters,vocals:'absent'})).toBe(false);});
 it('matches measured ranges and standard keys',()=>{const t={bpm:124,key:'A minor',genres:['Deep House'],energy:6,analysis:{structure:{windows:[{vocals:true}]}}};expect(matchesFilters(t,{...emptyFilters,key:'8A',genre:'house',bpmMin:'120',energyMax:'7',vocals:'present'})).toBe(true);});
});

it('limits default analysis to visible results and preserves explicit checked tracks',()=>{
 const tracks=[{id:'a'},{id:'b'}];
 expect(analysisSelection(tracks,[tracks[0]],[])).toEqual(['a']);
 expect(analysisSelection(tracks,[],[])).toEqual([]);
 expect(analysisSelection(tracks,[tracks[0]],['b','b','removed'])).toEqual(['b']);
 expect(analysisSelection(tracks,[tracks[0]],['removed'])).toEqual(['a']);
});

import {restoreBrief,pruneBriefTracks,defaultBrief,snapGrid,nearest,plainKey} from './library';
describe('set brief persistence',()=>{
 it('restores valid fields and rejects wrong types',()=>{
  const brief=restoreBrief({schema_version:1,target_seconds:5400,skill:'3',mood:'warm',energy_arc:[3,'x'],key_lock:false});
  expect(brief.target_seconds).toBe(5400);expect(brief.skill).toBe(defaultBrief.skill);expect(brief.mood).toBe('warm');
  expect(brief.energy_arc).toEqual(defaultBrief.energy_arc);expect(brief.key_lock).toBe(false);
 });
 it('ignores future schemas and drops removed tracks',()=>{
  expect(restoreBrief({schema_version:2,target_seconds:60}).target_seconds).toBe(3600);
  const brief=restoreBrief({must_play:['a','gone'],start:'gone',finish:'a',pinned:[{track_id:'gone',position:0},{track_id:'a',position:1},{transition_id:'t'}]},['a']);
  expect(brief.must_play).toEqual(['a']);expect(brief.start).toBeNull();expect(brief.finish).toBe('a');expect(brief.pinned).toHaveLength(2);
  expect(pruneBriefTracks(brief,['a'])).toBe(brief);
 });
});
describe('review editing helpers',()=>{
 it('snaps to a corrected grid before analysed beats',()=>{
  const track={overrides:{grid:[0,1.1,2.2]},analysis:{beats:[0,1,2]}};
  expect(nearest(snapGrid(track),1.04)).toBe(1.1);expect(nearest(snapGrid({analysis:{beats:[0,1,2]}}),1.04)).toBe(1);expect(nearest([],3)).toBe(3);
 });
 it('treats modifier chords as non-review keys',()=>{expect(plainKey({})).toBe(true);expect(plainKey({ctrlKey:true})).toBe(false);expect(plainKey({metaKey:true})).toBe(false);});
});
