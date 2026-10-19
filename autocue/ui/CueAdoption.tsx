import React,{useState} from 'react';
import {Button} from './ui';

export function CueAdoption({track,run}:any){
  const [preview,setPreview]=useState<any>(null),[hot,setHot]=useState(false);
  return <div className="stack">
    <p className="muted">Lets future approved writes replace cues an earlier Autocue version put in rekordbox. Manual cues are never included, and nothing in rekordbox changes now.</p>
    <div className="row">
      <label className="check grow"><input type="checkbox" checked={hot} onChange={e=>{setHot(e.target.checked);setPreview(null);}}/>Hot cues</label>
      <Button size="sm" onClick={()=>run(async()=>{const r=await window.autocue.call('rekordbox-preview',{track_ids:[track.id],adopt:true,hot});setPreview(r);return r;})}>Find Autocue cues</Button>
    </div>
    {preview&&<>
      {preview.cues.length?<div className="table-wrap"><table><thead><tr><th>Comment</th><th>Position</th><th>Loop end</th></tr></thead><tbody>
        {preview.cues.map((c:any)=><tr key={c.UUID}><td>{c.Comment}</td><td className="mono">{(c.InMsec/1000).toFixed(3)}s</td><td className="mono">{c.OutMsec>=0?(c.OutMsec/1000).toFixed(3)+'s':'—'}</td></tr>)}
      </tbody></table></div>:<p className="muted">No Autocue-generated cues found.</p>}
      <div className="row end"><Button size="sm" kind="primary" disabled={!preview.cues.length} onClick={()=>run(async()=>{const r=await window.autocue.call('rekordbox-apply',{id:preview.id});setPreview(null);return r;},'Generated cue ownership recorded locally')}>Adopt these cues</Button></div>
    </>}
  </div>;
}
