import React,{useState} from 'react';
import {Button} from './ui';
import {padRepairRows} from './padRepair';

export function CueAdoption({track,run}:any){
  const [preview,setPreview]=useState<any>(null),[hot,setHot]=useState(false),[library,setLibrary]=useState<any>(null);
  return <div className="stack">
    <p className="muted">Lets future approved writes replace cues that earlier versions put in rekordbox. Manual cues are never included, and nothing in rekordbox changes now.</p>
    <div className="row">
      <label className="check grow"><input type="checkbox" checked={hot} onChange={e=>{setHot(e.target.checked);setPreview(null);}}/>Hot cues</label>
      <Button size="sm" onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-preview',{track_ids:[track.id],adopt:true,hot});setPreview(r);return r;})}>Find earlier cues</Button>
      <Button size="sm" onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-preview',{adopt:true,all:true});setLibrary(r);setPreview(null);return r;})}>Find earlier cues in every track</Button>
    </div>
    {preview&&<>
      {preview.cues.length?<div className="table-wrap"><table><thead><tr><th>Comment</th><th>Position</th><th>Loop end</th></tr></thead><tbody>
        {preview.cues.map((c:any)=><tr key={c.UUID}><td>{c.Comment}</td><td className="mono">{(c.InMsec/1000).toFixed(3)}s</td><td className="mono">{c.OutMsec>=0?(c.OutMsec/1000).toFixed(3)+'s':'—'}</td></tr>)}
      </tbody></table></div>:<p className="muted">No cues from earlier versions found.</p>}
      <div className="row end"><Button size="sm" kind="primary" disabled={!preview.cues.length} onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-apply',{id:preview.id});setPreview(null);return r;},'Generated cue ownership recorded locally')}>Adopt these cues</Button></div>
    </>}
    {library&&<LibraryAdoption preview={library} run={run} done={()=>setLibrary(null)}/>}
    <HotPadRepair run={run}/>
  </div>;
}

/** Moves hot cues that versions before 2026-10-22 put one pad late (D..H on rekordbox's Kind 4..8)
 *  onto their pads. A cue whose pad holds the DJ's own cue is removed when it is also a memory cue. */
function HotPadRepair({run}:any){
  const [preview,setPreview]=useState<any>(null);
  const rows=preview?padRepairRows(preview):[];
  const changes=preview?preview.moves.length+(preview.drops||[]).length:0;
  return <div className="stack">
    <p className="muted">Earlier versions put hot cues from D onward one pad late, and hot D on no pad. This moves them onto their rekordbox pads, keeping time, colour and name. Your own cues are never moved or removed; where one holds a pad, OnSet's hot cue for that pad is removed only if it is also a memory cue. Close rekordbox before applying.</p>
    <div className="row end"><Button size="sm" onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-preview',{repair_hot_slots:true});setPreview(r);return r;})}>Find hot cues on the wrong pads</Button></div>
    {preview&&<>
      {rows.length?<div className="table-wrap"><table><thead><tr><th>Track</th><th>Moves</th><th>Removed</th><th>Left in place</th></tr></thead><tbody>
        {rows.map(r=><tr key={r.title}><td>{r.title}</td><td>{r.moves.join(', ')||'—'}</td><td>{r.removed.join(', ')||'—'}</td><td>{r.left.join(', ')||'—'}</td></tr>)}
      </tbody></table></div>:<p className="muted">Every hot cue is already on its pad.</p>}
      <div className="row end"><Button size="sm" kind="primary" disabled={!changes} onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-apply',{id:preview.id});setPreview(null);return r;},'Hot cues moved to their pads; backup saved')}>Apply {changes} change{changes===1?'':'s'}</Button></div>
    </>}
  </div>;
}

/** Every track's cues from earlier versions, hot and memory, adopted together. */
function LibraryAdoption({preview,run,done}:any){
  const cues=preview.tracks.reduce((n:number,e:any)=>n+e.cues.length,0);
  const tracks=new Set(preview.tracks.map((e:any)=>e.track_id)).size;
  return <>
    <p>{cues} cue{cues===1?'':'s'} from earlier versions in {tracks} track{tracks===1?'':'s'}
      {preview.skipped.length?` · ${preview.skipped.length} track${preview.skipped.length===1?'':'s'} skipped (not matched to exactly one rekordbox entry)`:''}</p>
    {preview.tracks.length>0&&<div className="table-wrap"><table><thead><tr><th>Track</th><th>Kind</th><th>Cues</th></tr></thead><tbody>
      {preview.tracks.map((e:any)=><tr key={e.track_id+(e.hot?':hot':':memory')}><td>{e.title}</td><td>{e.hot?'Hot':'Memory'}</td><td className="mono">{e.cues.length}</td></tr>)}
    </tbody></table></div>}
    <div className="row end"><Button size="sm" kind="primary" disabled={!cues} onClick={()=>run(async()=>{const r=await window.onset.call('rekordbox-apply',{id:preview.id});done();return r;},'Generated cue ownership recorded locally for the whole library')}>Adopt all {cues} cues</Button></div>
  </>;
}
