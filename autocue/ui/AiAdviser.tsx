import React,{useEffect,useState} from 'react';
import {Button,Card,Disclosure,Field,Pill} from './ui';

type Provider='claude'|'codex'|'gemini'|'chatgpt';
type Cli={provider:Provider;path:string|null;version:string|null;install:{command:string;url:string}|null};
type Onset={providers:()=>Promise<{clis:Cli[];chatgpt:{connected:boolean;plan_enabled:boolean}}>;
  testProvider:(p:Provider)=>Promise<{seconds:number}>;
  lookUpDetails:(p:Provider[],ids:string[],web:boolean)=>Promise<any>;
  compareSets:(p:Provider[],brief:any,plans:any[])=>Promise<any>};
const onset=():Onset|undefined=>(window as any).onset?.providers?(window as any).onset:undefined;

const NAMES:Record<Provider,string>={claude:'Claude',codex:'Codex',gemini:'Gemini',chatgpt:'ChatGPT'};
/** Details lookups for many tracks take long; the first ones selected are sent. */
export const DETAIL_LIMIT=25;

export function percent(x:number|null|undefined){return typeof x==='number'?`${Math.round(x*100)}%`:'—';}

/** The AI adviser in OnSet: any installed AI CLI or a connected ChatGPT account. */
export function AiAdviser({ids,brief,plans,setPlans,titles,run,busy,chatgptPanel}:{ids:string[];brief:any;plans:any[];
  setPlans:(p:any[])=>void;titles:(id:string)=>string;run:(a:()=>Promise<any>,success?:string)=>Promise<any>;busy:boolean;
  chatgptPanel:React.ReactNode}){
  const api=onset();
  const [status,setStatus]=useState<Awaited<ReturnType<Onset['providers']>>|null>(null);
  const [chosen,setChosen]=useState<Provider[]>([]),[web,setWeb]=useState(false);
  const [comparison,setComparison]=useState<any>(null),[details,setDetails]=useState<any>(null);
  useEffect(()=>{api?.providers().then(s=>{setStatus(s);setChosen(c=>c.length?c:[...s.clis.filter(x=>x.path).map(x=>x.provider),...(s.chatgpt.connected?['chatgpt' as Provider]:[])]);}).catch(()=>setStatus(null));},[]);
  if(!api)return <Card title="ChatGPT adviser">{chatgptPanel}</Card>;
  const available:Provider[]=status?[...status.clis.filter(c=>c.path).map(c=>c.provider),...(status.chatgpt.connected?['chatgpt' as Provider]:[])]:[];
  const toggle=(p:Provider)=>setChosen(c=>c.includes(p)?c.filter(x=>x!==p):[...c,p]);
  const picked=chosen.filter(p=>available.includes(p));
  return <Card title="AI adviser">
    <div className="stack">
      <Field label="Providers" hint="Each answer is kept separately and labelled with who gave it.">
        <div className="stack">
          {status?.clis.map(c=><div className="row" key={c.provider}>
            <label className="check grow"><input type="checkbox" disabled={!c.path} checked={picked.includes(c.provider)} onChange={()=>toggle(c.provider)}/>{NAMES[c.provider]}</label>
            {c.path?<><span className="dim">{c.version||'installed'}</span><Button size="sm" kind="ghost" onClick={()=>run(()=>api.testProvider(c.provider),`${NAMES[c.provider]} answered`)}>Test</Button></>
              :<><Pill state="warn">Not installed</Pill><span className="mono dim" title="Install command">{c.install?.command}</span>
                {c.install&&<Button size="sm" kind="ghost" icon="external" onClick={()=>window.autocue.call('open-source',{url:c.install!.url})}>How to install</Button>}</>}
          </div>)}
          <div className="row">
            <label className="check grow"><input type="checkbox" disabled={!status?.chatgpt.connected} checked={picked.includes('chatgpt')} onChange={()=>toggle('chatgpt')}/>ChatGPT</label>
            {status?.chatgpt.connected?<span className="dim">connected</span>:<span className="dim">Connect in the panel below</span>}
          </div>
        </div>
      </Field>
      <label className="check"><input type="checkbox" checked={web} onChange={e=>setWeb(e.target.checked)}/>Look up track details online</label>
      <div className="row">
        <Button kind="primary" disabled={busy||!picked.length||!plans.length} title={plans.length?undefined:'Build options with the planner first'}
          onClick={()=>run(async()=>{const r=await api.compareSets(picked,{...brief,track_ids:ids},plans);setComparison(r);
            const valid=r.ai.filter((a:any)=>a.valid).map((a:any)=>({...a.set,source:a.provider}));if(valid.length)setPlans([...plans,...valid]);return r;},'Compared with the planner')}>Compare with the planner</Button>
        <Button disabled={busy||!picked.length||!ids.length}
          onClick={()=>run(async()=>{const r=await api.lookUpDetails(picked,ids.slice(0,DETAIL_LIMIT),web);setDetails(r);return r;},'Track details saved with their sources')}>Look up track details</Button>
      </div>
      <p className="hint">Sends track profiles and the planner's validated options, never audio.{ids.length>DETAIL_LIMIT&&` Details are looked up for the first ${DETAIL_LIMIT} selected tracks.`}</p>
      {comparison&&<Comparison result={comparison} titles={titles}/>}
      {details&&<ProviderResults rows={details.providers}/>}
      <Disclosure summary="ChatGPT account">{chatgptPanel}</Disclosure>
    </div>
  </Card>;
}

function ProviderResults({rows}:{rows:any[]}){
  return <div className="stack">{rows.map((r:any)=><div className="row" key={r.provider}>
    <strong>{r.provider}</strong>{r.ok?<span className="dim">answered in {Math.round(r.seconds)} s · {r.consulted} pages consulted</span>:<span className="bad">{r.error}</span>}
  </div>)}</div>;
}

/** The planner's best set beside each AI's pick: agreement, validity and the AI's critique. */
export function Comparison({result,titles}:{result:any;titles:(id:string)=>string}){
  const planner=result.planner;
  return <div className="stack comparison">
    <div><strong>Planner</strong> <span className="dim">· {planner.track_ids.length} tracks · {Math.round((planner.duration_seconds||0)/60)} min</span>
      <p className="hint">{planner.track_ids.map(titles).join(' → ')}</p></div>
    {result.ai.map((a:any)=><div key={a.provider} className="stack">
      <div className="row"><strong className="grow">{a.provider}</strong>
        {!a.ok?<Pill state="bad">Failed</Pill>:a.valid?<Pill state="good">Valid set</Pill>:<Pill state="warn">Rejected</Pill>}
        {a.ok&&<span className="dim">prefers {a.prefers==='own'?'its own set':"the planner's"}</span>}</div>
      {!a.ok&&<p className="bad">{a.error}</p>}
      {a.ok&&<>
        <p className="hint">{(a.track_ids||[]).map(titles).join(' → ')||'No tracks chosen'}</p>
        <p className="dim">{a.agreement.shared_tracks} shared tracks · {a.agreement.ai_only} new · same order {percent(a.agreement.order_agreement)}{a.agreement.same_opening?' · same opener':''}</p>
        {!a.valid&&<p className="bad">Not saved: {a.reason}</p>}
        <Disclosure summary="Critique of the planner's set"><p>{a.critique}</p></Disclosure>
        <Disclosure summary="Why this set"><p>{a.explanation}</p></Disclosure>
      </>}
    </div>)}
  </div>;
}
