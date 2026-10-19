import React from 'react';
import {emptyFilters} from './library';
import {Button,Field} from './ui';

export function activeFilterCount(value:typeof emptyFilters){
  return (Object.keys(emptyFilters) as (keyof typeof emptyFilters)[]).filter(k=>value[k]!==emptyFilters[k]).length;
}
export function LibraryFilters({value,onChange}:{value:typeof emptyFilters;onChange:(v:typeof emptyFilters)=>void}){
  const set=(key:keyof typeof emptyFilters)=>(e:React.ChangeEvent<HTMLInputElement|HTMLSelectElement>)=>onChange({...value,[key]:e.target.value});
  return <div className="filters-panel">
    <Field label="BPM"><div className="row" style={{flexWrap:'nowrap'}}><input aria-label="Minimum BPM" type="number" placeholder="Min" value={value.bpmMin} onChange={set('bpmMin')}/><input aria-label="Maximum BPM" type="number" placeholder="Max" value={value.bpmMax} onChange={set('bpmMax')}/></div></Field>
    <Field label="Energy"><div className="row" style={{flexWrap:'nowrap'}}><input aria-label="Minimum energy" type="number" placeholder="Min" value={value.energyMin} onChange={set('energyMin')}/><input aria-label="Maximum energy" type="number" placeholder="Max" value={value.energyMax} onChange={set('energyMax')}/></div></Field>
    <Field label="Key"><input placeholder="8A or A minor" value={value.key} onChange={set('key')}/></Field>
    <Field label="Genre"><input placeholder="Contains…" value={value.genre} onChange={set('genre')}/></Field>
    <Field label="Vocals"><select value={value.vocals} onChange={set('vocals')}>{['any','present','absent','unknown'].map(v=><option key={v} value={v}>{v[0].toUpperCase()+v.slice(1)}</option>)}</select></Field>
    <Field label="Analysis"><select value={value.analysis} onChange={set('analysis')}>{['any','pending','imported','complete','partial','missing'].map(v=><option key={v} value={v}>{v[0].toUpperCase()+v.slice(1)}</option>)}</select></Field>
    <div className="row between">
      <span className="hint">Tracks with unknown values never match a specific filter.</span>
      <Button size="sm" kind="ghost" disabled={!activeFilterCount(value)} onClick={()=>onChange({...emptyFilters})}>Reset</Button>
    </div>
  </div>;
}
