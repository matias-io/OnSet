import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Card,Disclosure,KeyValues} from './ui';
import {StructureReads} from './StructureReads';

const curveColor=['#8b7cf6','#f472b6','#fb923c','#facc15','#34d399'];
const fmt=(v:any):string=>typeof v==='number'?(Number.isInteger(v)?String(v):v.toFixed(3)):Array.isArray(v)?v.map(fmt).join(', '):typeof v==='object'&&v!==null?JSON.stringify(v):String(v);

export function EvidencePanel({track}:{track:Track}){
  const [evidence,setEvidence]=useState<any>(null),[reloads,setReloads]=useState(0);
  useEffect(()=>{let active=true;window.onset.call('evidence',{id:track.id}).then(v=>{if(active)setEvidence(v);}).catch(()=>{if(active)setEvidence(null);});return()=>{active=false;};},[track.id,track.revision,reloads]);
  const a=track.analysis||{},ep=a.energy_profile||{},dimensions=['drive','density','intensity','tension','contrast'];
  const research=evidence?.enrichment?.research,claims=evidence?.enrichment?.claims||[];
  const keys=(a.key_observations||[a.key]).filter(Boolean);
  const curves=dimensions.filter(name=>(ep[name]||[]).length);
  return <>
    <Card title="Character over time">
      {curves.length?<div className="curves">{curves.map(name=>{const curve:number[]=ep[name],i=dimensions.indexOf(name);
        const points=curve.map((v,j)=>`${j/Math.max(1,curve.length-1)*600},${32-Math.max(0,Math.min(1,v))*28}`).join(' ');
        return <div className="curve" key={name}><span>{name}</span><svg viewBox="0 0 600 34" preserveAspectRatio="none" role="img" aria-label={`${name} through the track`}><polyline points={points} fill="none" stroke={curveColor[i]} strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round"/></svg></div>;})}</div>
        :<p className="muted">Analyse the track to see how its energy develops.</p>}
    </Card>
    <Card title="Detector evidence">
      <p className="muted" style={{marginBottom:6}}>Estimates stay unverified until you check them. Disagreements are kept.</p>
      <Disclosure summary={`Key estimates${keys.length?` · ${keys.length}`:''}`}>
        {keys.length?<table><thead><tr><th>Source</th><th>Key</th><th>Strength</th><th>State</th><th>Alternatives</th></tr></thead><tbody>
          {keys.map((k:any,i:number)=><tr key={i}><td>{k.source||'—'}</td><td>{k.name||k.camelot||'—'}</td><td>{typeof k.strength==='number'?k.strength.toFixed(2):'—'}</td><td>{k.state||'—'}</td><td className="dim">{(k.alternatives||[]).map((x:any)=>x.name).filter(Boolean).join(', ')||'—'}</td></tr>)}
        </tbody></table>:<p className="muted">No key estimate yet.</p>}
      </Disclosure>
      <Disclosure summary="Rhythm and grid">
        {a.grid_observation?<KeyValues rows={Object.entries(a.grid_observation).filter(([,v])=>!Array.isArray(v)||(v as any[]).length<8).map(([k,v])=>[k.replaceAll('_',' '),fmt(v)])}/>:<p className="muted">No grid evidence yet.</p>}
      </Disclosure>
      <Disclosure summary="Where the parts are, by source" defaultOpen>
        <StructureReads track={track} observations={evidence?.track?.observations||[]} onRead={()=>setReloads(n=>n+1)}/>
      </Disclosure>
      <Disclosure summary="Raw event opinions">
        <pre>{JSON.stringify({observations:evidence?.track?.observations,resolution:evidence?.resolved_opinions},null,2)}</pre>
      </Disclosure>
    </Card>
    <Card title="Sourced descriptions">
      {research&&<p className="hint" style={{marginBottom:8}}>{research.requested?'Online research':'Model interpretation'} · {research.consulted_sources?.length||0} sources consulted · {research.model}</p>}
      {claims.length?claims.map((c:any,i:number)=><div className="claim" key={i}>
        <strong>{c.kind}: {c.value}</strong>
        <small>{c.source_type} · {c.confidence} confidence · {c.retrieved}{c.citation_check&&` · citation ${c.citation_check.status}`}</small>
        {c.citation_check?.status==='unverified'&&c.citation_check.requested_url&&<small>Unverified proposed URL: {c.citation_check.requested_url}</small>}
        {c.url&&<div className="row" style={{marginTop:6}}><span className="mono dim" style={{overflowWrap:'anywhere'}}>{c.url}</span><Button size="sm" kind="ghost" icon="external" onClick={()=>window.onset.call('open-source',{url:c.url})}>Open</Button></div>}
      </div>):<p className="muted">No descriptions yet. Use “Look up track details” in the set planner.</p>}
    </Card>
  </>;
}
