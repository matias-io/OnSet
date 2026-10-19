import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Pill} from './ui';

const EVENTS:[string,string][]=[['drop','First full groove'],['breakdown','Breakdown'],['second_drop','Later drop'],['outro','Outro']];
const READERS=['claude','codex','gemini'] as const;
type Reader=typeof READERS[number];
const NAMES:Record<Reader,string>={claude:'Claude',codex:'Codex',gemini:'Gemini'};

/** One cell: the bar a source put an event at, or why it has none. */
export function cell(o:any){
  if(!o)return '—';
  if(o.state==='absent')return 'none';
  if(o.state==='unknown'||o.state==='failed')return '?';
  return typeof o.bar==='number'?`bar ${o.bar}`:'?';
}

/** Where each source puts the main parts of the track, and AI reads to add more sources. */
export function StructureReads({track,observations,onRead}:{track:Track;observations:any[];onRead:()=>void}){
  const onset=(window as any).onset;
  const [installed,setInstalled]=useState<Reader[]>([]),[chosen,setChosen]=useState<Reader[]>([]);
  const [busy,setBusy]=useState(false),[result,setResult]=useState<any>(null),[error,setError]=useState('');
  useEffect(()=>{onset?.providers?.().then((s:any)=>{const have=s.clis.filter((c:any)=>c.path).map((c:any)=>c.provider);setInstalled(have);setChosen(have.filter((p:Reader)=>p!=='gemini'));}).catch(()=>{});},[]);
  const sources=[...new Set(observations.map(o=>o.source))];
  async function read(providers:Reader[],audio:boolean){
    setBusy(true);setError('');
    try{const r=await onset.readStructure(providers,track.id,audio);setResult(r);onRead();}
    catch(e:any){setError(e.message);}finally{setBusy(false);}
  }
  return <div className="stack">
    {sources.length?<table className="reads"><thead><tr><th>Part</th>{sources.map(s=><th key={s}>{s}</th>)}</tr></thead><tbody>
      {EVENTS.map(([event,label])=><tr key={event}><td>{label}</td>{sources.map(s=><td key={s}>{cell(observations.find(o=>o.source===s&&o.event===event))}</td>)}</tr>)}
    </tbody></table>:<p className="muted">No independent reads yet.</p>}
    {onset?.readStructure?<>
      <div className="row">
        {READERS.map(p=><label className="check" key={p}><input type="checkbox" disabled={!installed.includes(p)} checked={chosen.includes(p)}
          onChange={()=>setChosen(c=>c.includes(p)?c.filter(x=>x!==p):[...c,p])}/>{NAMES[p]}{!installed.includes(p)&&' (not installed)'}</label>)}
      </div>
      <div className="row">
        <Button disabled={busy||!chosen.length} onClick={()=>read(chosen,false)}>Read the spectrogram</Button>
        <Button kind="ghost" disabled={busy||!installed.includes('gemini')} title="Sends this track's audio file to Gemini"
          onClick={()=>{if(window.confirm(`Send the audio of "${track.title}" to Gemini so it can listen for the structure?`))read(['gemini'],true);}}>Let Gemini listen…</Button>
      </div>
      <p className="hint">Spectrogram reads send an image of this track's analysis, never audio. Reads stay unverified until you check them by ear.</p>
      {busy&&<p className="dim">Reading… this can take a minute or two.</p>}
      {error&&<p className="bad">{error}</p>}
      {result&&<div className="row">{result.reads.map((r:any)=><Pill key={r.provider} state={r.ok?'good':'bad'}>{r.provider}: {r.ok?'read saved':r.error}</Pill>)}</div>}
    </>:<p className="hint">AI structure reads need the OnSet desktop app.</p>}
  </div>;
}
