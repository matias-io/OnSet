import React from 'react';

export function CueAudition({role,time,color,seek,play}:{role:string;time:number;color:string;seek:(time:number)=>void;play:()=>void}){
  const audition=(start:number)=>{seek(start);play();};
  return <div className="cue-audition">
    <button className="cue-letter" style={{background:color}} title={`Play from cue ${role}`} aria-label={`Play from cue ${role}`} onClick={()=>audition(time)}>{role}</button>
    <button className="cue-preroll" title={`Play from 4 seconds before cue ${role}`} aria-label={`Play from 4 seconds before cue ${role}`} onClick={()=>audition(Math.max(0,time-4))}>−4s</button>
  </div>;
}
