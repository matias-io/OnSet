import React,{useEffect,useState} from 'react';
import {Button,Card} from './ui';

const models=[{id:'edmformer',name:'EDMFormer',detail:'Section labels trained on electronic music'},{id:'allin1',name:'All-In-One',detail:'Boundary and function estimates'}];

export function ExperimentalModels({run}:any){
  const [enabled,setEnabled]=useState<string[]>([]);
  useEffect(()=>{let active=true;window.autocue.call('settings').then(s=>{if(active)setEnabled(s.experimental_models||[]);}).catch(()=>{});return()=>{active=false;};},[]);
  async function change(id:string,on:boolean){const next=on?[...enabled,id]:enabled.filter(v=>v!==id);const previous=enabled;setEnabled(next);try{await window.autocue.call('settings',{key:'experimental_models',value:next});}catch(e){setEnabled(previous);throw e;}}
  return <Card title="Experimental structure models">
    <p className="muted" style={{marginBottom:6}}>Extra opinions kept separate from your cues until they are evaluated. Setup downloads model files and can take several minutes.</p>
    {models.map(m=><div className="setting-row" key={m.id}>
      <input type="checkbox" aria-label={`Include ${m.name} in deep analysis`} checked={enabled.includes(m.id)} onChange={e=>run(()=>change(m.id,e.target.checked))}/>
      <div className="grow"><strong>{m.name}</strong><small>{m.detail}</small></div>
      <Button size="sm" onClick={()=>run(()=>window.autocue.call('worker-setup',{model:m.id}),`${m.name} setup queued`)}>Set up</Button>
    </div>)}
  </Card>;
}
