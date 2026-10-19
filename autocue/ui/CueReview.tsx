import React,{useCallback,useEffect,useState} from 'react';
import type {Cue,Track} from './types';
import {snapGrid,plainKey} from './library';
function CueRow({cue,track,selected,toggle,run,seek,play,onDirty}:any){
 const [time,setTime]=useState(String(cue.time)),[end,setEnd]=useState(cue.end===null?'':String(cue.end));
 useEffect(()=>{setTime(String(cue.time));setEnd(cue.end===null?'':String(cue.end));},[cue.time,cue.end]);
 const dirty=+time!==cue.time||(cue.end!==null&&+end!==cue.end);
 useEffect(()=>onDirty(cue.id,dirty),[cue.id,dirty,onDirty]);
 const review=(status:string,position?:number)=>run(()=>window.autocue.call('review',{id:track.id,cue_id:cue.id,status,...(position===undefined?{}:{position})}));
 function key(e:React.KeyboardEvent){if((e.target as HTMLElement).matches('input,button,select,textarea'))return;
   if(!plainKey(e)&&!['ArrowLeft','ArrowRight'].includes(e.key))return;
   if(['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();const beats=snapGrid(track);const next=e.key==='ArrowRight';const candidates=beats.filter((t:number)=>next?t>cue.time+.001:t<cue.time-.001);const position=e.shiftKey?Math.max(0,Math.min(track.duration-.001,cue.time+(next ? .1 : -.1))):(next?candidates[0]:candidates.at(-1));if(position!==undefined)review('proposed',position);}
   if(e.key.toLowerCase()==='a'&&!dirty)review('approved');if(e.key.toLowerCase()==='r')review('rejected');
 }
 return <div className="cue-row" tabIndex={0} role="group" aria-label={`Cue ${cue.role}: ${cue.name}`} onKeyDown={key}><input type="checkbox" aria-label={`Select cue ${cue.role}`} checked={selected} onChange={toggle}/><button className="cue-letter" onClick={()=>{seek(Math.max(0,cue.time-4));play();}}>{cue.role}</button><div><strong>{cue.name}</strong><small>{cue.source} · {cue.confidence}</small><span className={`pill ${cue.review}`}>{cue.review}</span></div><label>Position (s)<input aria-label={`Cue ${cue.role} position seconds`} type="number" min="0" max={track.duration} step=".01" value={time} onChange={e=>setTime(e.target.value)}/></label>{cue.end!==null&&<label>Loop end (s)<input aria-label={`Cue ${cue.role} loop end seconds`} type="number" step=".01" value={end} onChange={e=>setEnd(e.target.value)}/></label>}<div className="toolbar"><button onClick={()=>run(()=>window.autocue.call('review',{id:track.id,cue_id:cue.id,status:'proposed',position:+time,...(end?{end:+end}:{})}))}>Save position</button><button disabled={dirty} onClick={()=>review('approved')}>Approve {cue.role}</button><button onClick={()=>review('rejected')}>Reject {cue.role}</button><button disabled={dirty} onClick={()=>review('delete-approved')}>Mark {cue.role} for removal</button></div></div>;
}
export function CueReview({track,run,seek,play}:{track:Track;run:any;seek:(n:number)=>void;play:()=>void}){
 const [selected,setSelected]=useState<string[]>([]);
 const [dirty,setDirty]=useState<Record<string,boolean>>({});
 const changed=Object.values(dirty).some(Boolean);
 const onDirty=useCallback((id:string,value:boolean)=>setDirty(previous=>previous[id]===value?previous:{...previous,[id]:value}),[]);
 useEffect(()=>{setSelected([]);setDirty({});},[track.id,track.revision]);
 return <section className="panel"><div className="panel-heading"><h3>Cue decisions</h3><button onClick={()=>run(()=>window.autocue.call('undo',{id:track.id}))}>Undo last decision</button></div><p className="muted">Focus a cue row: ←/→ move one grid step, Shift+←/→ move 0.1s, A approves, R rejects. Space plays or pauses outside an input. Removal is applied only through the exact write preview.</p>{changed&&<p className="warning">Save edited positions before approving decisions.</p>}<div className="toolbar"><button onClick={()=>setSelected(track.cues.map(c=>c.id))}>Select all cues</button><button onClick={()=>setSelected([])}>Clear cue selection</button>{['approved','rejected','proposed'].map(status=><button key={status} disabled={!selected.length||changed} onClick={()=>run(()=>window.autocue.call('review-bulk',{id:track.id,revision:track.revision,cue_ids:selected,status}))}>{status==='approved'?'Approve selected':status==='rejected'?'Reject selected':'Reset selected'}</button>)}</div>{track.cues.map(c=><CueRow key={c.id} cue={c} track={track} selected={selected.includes(c.id)} toggle={()=>setSelected(s=>s.includes(c.id)?s.filter(id=>id!==c.id):[...s,c.id])} run={run} seek={seek} play={play} onDirty={onDirty}/>)}{!track.cues.length&&<p className="empty">Analyse this track to propose cues.</p>}</section>;
}
