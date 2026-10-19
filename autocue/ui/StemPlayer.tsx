import React,{useEffect,useRef,useState} from 'react';
import type {Track} from './types';
export function StemPlayer({track,position,playOriginal,pauseOriginal}:{track:Track;position:number;playOriginal:()=>void;pauseOriginal:()=>void}){
 const stems=Object.keys(track.analysis?.deep?.stems||{}).filter(s=>['drums','bass','vocals','other'].includes(s));
 const [urls,setUrls]=useState<Record<string,string>>({}),[error,setError]=useState('');
 const players=useRef<Record<string,HTMLAudioElement|null>>({});
 useEffect(()=>{let active=true;Promise.all(stems.map(async name=>[name,await window.autocue.media(track.id,name)])).then(values=>{if(active)setUrls(Object.fromEntries(values));}).catch(e=>setError(e.message));return()=>{active=false;Object.values(players.current).forEach(p=>p?.pause());};},[track.id,track.revision]);
 function stop(){Object.values(players.current).forEach(p=>p?.pause());}
 async function play(){pauseOriginal();try{await Promise.all(Object.values(players.current).filter(Boolean).map(async p=>{p!.currentTime=position;await p!.play();}));}catch(e:any){stop();setError(e.message);}}
 return <section className="panel"><h3>Local stem audition</h3><p className="muted">Separation is fallible. Compare against the original mix. Separate streaming controls are for inspection; rendered transition previews provide the timed rehearsal.</p>{stems.length?<><div className="toolbar"><button onClick={play}>Play stems at cursor</button><button onClick={stop}>Pause stems</button><button onClick={()=>{stop();playOriginal();}}>Compare original mix</button></div>{stems.map(name=><div className="stem-control" key={name}><strong>{name}</strong><audio ref={p=>{players.current[name]=p;}} controls src={urls[name]}/><label>{name} level<input type="range" min="0" max="1" step=".01" defaultValue="1" onChange={e=>{const p=players.current[name];if(p)p.volume=+e.target.value;}}/></label></div>)}</>:<p>Complete deep analysis to audition drums, bass, vocals and other material here.</p>}{error&&<p className="warning">{error}</p>}</section>;
}
