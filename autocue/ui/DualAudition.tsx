import React,{useEffect,useRef,useState} from 'react';
import {Button,Disclosure} from './ui';

export function DualAudition({option}:any){
  const a=useRef<HTMLAudioElement>(null),b=useRef<HTMLAudioElement>(null),[sources,setSources]=useState<string[]>([]),[error,setError]=useState('');
  useEffect(()=>{let active=true;window.autocue&&Promise.all(option.track_ids.map((id:string)=>window.autocue.media(id))).then(v=>{if(active)setSources(v);}).catch(e=>{if(active)setError(e.message);});return()=>{active=false;};},[option.id]);
  async function play(){if(!a.current||!b.current)return;try{a.current.currentTime=option.out_start;b.current.currentTime=option.in_start;a.current.playbackRate=option.out_rate;b.current.playbackRate=option.in_rate;a.current.preservesPitch=b.current.preservesPitch=option.key_lock;await Promise.all([a.current.play(),b.current.play()]);}catch(e:any){setError(e.message);}}
  return <Disclosure summary="Audition both decks">
    <div className="audition">
      {[['Outgoing',a,sources[0]],['Incoming',b,sources[1]]].map(([label,ref,src]:any)=><div className="audition-deck" key={label}>
        <span>{label}</span><audio ref={ref} src={src} controls/>
        <input type="range" aria-label={`${label} level`} min="0" max="1" step=".01" defaultValue="1" onChange={e=>{if(ref.current)ref.current.volume=+e.target.value;}}/>
      </div>)}
      <div className="row"><Button size="sm" icon="play" onClick={play}>Start both at entry points</Button><Button size="sm" kind="ghost" icon="pause" onClick={()=>{a.current?.pause();b.current?.pause();}}>Pause</Button></div>
      {error&&<p className="error-text">{error}</p>}
    </div>
  </Disclosure>;
}
