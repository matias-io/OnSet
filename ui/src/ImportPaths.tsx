import React,{useState} from 'react';
import {Button,Card,Icon} from './ui';

export function ImportPaths({run,onClose}:{run:any;onClose?:()=>void}){
  const [paths,setPaths]=useState(''),[result,setResult]=useState<any>(null);
  return <Card title="Paste paths" actions={onClose&&<Button size="sm" kind="ghost" icon="x" label="Close" onClick={onClose}/>}>
    <textarea aria-label="Local file or folder paths, one per line" style={{width:'100%'}} value={paths} onChange={e=>setPaths(e.target.value)} placeholder={'One file or folder per line, e.g.\nC:\\Music\\track.flac'}/>
    <div className="row" style={{marginTop:10}}>
      <span className="muted grow">Audio and analysis files stay where they are.</span>
      <Button kind="primary" size="sm" disabled={!paths.trim()} onClick={()=>run(async()=>{const r=await window.onset.call('import',{paths:paths.split(/\r?\n/).map(p=>p.trim().replace(/^"|"$/g,'')).filter(Boolean)});setResult(r);return r;})}>Import</Button>
    </div>
    {result&&<div style={{marginTop:10}}>
      <p className="muted">{result.imported} records processed · {result.track_ids?.length??result.imported} tracks matched</p>
      {result.failed.map((f:any,i:number)=><p key={i} className="warning"><Icon name="alert" size={14}/>{f.path}: {f.error}</p>)}
    </div>}
  </Card>;
}
