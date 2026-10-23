import {describe,expect,it} from 'vitest';
import {padLetter,padRepairRows} from './padRepair';

describe('pad repair',()=>{
  it('names rekordbox pads, which skip Kind 4',()=>{
    expect([1,2,3,4,5,6,7,8,9].map(padLetter)).toEqual(['A','B','C','no pad','D','E','F','G','H']);
  });

  it('groups moves, removals and cues left in place by track',()=>{
    const cue=(Comment:string,Kind:number)=>({Comment,Kind});
    const rows=padRepairRows({
      moves:[{title:'Stardust',before:cue('D drop',4),kind:5},{title:'Empty',before:cue('G drop 2',7),kind:8}],
      drops:[{title:'Stardust',before:cue('E breakdown',5)}],
      conflicts:[{title:'Empty',cue:'F outro'}],
    });
    expect(rows).toEqual([
      {title:'Empty',moves:['G drop 2: F → G'],removed:[],left:['F outro']},
      {title:'Stardust',moves:['D drop: no pad → D'],removed:['E breakdown (pad D)'],left:[]},
    ]);
  });
});
