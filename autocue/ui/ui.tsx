import React,{useState} from 'react';
import type {Track} from './types';

// Shared interface primitives. Keep visual decisions in style.css tokens rather than inline colours.

const paths:Record<string,string>={
  library:'M4 5h16M4 12h16M4 19h10',
  wave:'M3 12h2M7 8v8M11 4v16M15 7v10M19 10v4M21 12h0',
  transition:'M7 7h11l-3-3M17 17H6l3 3',
  set:'M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01',
  settings:'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6ZM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z',
  plus:'M12 5v14M5 12h14',
  search:'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-4-4',
  filter:'M4 5h16l-6 8v6l-4-2v-4L4 5Z',
  play:'M7 5v14l12-7L7 5Z',
  pause:'M8 5v14M16 5v14',
  check:'M5 12.5l4.5 4.5L19 7.5',
  x:'M6 6l12 12M18 6 6 18',
  undo:'M9 14 4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3',
  download:'M12 4v11m0 0-4-4m4 4 4-4M5 20h14',
  upload:'M12 20V9m0 0-4 4m4-4 4 4M5 4h14',
  chevron:'M9 6l6 6-6 6',
  up:'M12 19V5m0 0-6 6m6-6 6 6',
  down:'M12 5v14m0 0-6-6m6 6 6-6',
  pin:'M12 17v5M8 3h8l-1 6 3 4H6l3-4-1-6Z',
  alert:'M12 9v4m0 4h.01M10.3 3.9 2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z',
  spark:'M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3Z',
  folder:'M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z',
  headphones:'M4 15v-3a8 8 0 0 1 16 0v3M4 15a2 2 0 0 1 2-2h1v7H6a2 2 0 0 1-2-2v-3ZM20 15a2 2 0 0 0-2-2h-1v7h1a2 2 0 0 0 2-2v-3Z',
  external:'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5',
  refresh:'M20 11a8 8 0 0 0-14.9-3M4 4v4h4M4 13a8 8 0 0 0 14.9 3M20 20v-4h-4',
};
export function Icon({name,size=16}:{name:string;size?:number}){
  return <svg className="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]||''}/></svg>;
}

type ButtonProps={children?:React.ReactNode;onClick?:()=>any;disabled?:boolean;kind?:''|'primary'|'ghost'|'danger'|'subtle';size?:''|'sm';icon?:string;label?:string;title?:string;className?:string};
export function Button({children,onClick,disabled=false,kind='',size='',icon,label,title,className=''}:ButtonProps){
  return <button type="button" className={['btn',kind,size,!children&&icon?'icon-only':'',className].filter(Boolean).join(' ')} onClick={onClick} disabled={disabled} aria-label={label} title={title||label}>
    {icon&&<Icon name={icon} size={size==='sm'?14:16}/>}{children}
  </button>;
}

const tone:Record<string,string>={approved:'good',reviewed:'good',complete:'good',running:'info',reviewing:'info',queued:'info',
  partial:'warn',stale:'warn',paused:'warn',pending:'neutral',failed:'bad',rejected:'bad',cancelled:'neutral','delete-approved':'bad'};
export function Pill({children,state=''}:{children:React.ReactNode;state?:string}){
  return <span className={`pill ${tone[state]||'neutral'}`}>{children}</span>;
}
export function Empty({children,icon}:{children:React.ReactNode;icon?:string}){
  return <div className="empty">{icon&&<span className="empty-icon"><Icon name={icon} size={22}/></span>}{children}</div>;
}
export function Card({title,actions,children,className=''}:{title?:React.ReactNode;actions?:React.ReactNode;children:React.ReactNode;className?:string}){
  return <section className={`card ${className}`}>
    {(title||actions)&&<div className="card-head">{title&&<h3>{title}</h3>}{actions&&<div className="card-actions">{actions}</div>}</div>}
    {children}
  </section>;
}
export function Tabs({value,options,onChange}:{value:string;options:{id:string;label:string;count?:number}[];onChange:(id:string)=>void}){
  return <div className="tabs" role="tablist">{options.map(o=><button type="button" role="tab" aria-selected={value===o.id} key={o.id} className={value===o.id?'active':''} onClick={()=>onChange(o.id)}>{o.label}{o.count!==undefined&&<span className="count">{o.count}</span>}</button>)}</div>;
}
export function Segmented({value,options,onChange,label}:{value:string;options:{id:string;label:string}[];onChange:(id:string)=>void;label:string}){
  return <div className="segmented" role="radiogroup" aria-label={label}>{options.map(o=><button type="button" role="radio" aria-checked={value===o.id} key={o.id} className={value===o.id?'active':''} onClick={()=>onChange(o.id)}>{o.label}</button>)}</div>;
}
export function Field({label,children,full=false,hint}:{label:string;children:React.ReactNode;full?:boolean;hint?:string}){
  return <label className={`field${full?' full':''}`}><span>{label}</span>{children}{hint&&<small>{hint}</small>}</label>;
}
export function Stat({label,value,unit}:{label:string;value:React.ReactNode;unit?:string}){
  return <div className="stat"><span>{label}</span><strong>{value}{unit&&<small>{unit}</small>}</strong></div>;
}
export function KeyValues({rows}:{rows:[string,React.ReactNode][]}){
  return <dl className="kv">{rows.filter(([,v])=>v!==undefined&&v!==null&&v!=='').map(([k,v])=><React.Fragment key={k}><dt>{k}</dt><dd>{v}</dd></React.Fragment>)}</dl>;
}
export function Disclosure({summary,children,defaultOpen=false}:{summary:React.ReactNode;children:React.ReactNode;defaultOpen?:boolean}){
  const [open,setOpen]=useState(defaultOpen);
  return <div className={`disclosure${open?' open':''}`}><button type="button" className="disclosure-toggle" aria-expanded={open} onClick={()=>setOpen(!open)}><Icon name="chevron" size={14}/>{summary}</button>{open&&<div className="disclosure-body">{children}</div>}</div>;
}

// Chip list with an "add" dropdown; replaces tall multi-select boxes.
export function TrackPicker({label,tracks,value,onChange,placeholder='Add a track…'}:{label:string;tracks:Track[];value:string[];onChange:(v:string[])=>void;placeholder?:string}){
  const lookup=new Map(tracks.map(t=>[t.id,t]));
  return <div className="field full"><span>{label}</span>
    <div className="chips">
      {value.map(id=><span className="chip" key={id}>{lookup.get(id)?.title||id}<button type="button" aria-label={`Remove ${lookup.get(id)?.title||id}`} onClick={()=>onChange(value.filter(v=>v!==id))}><Icon name="x" size={12}/></button></span>)}
      <select aria-label={`${label}: add track`} value="" onChange={e=>{if(e.target.value)onChange([...value,e.target.value]);}}>
        <option value="">{placeholder}</option>
        {tracks.filter(t=>!value.includes(t.id)).map(t=><option key={t.id} value={t.id}>{t.title}{t.artist?` — ${t.artist}`:''}</option>)}
      </select>
    </div>
  </div>;
}
