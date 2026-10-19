import React,{useState} from 'react';
import {Button,Field} from './ui';

export function Relocate({track,run}:any){
  const [path,setPath]=useState('');
  return <div className="stack">
    <Field label="New location" hint={`Currently ${track.path}. The file must match the recorded audio exactly.`}><input placeholder="C:\Music\…" value={path} onChange={e=>setPath(e.target.value)}/></Field>
    <div className="row end"><Button size="sm" disabled={!path.trim()} onClick={()=>run(()=>window.autocue.call('relocate',{id:track.id,path:path.trim().replace(/^"|"$/g,'')}),'Verified audio location updated')}>Verify and link</Button></div>
  </div>;
}
