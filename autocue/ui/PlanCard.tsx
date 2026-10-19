import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Card,Disclosure,Icon,Pill} from './ui';
import {clock} from './library';
const api=window.autocue;

function EnergyArc({values}:{values:(number|null)[]}){
  const known=values.map((v,i)=>({v,i})).filter(p=>p.v!==null) as {v:number;i:number}[];
  if(!known.length)return <p className="muted" style={{margin:'8px 0'}}>Rate or calibrate tracks to see the energy curve.</p>;
  const x=(i:number)=>16+i*568/Math.max(1,values.length-1),y=(v:number)=>74-v*6.4;
  const line=known.map((p,n)=>`${n?'L':'M'}${x(p.i)} ${y(p.v)}`).join(' ');
  return <svg className="energy-arc" viewBox="0 0 600 84" preserveAspectRatio="none" role="img" aria-label="Set energy curve">
    <defs><linearGradient id="arc" x1="0" x2="1"><stop offset="0" stopColor="#8b7cf6"/><stop offset="1" stopColor="#f472b6"/></linearGradient></defs>
    {[2,5,8].map(v=><line key={v} x1="0" x2="600" y1={y(v)} y2={y(v)} stroke="#24262d" strokeWidth="1" vectorEffect="non-scaling-stroke"/>)}
    <path d={line} fill="none" stroke="url(#arc)" strokeWidth="2.5" vectorEffect="non-scaling-stroke"/>
    {known.map(p=><circle key={p.i} cx={x(p.i)} cy={y(p.v)} r="3.5" fill="#ececf1"/>)}
  </svg>;
}

export function PlanCard({plan,index,tracks,run,onUpdated,onPin,onPinTransition,pinned,onUseBrief,onReplan,transpose={},onTranspose}:any){
  const [links,setLinks]=useState<any[]>([]),[name,setName]=useState('Autocue set'),[preview,setPreview]=useState<any>(null),[fallbacks,setFallbacks]=useState<Record<number,any[]>>({}),[open,setOpen]=useState<number|null>(null);
  useEffect(()=>{Promise.all(plan.transition_ids.map((id:string)=>api.call('transition',{id}))).then(setLinks).catch(()=>{});},[plan.id]);
  const lookup=new Map<string,Track>(tracks.map((t:Track)=>[t.id,t]));
  const energy=plan.track_ids.map((id:string)=>{const t=lookup.get(id);return t?.energy??t?.analysis?.energy_profile?.rating??null;});
  const locked=(i:number,j:number)=>pinned.some((p:any)=>p.track_id&&[plan.track_ids[i],plan.track_ids[j]].includes(p.track_id));
  async function move(i:number,delta:number){const ids=[...plan.track_ids];[ids[i],ids[i+delta]]=[ids[i+delta],ids[i]];const changed=await run(()=>api.call('replan-order',{id:plan.id,track_ids:ids}));if(changed)onUpdated(changed);}
  return <Card>
    <div className="plan-head">
      <h3>Option {index+1}</h3>
      <span className="mono dim">{clock(plan.duration_seconds,false)} · {plan.track_ids.length} tracks</span>
      <Pill state={plan.stale?'stale':'complete'}>{plan.stale?'Needs revalidation':'Timing validated'}</Pill>
      <div className="row spacer">
        {onUseBrief&&plan.brief&&<Button size="sm" kind="ghost" onClick={onUseBrief}>Use this brief</Button>}
      </div>
    </div>
    {Object.entries(plan.brief?.transpose||{}).some(([id,v])=>v&&plan.track_ids.includes(id))&&<p className="hint">Includes deliberate key shifts; each transition lists the deck setting.</p>}
    {plan.stale?<p className="warning"><Icon name="alert" size={14}/>{plan.validation_error||'Track evidence changed. Replan or validate before using this set.'}</p>:<p className="muted">{plan.explanation}</p>}
    <EnergyArc values={energy}/>
    <ol className="set-list">{plan.track_ids.map((id:string,i:number)=>{const t=lookup.get(id),link=links[i],isPinned=pinned.some((p:any)=>p.track_id===id&&p.position===i);
      return <li key={id} className="set-item">
        <span className="set-index">{i+1}</span>
        <div style={{minWidth:0}}><strong>{t?.title||id}</strong><small>{[t?.artist,t?.bpm&&`${t.bpm.toFixed(0)} BPM`].filter(Boolean).join(' · ')}</small></div>
        <div className="row">
          <Button size="sm" kind="ghost" icon="up" label="Move earlier" disabled={i===0||locked(i,i-1)} onClick={()=>move(i,-1)}/>
          <Button size="sm" kind="ghost" icon="down" label="Move later" disabled={i===plan.track_ids.length-1||locked(i,i+1)} onClick={()=>move(i,1)}/>
          <Button size="sm" kind="ghost" icon="pin" className={isPinned?'pinned':''} label={isPinned?'Unpin position':'Pin to this position'} onClick={()=>onPin(id,i,!isPinned)}/>
          {onTranspose&&<select aria-label={`Key shift for ${t?.title||id}`} title="Deliberate key shift (key lock on); rebuild to apply" value={transpose[id]||0} onChange={e=>onTranspose(id,+e.target.value)}>{[-2,-1,0,1,2].map(v=><option key={v} value={v}>{v>0?`+${v}`:v===0?'±0':v} st</option>)}</select>}
          {onReplan&&!plan.stale&&<Button size="sm" kind="ghost" icon="refresh" disabled={i===plan.track_ids.length-1} label={`Keep tracks 1–${i+1} and replan the rest`} onClick={()=>onReplan(i+1)}/>}
        </div>
        {link&&<div className="link-row">
          <button className="link-summary" aria-expanded={open===i} onClick={()=>setOpen(open===i?null:i)}><Icon name="transition" size={14}/>{link.name} · {link.wall_seconds.toFixed(0)}s · {link.pitch_pct>=0?'+':''}{link.pitch_pct.toFixed(2)}%{link.warnings.length>0&&<Icon name="alert" size={14}/>}<span className="spacer"><Icon name="chevron" size={14}/></span></button>
          {open===i&&<div className="link-detail">
            <div className="meta-line"><span>Out {clock(link.out_start,false)} → {clock(link.out_end,false)}</span><span>In {clock(link.in_start,false)} → {clock(link.in_end,false)}</span></div>
            <ol className="steps">{link.actions.map((a:any,n:number)=><li key={n}><span>+{a.at.toFixed(0)}s</span>{a.action}</li>)}</ol>
            {link.warnings.map((w:string)=><p className="warning" key={w}><Icon name="alert" size={14}/>{w}</p>)}
            <div className="row">
              <Button size="sm" icon="play" disabled={!link.preview_supported} onClick={()=>run(async()=>{const r=await api.previewMedia(link.id,false);setPreview(r);return r;})}>Audition</Button>
              <Button size="sm" icon="pin" className={pinned.some((p:any)=>p.transition_id===link.id)?'pinned':''} onClick={()=>onPinTransition(link.id,!pinned.some((p:any)=>p.transition_id===link.id))}>{pinned.some((p:any)=>p.transition_id===link.id)?'Pinned':'Pin transition'}</Button>
              <Button size="sm" kind="ghost" onClick={()=>run(async()=>{const values=await api.call('transition-alternatives',{id:plan.id,index:i});setFallbacks(f=>({...f,[i]:values}));return values;})}>Find alternatives</Button>
            </div>
            {fallbacks[i]&&(fallbacks[i].length?fallbacks[i].map(f=><div className="list-row" key={f.transition.id}><span className="grow">{f.transition.name} · {f.transition.wall_seconds.toFixed(0)}s</span><Button size="sm" onClick={()=>run(async()=>{const selected=await api.call('import-plan',{plan:f.plan});onUpdated(selected);return selected;})}>Use</Button></div>)
              :<p className="muted">No alternative keeps this set's timing and constraints.</p>)}
          </div>}
        </div>}
      </li>;})}</ol>
    {preview?.url!==undefined&&preview?.kind!=='playlist'&&<audio controls src={preview.url} style={{width:'100%',marginTop:10}}/>}
    {Object.keys(plan.omission_reasons||{}).length>0&&<Disclosure summary={`${Object.keys(plan.omission_reasons).length} ${Object.keys(plan.omission_reasons).length===1?'track':'tracks'} left out`}>{Object.entries(plan.omission_reasons).map(([id,reason])=><p className="muted" key={id}><b style={{color:'var(--text)',fontWeight:500}}>{lookup.get(id)?.title||id}</b> — {String(reason)}</p>)}</Disclosure>}
    <hr className="divider"/>
    <div className="row">
      <span className="muted">Export</span>
      {['json','csv','html','pdf'].map(extension=><Button key={extension} size="sm" kind="ghost" onClick={()=>run(()=>api.export({kind:'plan',id:plan.id,name:`autocue-set.${extension}`}))}>{extension.toUpperCase()}</Button>)}
      <div className="row spacer" style={{flexWrap:'nowrap'}}>
        <input aria-label="Playlist name" value={name} onChange={e=>setName(e.target.value)} style={{width:180}}/>
        <Button size="sm" onClick={()=>run(async()=>{const r=await api.call('rekordbox-preview',{plan_id:plan.id,playlist_name:name});setPreview(r);return r;})}>Preview rekordbox playlist</Button>
      </div>
    </div>
    {preview?.kind==='playlist'&&<div className="write-preview">
      <h4>Create “{preview.name}”</h4>
      <p className="muted">{preview.entries.length} tracks in this order. Existing playlists are kept.</p>
      <div className="row end" style={{marginTop:10}}>
        <Button size="sm" kind="ghost" onClick={()=>setPreview(null)}>Discard</Button>
        <Button size="sm" kind="primary" onClick={()=>run(async()=>{const r=await api.call('rekordbox-apply',{id:preview.id});setPreview(null);return r;},'Reviewed playlist written; backup saved')}>Write playlist</Button>
      </div>
    </div>}
  </Card>;
}
