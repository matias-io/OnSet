import React,{useEffect,useState} from 'react';
export function ExperimentalModels({run}:any){
 const [enabled,setEnabled]=useState<string[]>([]);
 useEffect(()=>{let active=true;window.autocue.call('settings').then(s=>{if(active)setEnabled(s.experimental_models||[]);}).catch(()=>{});return()=>{active=false;};},[]);
 async function change(id:string,on:boolean){const next=on?[...enabled,id]:enabled.filter(v=>v!==id);const previous=enabled;setEnabled(next);try{await window.autocue.call('settings',{key:'experimental_models',value:next});}catch(e){setEnabled(previous);throw e;}}
 return <section className="panel"><h3>Experimental structure sources</h3><p className="muted">These sources stay separate from resolved cues until evaluated on verified listening labels. Setup downloads local model assets and can take several minutes. Enabling a source adds it to subsequent deep jobs; failure remains visible as partial analysis.</p>{['edmformer','allin1'].map(id=><div className="toolbar" key={id}><label><input type="checkbox" checked={enabled.includes(id)} onChange={e=>run(()=>change(id,e.target.checked))}/> Include {id} opinions in deep analysis</label><button onClick={()=>run(()=>window.autocue.call('worker-setup',{model:id}),`${id} setup queued`)}>Set up isolated {id}</button></div>)}</section>;
}
