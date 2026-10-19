import React,{useState} from 'react';
import type {Track} from './types';
import {Button,Card,Disclosure} from './ui';

const labels=['intro','groove','verse','vocal','breakdown','build','drop','stripped_groove','outro','silence','unknown'];

export function StructureEditor({track,onSave}:{track:Track;onSave:(changes:any)=>void}){
  const original=track.overrides?.sections||track.analysis?.structure?.sections||[];
  const [sections,setSections]=useState<any[]>(original),[grid,setGrid]=useState(JSON.stringify(track.overrides?.grid||track.analysis?.downbeats||[])),[error,setError]=useState('');
  function change(index:number,key:string,value:any){setSections(current=>current.map((s,i)=>i===index?{...s,[key]:value}:s));}
  function save(){try{const times=JSON.parse(grid),prior=track.overrides?.grid||track.analysis?.downbeats||[];onSave({sections,...(JSON.stringify(times)!==JSON.stringify(prior)?{grid:times}:{})});setError('');}catch(e:any){setError(e.message);}}
  return <Card title="Sections" actions={<Button size="sm" kind="primary" onClick={save}>Save corrections</Button>}>
    <p className="muted" style={{marginBottom:12}}>Times are in seconds of the original audio. You can also drag section edges on the waveform.</p>
    {sections.length?<div className="table-wrap section-table"><table><thead><tr><th>Section</th><th>Start (s)</th><th>End (s)</th></tr></thead><tbody>
      {sections.map((s,i)=><tr key={`${i}-${s.label}`}>
        <td><select aria-label={`Section ${i+1} label`} value={s.label} onChange={e=>change(i,'label',e.target.value)}>{Array.from(new Set([...labels,s.label])).map(l=><option key={l} value={l}>{l.replace('_',' ')}</option>)}</select></td>
        <td><input aria-label={`Section ${i+1} start`} type="number" min="0" max={track.duration} step=".01" value={s.start} onChange={e=>change(i,'start',+e.target.value)}/></td>
        <td><input aria-label={`Section ${i+1} end`} type="number" min="0" max={track.duration} step=".01" value={s.end} onChange={e=>change(i,'end',+e.target.value)}/></td>
      </tr>)}
    </tbody></table></div>:<p className="muted">No sections yet. Analyse the track first.</p>}
    <div style={{marginTop:8}}>
      <Disclosure summary="Downbeat grid (advanced)">
        <p className="muted" style={{marginBottom:8}}>JSON list of downbeat times. Changes affect analysis and recommendations only; the rekordbox grid is never rewritten.</p>
        <textarea aria-label="Analysis downbeats, JSON seconds" className="mono" style={{width:'100%'}} value={grid} onChange={e=>setGrid(e.target.value)}/>
      </Disclosure>
    </div>
    {error&&<p className="error-text">{error}</p>}
  </Card>;
}
