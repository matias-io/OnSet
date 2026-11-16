import React,{useState} from 'react';
import {Button,Card,Field,Icon} from './ui';
const api=window.onset;

type Rule={from:string;to:string};

function FolderInput({label,value,onChange,placeholder='D:\\Music'}:{label:string;value:string;onChange:(v:string)=>void;placeholder?:string}){
  return <Field label={label}><div className="row" style={{flexWrap:'nowrap'}}>
    <input className="grow" value={value} onChange={e=>onChange(e.target.value)} placeholder={placeholder}/>
    <Button size="sm" icon="folder" label={`Choose ${label.toLowerCase()}`} onClick={async()=>{const [folder]=await api.pick('folder');if(folder)onChange(folder);}}/>
  </div></Field>;
}

// Import a workspace exported on another computer. Paths are remapped, then every file is checked by checksum.
export function PortableImport({run}:{run:any}){
  const [file,setFile]=useState(''),[rules,setRules]=useState<Rule[]>([{from:'',to:''}]),[report,setReport]=useState<any>(null);
  const active=rules.filter(r=>r.from.trim()&&r.to.trim());
  const set=(i:number,change:Partial<Rule>)=>setRules(rs=>rs.map((r,n)=>n===i?{...r,...change}:r));
  return <Card title="Import from another computer">
    <p className="muted">Use a workspace JSON or SQLite export. Map the old music folders to where the same files live here; each file is matched by checksum, never by name.</p>
    <div className="row" style={{margin:'12px 0',flexWrap:'nowrap'}}>
      <input className="grow" aria-label="Workspace export file" value={file} readOnly placeholder="No export chosen"/>
      <Button size="sm" onClick={async()=>{const [picked]=await api.pick('workspace');if(picked)setFile(picked);}}>Choose export</Button>
    </div>
    {rules.map((r,i)=><div className="form-grid" key={i}>
      <Field label={`Old folder ${i+1}`}><input value={r.from} onChange={e=>set(i,{from:e.target.value})} placeholder="D:\Music"/></Field>
      <FolderInput label={`New folder ${i+1}`} value={r.to} onChange={v=>set(i,{to:v})}/>
    </div>)}
    <div className="row" style={{marginTop:8}}>
      <Button size="sm" kind="ghost" icon="plus" onClick={()=>setRules(rs=>[...rs,{from:'',to:''}])}>Add folder mapping</Button>
      <Button size="sm" kind="primary" className="spacer" disabled={!file} onClick={()=>run(async()=>{const r=await api.call('import-workspace',{path:file,remap:active});setReport(r);return r;},'Workspace imported')}>Import</Button>
    </div>
    {report&&<div className="stack" style={{marginTop:12}}>
      <p>{report.imported} profiles read · {report.located} matched to local audio · {report.history_rows} history rows restored</p>
      {report.missing_audio?.length>0&&<p className="warning"><Icon name="alert" size={14}/>{report.missing_audio.length} tracks have no matching audio here. Add a folder mapping and import again, or use Locate moved audio on each track.</p>}
      {report.failed?.map((f:any)=><p className="warning" key={f.path}><Icon name="alert" size={14}/>{f.error}</p>)}
    </div>}
  </Card>;
}

// After moving the feature/stem cache folder, point stored analysis at the new location.
export function CacheRelocation({run}:{run:any}){
  const [from,setFrom]=useState(''),[to,setTo]=useState(''),[makeDefault,setMakeDefault]=useState(true),[report,setReport]=useState<any>(null);
  return <Card title="Move analysis cache">
    <p className="muted">Move the cache folder yourself first (features, stems and model outputs). OnSet then updates its references and checks every file exists.</p>
    <div className="form-grid" style={{marginTop:12}}>
      <Field label="Previous cache folder"><input value={from} onChange={e=>setFrom(e.target.value)} placeholder="C:\Users\you\AppData\Local\Onset\Onset\data\workspace\features"/></Field>
      <FolderInput label="New cache folder" value={to} onChange={setTo} placeholder="E:\OnSet cache"/>
    </div>
    <div className="row" style={{marginTop:8}}>
      <label className="check grow"><input type="checkbox" checked={makeDefault} onChange={e=>setMakeDefault(e.target.checked)}/>Use the new folder for future analysis</label>
      <Button size="sm" kind="primary" disabled={!from.trim()||!to.trim()} onClick={()=>run(async()=>{const r=await api.call('relocate-cache',{from,to,set_default:makeDefault});setReport(r);return r;},'Cache references updated')}>Update references</Button>
    </div>
    {report&&<div className="stack" style={{marginTop:12}}>
      <p>{report.updated} tracks updated · {report.cache_files_rewritten} cached results rewritten</p>
      {report.missing?.map((m:any)=><p className="warning" key={m.track_id}><Icon name="alert" size={14}/>Missing after the move: {m.missing.join(', ')}</p>)}
    </div>}
  </Card>;
}
