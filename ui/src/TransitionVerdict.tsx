import React,{useEffect,useState} from 'react';
import {Button} from './ui';

export const verdicts=[['works','Works'],['needs-changes','Needs changes'],['does-not-work',"Doesn't work"]] as const;
export const hearings=[['render','Rendered preview'],['rehearsal','Rehearsed on the decks'],['set','Played in a set']] as const;

// Saved verdicts, fetched once for the page rather than once per option card.
let saved:Promise<Record<string,any>>|null=null;
export function savedVerdicts(reload=false){
  if(!saved||reload)saved=window.onset.call('benchmark',{}).then((b:any)=>Object.fromEntries((b.transition_ratings||[]).map((r:any)=>[r.option_id,r]))).catch(()=>({}));
  return saved;
}

/** The DJ's listening verdict on one transition option, kept as transition acceptability evidence. */
export function TransitionVerdict({option,run}:{option:any;run:any}){
  const [verdict,setVerdict]=useState(''),[heard,setHeard]=useState('render'),[note,setNote]=useState(''),[last,setLast]=useState<any>(null);
  useEffect(()=>{let active=true;savedVerdicts().then(all=>{const r=all[option.id];if(active&&r){setLast(r);setVerdict(r.verdict);setHeard(r.heard);setNote(r.note||'');}});return ()=>{active=false;};},[option.id]);
  function save(){run(async()=>{const r=await window.onset.call('transition-rating',{schema_version:1,option_id:option.id,verdict,heard,...(note?{note}:{})});setLast(r);savedVerdicts(true);return r;},'Verdict saved');}
  return <div className="stack verdict">
    <div className="row">{verdicts.map(([id,label])=><Button key={id} size="sm" kind={verdict===id?'primary':'ghost'} onClick={()=>setVerdict(id)}>{label}</Button>)}</div>
    <div className="row">
      <select aria-label="Heard in" value={heard} onChange={e=>setHeard(e.target.value)}>{hearings.map(([id,label])=><option key={id} value={id}>{label}</option>)}</select>
      <input className="grow" aria-label="Verdict note" maxLength={500} placeholder="What to change (optional)" value={note} onChange={e=>setNote(e.target.value)}/>
      <Button size="sm" disabled={!verdict} onClick={save}>Save verdict</Button>
    </div>
    {last&&<p className="muted">Saved: {verdicts.find(([id])=>id===last.verdict)?.[1]} · {hearings.find(([id])=>id===last.heard)?.[1].toLowerCase()}</p>}
  </div>;
}
