import React,{useCallback,useEffect,useState} from 'react';
import type {Track} from './types';
import {snapGrid,plainKey,roleColor} from './library';
import {Button,Card,Pill} from './ui';

function CueRow({cue,track,selected,toggle,run,seek,play,onDirty}:any){
  const [time,setTime]=useState(String(cue.time)),[end,setEnd]=useState(cue.end===null?'':String(cue.end));
  useEffect(()=>{setTime(String(cue.time));setEnd(cue.end===null?'':String(cue.end));},[cue.time,cue.end]);
  const dirty=+time!==cue.time||(cue.end!==null&&+end!==cue.end);
  useEffect(()=>onDirty(cue.id,dirty),[cue.id,dirty,onDirty]);
  const review=(status:string,position?:number)=>run(()=>window.onset.call('review',{id:track.id,cue_id:cue.id,status,...(position===undefined?{}:{position})}));
  const save=()=>run(()=>window.onset.call('review',{id:track.id,cue_id:cue.id,status:'proposed',position:+time,...(end?{end:+end}:{})}));
  function key(e:React.KeyboardEvent){
    if((e.target as HTMLElement).matches('input,button,select,textarea'))return;
    if(!plainKey(e)&&!['ArrowLeft','ArrowRight'].includes(e.key))return;
    if(['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();const beats=snapGrid(track);const next=e.key==='ArrowRight';const candidates=beats.filter((t:number)=>next?t>cue.time+.001:t<cue.time-.001);const position=e.shiftKey?Math.max(0,Math.min(track.duration-.001,cue.time+(next?.1:-.1))):(next?candidates[0]:candidates.at(-1));if(position!==undefined)review('proposed',position);}
    if(e.key.toLowerCase()==='a'&&!dirty)review('approved');
    if(e.key.toLowerCase()==='r')review('rejected');
  }
  return <div className="cue-row" tabIndex={0} role="group" aria-label={`Cue ${cue.role}: ${cue.name}`} onKeyDown={key}>
    <input type="checkbox" aria-label={`Select cue ${cue.role}`} checked={selected} onChange={toggle}/>
    <button className="cue-letter" style={{background:roleColor[cue.role]||'#cccccc'}} title="Play from just before this cue" onClick={()=>{seek(Math.max(0,cue.time-4));play();}}>{cue.role}</button>
    <div className="cue-name"><strong>{cue.name}</strong><small>{cue.source} · {cue.confidence}</small></div>
    <div className="cue-times">
      <input aria-label={`Cue ${cue.role} position seconds`} type="number" min="0" max={track.duration} step=".01" value={time} onChange={e=>setTime(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&dirty)save();}}/>
      {cue.end!==null&&<><span>→</span><input aria-label={`Cue ${cue.role} loop end seconds`} type="number" step=".01" value={end} onChange={e=>setEnd(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&dirty)save();}}/></>}
      {dirty&&<Button size="sm" kind="primary" onClick={save}>Save</Button>}
    </div>
    <div className="cue-actions">
      <Pill state={cue.review}>{cue.review==='delete-approved'?'removal':cue.review}</Pill>
      <Button size="sm" icon="check" kind="ghost" className={cue.review==='approved'?'good-on':''} label={`Approve ${cue.role}`} disabled={dirty} onClick={()=>review('approved')}/>
      <Button size="sm" icon="x" kind="ghost" className={cue.review==='rejected'?'bad-on':''} label={`Reject ${cue.role}`} onClick={()=>review('rejected')}/>
      <Button size="sm" icon="trash" kind="ghost" label={`Mark ${cue.role} for removal from rekordbox`} disabled={dirty} onClick={()=>review('delete-approved')}/>
    </div>
  </div>;
}

export function CueReview({track,run,seek,play}:{track:Track;run:any;seek:(n:number)=>void;play:()=>void}){
  const [selected,setSelected]=useState<string[]>([]);
  const [dirty,setDirty]=useState<Record<string,boolean>>({});
  const changed=Object.values(dirty).some(Boolean);
  const onDirty=useCallback((id:string,value:boolean)=>setDirty(previous=>previous[id]===value?previous:{...previous,[id]:value}),[]);
  useEffect(()=>{setSelected([]);setDirty({});},[track.id,track.revision]);
  const all=track.cues.length>0&&selected.length===track.cues.length;
  return <Card title="Cue decisions" actions={<Button size="sm" kind="ghost" icon="undo" onClick={()=>run(()=>window.onset.call('undo',{id:track.id}))}>Undo</Button>}>
    {track.cues.length>0&&<div className="bulk-bar">
      <label className="check"><input type="checkbox" checked={all} onChange={()=>setSelected(all?[]:track.cues.map(c=>c.id))}/>{selected.length?`${selected.length} selected`:'Select all'}</label>
      {['approved','rejected','proposed'].map(status=><Button key={status} size="sm" disabled={!selected.length||changed} onClick={()=>run(()=>window.onset.call('review-bulk',{id:track.id,revision:track.revision,cue_ids:selected,status}))}>{status==='approved'?'Approve':status==='rejected'?'Reject':'Reset'}</Button>)}
    </div>}
    {changed&&<p className="warning" style={{margin:'8px 0'}}>Save edited positions before approving.</p>}
    <div className="cue-list">{track.cues.map(c=><CueRow key={c.id} cue={c} track={track} selected={selected.includes(c.id)} toggle={()=>setSelected(s=>s.includes(c.id)?s.filter(id=>id!==c.id):[...s,c.id])} run={run} seek={seek} play={play} onDirty={onDirty}/>)}</div>
    {!track.cues.length&&<p className="muted">Analyse this track to get cue proposals.</p>}
    {track.cues.length>0&&<p className="hint" style={{marginTop:12}}>With a cue focused: <kbd>←</kbd><kbd>→</kbd> grid step · <kbd>Shift</kbd> fine step · <kbd>A</kbd> approve · <kbd>R</kbd> reject · <kbd>Space</kbd> play · <kbd>Ctrl</kbd>+<kbd>Z</kbd> undo</p>}
  </Card>;
}
