import React,{useEffect,useState} from 'react';
import type {Track} from './types';
import {Button,Card,Icon,Pill} from './ui';
const api=window.autocue;

// Your genres, the audio model's Discogs styles and suggestions learned from your own tags, side by side.
export function GenreEvidence({track,tracks,run}:{track:Track;tracks:Track[];run:any}){
  const [suggested,setSuggested]=useState<any>(null),[error,setError]=useState('');
  const yours:string[]=(track as any).overrides?.genres||[];
  const estimates:any[]=(track.analysis?.genre_estimates||[]).slice(0,5);
  useEffect(()=>{let active=true;setSuggested(null);setError('');
    api.call('genre-suggestions',{track_ids:[track.id]}).then(r=>{if(active)setSuggested(r);}).catch(e=>{if(active)setError(e.message);});
    return()=>{active=false;};},[track.id,track.revision,JSON.stringify(yours)]);
  const add=(genre:string)=>run(()=>api.call('edit',{id:track.id,changes:{genres:[...new Set([...yours,genre])]}}),`Added ${genre} to your genres`);
  const remove=(genre:string)=>run(()=>api.call('edit',{id:track.id,changes:{genres:yours.filter(g=>g!==genre)}}),`Removed ${genre}`);
  const title=(id:string)=>tracks.find(t=>t.id===id)?.title||id;
  const mine=suggested?.suggestions?.find((s:any)=>s.track_id===track.id);
  const has=(g:string)=>yours.some(y=>y.toLowerCase()===g.toLowerCase());
  return <Card title="Genre">
    <div className="stack">
      <div>
        <h4>Your genres</h4>
        {yours.length?<div className="chips">{yours.map(g=><span className="chip" key={g}>{g}<button type="button" aria-label={`Remove ${g}`} onClick={()=>remove(g)}><Icon name="x" size={12}/></button></span>)}</div>
          :<p className="muted">None yet. Genres you set here or in the profile train the suggestions below.</p>}
        {!yours.length&&track.genres.length>0&&<div className="row" style={{marginTop:6}}>
          <span className="muted">Tags from the file or an import:</span>
          {track.genres.map(g=><Button key={g} size="sm" kind="ghost" title="Confirm this tag as your own" onClick={()=>add(g)}>Confirm {g}</Button>)}
        </div>}
      </div>
      <div>
        <h4>Audio model <Pill state="unverified">unverified</Pill></h4>
        {estimates.length?<table className="label-table"><tbody>{estimates.map(e=><tr key={e.genre}>
          <td>{e.genre}{e.parent&&e.parent!=='Electronic'?<span className="dim"> · {e.parent}</span>:null}</td>
          <td style={{width:'40%'}}><span className="energy-meter"><i><b style={{width:`${Math.round(e.score*100)}%`}}/></i>{e.score.toFixed(2)}</span></td>
          <td><Button size="sm" kind="ghost" disabled={has(e.genre)} onClick={()=>add(e.genre)}>{has(e.genre)?'Added':'Add'}</Button></td></tr>)}</tbody></table>
          :<p className="muted">Run deep analysis to get Discogs-style estimates from the audio.</p>}
      </div>
      <div>
        <h4>From tracks you tagged <Pill state="unverified">unverified</Pill></h4>
        {error&&<p className="error-text">{error}</p>}
        {!suggested&&!error&&<p className="muted">Comparing with your tagged tracks…</p>}
        {suggested&&!suggested.tagged_tracks&&<p className="muted">Tag a few tracks per genre yourself; similar-sounding tracks then get suggestions.</p>}
        {suggested&&suggested.tagged_tracks>0&&yours.length>0&&<p className="muted">This track is one of your {suggested.tagged_tracks} tagged examples.</p>}
        {suggested&&suggested.tagged_tracks>0&&!yours.length&&(mine?.suggestions?.length?<table className="label-table"><tbody>{mine.suggestions.map((s:any)=><tr key={s.genre}>
          <td>{s.genre}<br/><small className="dim">Sounds like {s.examples.map(title).join(', ')}</small></td>
          <td>{s.score.toFixed(2)}</td>
          <td><Button size="sm" kind="ghost" onClick={()=>add(s.genre)}>Add</Button></td></tr>)}</tbody></table>
          :<p className="muted">No close match among your {suggested.tagged_tracks} tagged tracks{mine?'':' (this track has no analysis features yet)'}.</p>)}
      </div>
    </div>
  </Card>;
}
