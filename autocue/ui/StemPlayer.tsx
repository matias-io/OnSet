import React,{useEffect,useRef,useState} from 'react';
import type {Track} from './types';
import {Button,Card,Empty} from './ui';

export function StemPlayer({track,position,playOriginal,pauseOriginal}:{track:Track;position:number;playOriginal:()=>void;pauseOriginal:()=>void}){
  const stems=Object.keys(track.analysis?.deep?.stems||{}).filter(s=>['drums','bass','vocals','other'].includes(s));
  const [urls,setUrls]=useState<Record<string,string>>({}),[error,setError]=useState('');
  const players=useRef<Record<string,HTMLAudioElement|null>>({});
  useEffect(()=>{let active=true;Promise.all(stems.map(async name=>[name,await window.autocue.media(track.id,name)])).then(values=>{if(active)setUrls(Object.fromEntries(values));}).catch(e=>setError(e.message));return()=>{active=false;Object.values(players.current).forEach(p=>p?.pause());};},[track.id,track.revision]);
  function stop(){Object.values(players.current).forEach(p=>p?.pause());}
  async function play(){pauseOriginal();try{await Promise.all(Object.values(players.current).filter(Boolean).map(async p=>{p!.currentTime=position;await p!.play();}));}catch(e:any){stop();setError(e.message);}}
  if(!stems.length)return <Card title="Stems"><Empty icon="headphones">Run deep analysis to separate drums, bass, vocals and other parts.</Empty></Card>;
  return <Card title="Stems" actions={<><Button size="sm" icon="play" onClick={play}>Play from cursor</Button><Button size="sm" icon="pause" onClick={stop}>Pause</Button><Button size="sm" kind="ghost" onClick={()=>{stop();playOriginal();}}>Original mix</Button></>}>
    <p className="muted" style={{marginBottom:10}}>Separation can leave artefacts — compare with the original mix.</p>
    {stems.map(name=><div className="stem-row" key={name}>
      <strong>{name}</strong>
      <audio ref={p=>{players.current[name]=p;}} controls src={urls[name]}/>
      <input type="range" aria-label={`${name} level`} min="0" max="1" step=".01" defaultValue="1" onChange={e=>{const p=players.current[name];if(p)p.volume=+e.target.value;}}/>
    </div>)}
    {error&&<p className="error-text">{error}</p>}
  </Card>;
}
