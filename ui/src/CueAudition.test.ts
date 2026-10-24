import {describe,it,expect,vi} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {CueAudition} from './CueAudition';

describe('cue audition controls',()=>{
  function controls(time:number){
    const calls:unknown[]=[];
    const seek=vi.fn((value:number)=>calls.push(value)),play=vi.fn(()=>calls.push('play'));
    const element=CueAudition({role:'D',time,color:'#fff',seek,play});
    return {buttons:element.props.children,element,calls};
  }
  it('plays the exact displayed cue time, including cues inside the first four seconds',()=>{
    for(const time of [3.946,49.562,224.1]){
      const {buttons,calls}=controls(time);
      buttons[0].props.onClick();
      expect(calls).toEqual([time,'play']);
    }
  });
  it('keeps the lead-in as a separate action and clamps it to the start',()=>{
    for(const [time,start] of [[3.946,0],[49.562,45.562]]){
      const {buttons,calls}=controls(time);
      buttons[1].props.onClick();
      expect(calls).toEqual([start,'play']);
    }
  });
  it('labels exact seeking and lead-in playback separately',()=>{
    const html=renderToStaticMarkup(controls(49.562).element);
    expect(html).toContain('aria-label="Play from cue D"');
    expect(html).toContain('aria-label="Play from 4 seconds before cue D"');
  });
});
