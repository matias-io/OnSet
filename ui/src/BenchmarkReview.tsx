import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Disclosure,Field} from './ui';
import {barAt,benchmarkEvents,clock} from './library';

type EventLabel={state:'present'|'absent'|'unknown';bar?:number};
const blank=():Record<string,EventLabel>=>Object.fromEntries(benchmarkEvents.map(([id])=>[id,{state:'unknown'}]));

// Structured listening labels. Grid samples, boundaries and sections stay editable as JSON under Advanced.
export function BenchmarkReview({track,run,position=0}:{track:Track;run:any;position?:number}){
  const [events,setEvents]=useState<Record<string,EventLabel>>(blank),[keys,setKeys]=useState(''),[bpm,setBpm]=useState(''),[notes,setNotes]=useState('');
  const [advanced,setAdvanced]=useState('{}'),[verified,setVerified]=useState(false),[error,setError]=useState(''),[split,setSplit]=useState('');
  useEffect(()=>{let active=true;window.onset.call('benchmark').then(value=>{if(!active)return;const r=value.records.find((r:any)=>r.track_id===track.id)||{};
    setEvents({...blank(),...(r.events||{})});setKeys((r.keys||[]).join(', '));setBpm(r.bpm?String(r.bpm):'');setNotes(r.notes||'');setVerified(!!r.verified);setSplit(r.split||'');
    const {grid_samples,boundaries,sections}=r;setAdvanced(JSON.stringify({...(grid_samples?{grid_samples}:{}),...(boundaries?{boundaries}:{}),...(sections?{sections}:{})},null,2));}).catch(e=>{if(active)setError(e.message);});return()=>{active=false;};},[track.id]);
  const update=(id:string,change:Partial<EventLabel>)=>setEvents(e=>{const next={...e[id],...change};if(next.state!=='present')delete next.bar;return {...e,[id]:next};});
  const playheadBar=barAt(track,position);
  function save(){try{
    const extra=JSON.parse(advanced||'{}');if(!extra||typeof extra!=='object'||Array.isArray(extra))throw new Error('Advanced labels must be a JSON object');
    const unexpected=Object.keys(extra).filter(k=>!['grid_samples','boundaries','sections'].includes(k));if(unexpected.length)throw new Error(`Unsupported advanced fields: ${unexpected.join(', ')}`);
    for(const [id,e] of Object.entries(events))if(e.state==='present'&&!(Number.isInteger(e.bar)&&(e.bar as number)>=0))throw new Error(`${id.replaceAll('_',' ')}: present events need a bar number`);
    const record:any={schema_version:1,track_id:track.id,verified,events,keys:keys.split(',').map(k=>k.trim()).filter(Boolean),notes,...extra};
    if(bpm){if(!(+bpm>0))throw new Error('BPM must be positive');record.bpm=+bpm;}
    setError('');run(()=>window.onset.call('benchmark-label',record),'Benchmark labels saved');}catch(e:any){setError(e.message);}}
  return <div className="stack">
    <p className="muted">Ground truth for measuring accuracy{split?` · ${split} split`:''}. Bars count from zero. Only labels you verified by listening count.</p>
    <table className="label-table"><thead><tr><th>Event</th><th>State</th><th>Bar</th><th/></tr></thead><tbody>
      {benchmarkEvents.map(([id,name])=>{const e=events[id]||{state:'unknown'};return <tr key={id}>
        <td>{name}</td>
        <td><select aria-label={`${name} state`} value={e.state} onChange={ev=>update(id,{state:ev.target.value as EventLabel['state']})}><option value="unknown">Not checked</option><option value="present">Present</option><option value="absent">Absent</option></select></td>
        <td><input aria-label={`${name} bar`} type="number" min="0" step="1" style={{width:90}} disabled={e.state!=='present'} value={e.bar??''} onChange={ev=>update(id,{bar:ev.target.value===''?undefined:Math.floor(+ev.target.value)})}/></td>
        <td><Button size="sm" kind="ghost" disabled={playheadBar===null} title={`Playhead ${clock(position)}`} onClick={()=>update(id,{state:'present',bar:playheadBar??undefined})}>Use playhead{playheadBar!==null?` (bar ${playheadBar})`:''}</Button></td>
      </tr>;})}
    </tbody></table>
    <div className="form-grid three">
      <Field label="Acceptable keys" hint="Standard or Camelot, comma separated"><input value={keys} onChange={e=>setKeys(e.target.value)} placeholder="8A, C major"/></Field>
      <Field label="True BPM"><input type="number" min="1" step="0.01" value={bpm} onChange={e=>setBpm(e.target.value)}/></Field>
      <Field label="Notes"><input value={notes} onChange={e=>setNotes(e.target.value)}/></Field>
    </div>
    <Disclosure summary="Advanced: grid samples, boundaries and sections (JSON)">
      <textarea aria-label="Advanced benchmark labels JSON" className="mono" rows={6} value={advanced} onChange={e=>setAdvanced(e.target.value)}/>
    </Disclosure>
    <div className="row"><label className="check grow"><input type="checkbox" checked={verified} onChange={e=>setVerified(e.target.checked)}/>I verified these labels by listening</label><Button size="sm" kind="primary" onClick={save}>Save labels</Button></div>
    {error&&<p className="error-text">{error}</p>}
  </div>;
}
