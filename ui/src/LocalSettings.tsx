import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Card,Field} from './ui';

export function ResourceSettings({run}:{run:any}){
  const [settings,setSettings]=useState<any>({});
  useEffect(()=>{let active=true;window.onset.call('settings').then(s=>{if(active)setSettings(s);}).catch(()=>{});return()=>{active=false;};},[]);
  const set=(key:string,numeric=false)=>(e:React.ChangeEvent<HTMLInputElement>)=>setSettings((s:any)=>({...s,[key]:numeric?+e.target.value:e.target.value}));
  return <Card title="Resources & caches" actions={<Button size="sm" onClick={()=>run(async()=>{for(const key of ['cache_dir','model_dir','light_jobs','heavy_jobs'])if(settings[key])await window.onset.call('settings',{key,value:settings[key]});},'Resource settings saved')}>Save</Button>}>
    <div className="form-grid">
      <Field label="Feature cache folder" hint="Blank uses the app data folder"><input placeholder="Default" value={settings.cache_dir??''} onChange={set('cache_dir')}/></Field>
      <Field label="Model folder" hint="Blank uses the app data folder"><input placeholder="Default" value={settings.model_dir??''} onChange={set('model_dir')}/></Field>
      <Field label="Light jobs at once" hint="1–4 · applies after restart"><input type="number" min="1" max="4" value={settings.light_jobs??2} onChange={set('light_jobs',true)}/></Field>
      <Field label="Heavy jobs at once" hint="1–2 · applies after restart"><input type="number" min="1" max="2" value={settings.heavy_jobs??1} onChange={set('heavy_jobs',true)}/></Field>
    </div>
  </Card>;
}

export function EnergyCompare({tracks,run}:{tracks:Track[];run:any}){
  const [a,setA]=useState(''),[b,setB]=useState(''),[sources,setSources]=useState<string[]>([]),[audioError,setAudioError]=useState('');
  useEffect(()=>{let active=true;setSources([]);setAudioError('');if(a&&b)Promise.all([window.onset.media(a),window.onset.media(b)]).then(values=>{if(active)setSources(values);}).catch(error=>{if(active)setAudioError(error.message);});return()=>{active=false;};},[a,b]);
  return <div className="stack">
    <div className="compare">{[[a,setA,'First track',0],[b,setB,'Second track',1]].map(([value,setter,label,i]:any)=><div key={label}>
      <Field label={label}><select value={value} onChange={e=>setter(e.target.value)}><option value="">Choose a track</option>{tracks.map(t=><option key={t.id} value={t.id}>{t.title}</option>)}</select></Field>
      {sources.length===2&&<audio controls preload="metadata" src={sources[i]}/>}
    </div>)}</div>
    {audioError&&<p className="error-text">{audioError}</p>}
    <div className="row">{['a','equal','b'].map(choice=><Button key={choice} disabled={!a||!b||a===b} onClick={()=>run(()=>window.onset.call('energy-judgment',{a,b,winner:choice==='equal'?'equal':choice==='a'?a:b}),'Energy comparison saved')}>{choice==='equal'?'About the same':choice==='a'?'First is stronger':'Second is stronger'}</Button>)}</div>
  </div>;
}
