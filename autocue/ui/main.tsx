import './devMock';
import React,{useEffect,useRef,useState} from 'react';
import {createRoot} from 'react-dom/client';
import type {Track,Cue} from './types';
import './style.css';
import {PlanCard} from './PlanCard';
import {DualAudition} from './DualAudition';
import {StructureEditor} from './StructureEditor';
import {EvidencePanel} from './EvidencePanel';
import {ResourceSettings,EnergyCompare} from './LocalSettings';
import {CueReview} from './CueReview';
import {StemPlayer} from './StemPlayer';
import {ImportPaths} from './ImportPaths';
import {BenchmarkReview} from './BenchmarkReview';
import {CueAdoption} from './CueAdoption';
import {ExperimentalModels} from './ExperimentalModels';
import {Relocate} from './Relocate';
import {LibraryFilters,activeFilterCount} from './LibraryFilters';
import {WorkspaceRecovery} from './WorkspaceRecovery';
import {PortableImport,CacheRelocation} from './Portable';
import {Button,Card,Disclosure,Empty,Field,Icon,KeyValues,Pill,Segmented,Stat,Tabs,TrackPicker} from './ui';
import {emptyFilters,matchesFilters,canonicalKey,analysisSelection,restoreBrief,pruneBriefTracks,snapGrid,nearest,roleColor,clock} from './library';

const api=window.autocue;
const keyLabel=(k:any)=>typeof k==='string'?k:k?.name||'';
function KeyBadge({value}:{value:any}){
  const camelot=canonicalKey(value),name=keyLabel(value);
  if(!camelot&&!name)return <span className="dim">—</span>;
  return <span className="key-badge">{camelot&&<b>{camelot}</b>}{name&&name!==camelot&&<span className="dim">{name}</span>}</span>;
}
function EnergyMeter({value}:{value:number|null|undefined}){
  if(value===null||value===undefined)return <span className="dim">—</span>;
  return <span className="energy-meter"><i><b style={{width:`${Math.max(0,Math.min(10,value))*10}%`}}/></i>{value}</span>;
}
const energyOf=(t:Track)=>t.energy??t.analysis?.energy_profile?.rating??null;

type Page='library'|'track'|'transitions'|'planner'|'settings';
const pages:{id:Page;label:string;icon:string}[]=[
  {id:'library',label:'Library',icon:'library'},{id:'track',label:'Track review',icon:'wave'},
  {id:'transitions',label:'Transitions',icon:'transition'},{id:'planner',label:'Set planner',icon:'set'},{id:'settings',label:'Settings',icon:'settings'}];

function Timeline({track,position,onSeek,onMove,onSection,snap}:{track:Track;position:number;onSeek:(n:number)=>void;onMove:(c:Cue,n:number)=>void;onSection:(sections:any[])=>void;snap:boolean}){
  const ref=useRef<HTMLCanvasElement>(null),drag=useRef<Cue|null>(null),sectionDrag=useRef<number|null>(null),markers=useRef<{cue:Cue;x:number;y:number}[]>([]);
  const [width,setWidth]=useState(0);
  useEffect(()=>{const canvas=ref.current;if(!canvas)return;const observer=new ResizeObserver(()=>setWidth(canvas.clientWidth));observer.observe(canvas);return()=>observer.disconnect();},[]);
  useEffect(()=>{
    const canvas=ref.current;if(!canvas||!canvas.clientWidth)return;
    const width=canvas.clientWidth,height=200,ratio=devicePixelRatio||1;
    canvas.width=width*ratio;canvas.height=height*ratio;
    const ctx=canvas.getContext('2d')!;ctx.scale(ratio,ratio);
    ctx.fillStyle='#0c0d10';ctx.fillRect(0,0,width,height);
    const duration=Math.max(1,track.duration),wave=track.analysis?.waveform||[],x=(t:number)=>t/duration*width;
    const sections=track.overrides?.sections||track.analysis?.structure?.sections||[];
    const sectionTone:Record<string,string>={intro:'#2a2f3a',outro:'#2a2f3a',groove:'#24314a',verse:'#2c2c4a',vocal:'#2c2c4a',build:'#3a2f48',drop:'#3b2a4f',breakdown:'#1f3a3c',stripped_groove:'#24314a',silence:'#1b1c21'};
    ctx.font='500 11px "Segoe UI Variable Text","Segoe UI",system-ui';
    sections.forEach((s:any)=>{const left=x(s.start),w=Math.max(0,x(s.end)-left);ctx.fillStyle=sectionTone[s.label]||'#262833';ctx.fillRect(left+1,0,w-2,24);ctx.save();ctx.beginPath();ctx.rect(left,0,w,24);ctx.clip();ctx.fillStyle='#c9c9d6';ctx.fillText(s.label.replace('_',' '),left+7,16);ctx.restore();});
    const mid=104,amp=62,played=x(position);
    wave.forEach((v:number,i:number)=>{const px=i/Math.max(1,wave.length)*width,h=Math.max(1,v*amp*2);ctx.fillStyle=px<played?'#b3a9ff':'#4b4770';ctx.fillRect(px,mid-h/2,Math.max(1,width/wave.length-.4),h);});
    const db=track.overrides?.grid||track.analysis?.downbeats||[];
    db.forEach((t:number,i:number)=>{if(i%4)return;ctx.strokeStyle=i%16===0?'rgba(255,255,255,.10)':'rgba(255,255,255,.04)';ctx.beginPath();ctx.moveTo(x(t),28);ctx.lineTo(x(t),172);ctx.stroke();});
    markers.current=[];
    track.cues.forEach(c=>{const cx=x(c.time),lane=markers.current.filter(m=>Math.abs(m.x-cx)<22).length,y=174-Math.min(lane,3)*22,labelX=Math.min(Math.max(0,cx-10),width-20),color=roleColor[c.role]||'#ffffff';
      markers.current.push({cue:c,x:labelX,y});
      if(c.end!==null){ctx.fillStyle=color+'22';ctx.fillRect(cx,28,x(c.end)-cx,144);}
      ctx.globalAlpha=c.review==='rejected'?.35:1;ctx.strokeStyle=color;ctx.lineWidth=c.review==='approved'?2:1;ctx.setLineDash(c.review==='approved'?[]:[3,3]);
      ctx.beginPath();ctx.moveTo(cx,28);ctx.lineTo(cx,y);ctx.stroke();ctx.setLineDash([]);
      ctx.fillStyle=color;ctx.beginPath();ctx.roundRect(labelX,y,20,20,5);ctx.fill();
      ctx.fillStyle='#0c0d10';ctx.font='700 11px "Segoe UI Variable Text","Segoe UI",system-ui';ctx.fillText(c.role,labelX+6.5,y+14);ctx.globalAlpha=1;});
    ctx.strokeStyle='#ffffff';ctx.lineWidth=1.5;ctx.beginPath();ctx.moveTo(played,24);ctx.lineTo(played,196);ctx.stroke();
    ctx.fillStyle='#71737f';ctx.font='11px "Cascadia Mono",Consolas,monospace';
    for(let i=1;i<4;i++)ctx.fillText(clock(duration*i/4,false),width*i/4-16,196);
  },[track,position,width]);
  function time(e:React.PointerEvent<HTMLCanvasElement>){const rect=e.currentTarget.getBoundingClientRect();let t=Math.max(0,Math.min(track.duration-.001,(e.clientX-rect.left)/rect.width*track.duration));if(snap)t=nearest(snapGrid(track),t);return t;}
  return <canvas ref={ref} className="timeline" role="img" aria-label="Waveform and draggable cue markers. Use the cue list to adjust cues with the keyboard."
    onPointerCancel={()=>{drag.current=null;sectionDrag.current=null;}}
    onPointerDown={e=>{drag.current=null;sectionDrag.current=null;const t=time(e);const sections=track.overrides?.sections||track.analysis?.structure?.sections||[];
      const boundary=sections.findIndex((v:any,i:number)=>i>0&&Math.abs(v.start-t)<track.duration/70);
      if(e.nativeEvent.offsetY<24&&boundary>0){sectionDrag.current=boundary;e.currentTarget.setPointerCapture(e.pointerId);return;}
      const near=markers.current.find(m=>e.nativeEvent.offsetX>=m.x&&e.nativeEvent.offsetX<=m.x+20&&e.nativeEvent.offsetY>=m.y&&e.nativeEvent.offsetY<=m.y+20)?.cue;
      if(near){drag.current=near;e.currentTarget.setPointerCapture(e.pointerId);}else onSeek(t);}}
    onPointerUp={e=>{if(sectionDrag.current!==null){const i=sectionDrag.current,ss=structuredClone(track.overrides?.sections||track.analysis?.structure?.sections||[]),t=time(e);if(t>ss[i-1].start&&t<ss[i].end){ss[i-1].end=t;ss[i].start=t;onSection(ss);}sectionDrag.current=null;}
      if(drag.current){onMove(drag.current,time(e));drag.current=null;}}}/>;
}

function Spectrogram({track}:{track:Track}){
  const ref=useRef<HTMLCanvasElement>(null);
  useEffect(()=>{const s=track.analysis?.spectrogram,c=ref.current;if(!s||!c)return;const rows=s.values.length,cols=s.values[0]?.length||0;if(!cols)return;c.width=cols;c.height=rows;
    const ctx=c.getContext('2d')!,img=ctx.createImageData(cols,rows);
    for(let y=0;y<rows;y++)for(let x=0;x<cols;x++){const v=Math.max(0,Math.min(1,(s.values[y][x]+70)/70)),i=((rows-1-y)*cols+x)*4;img.data[i]=30+225*Math.pow(v,1.4);img.data[i+1]=14+150*v*v*v;img.data[i+2]=40+170*v*(1-v)+60*v;img.data[i+3]=255;}
    ctx.putImageData(img,0,0);},[track]);
  const s=track.analysis?.spectrogram;
  if(!s)return <Empty icon="wave">Analyse the track to see its spectrogram.</Empty>;
  // Labels sit at their actual bin rows (bins are not linearly spaced).
  const freqs:number[]=s.frequencies,rows=freqs.length;
  const ticks=[100,500,1000,5000,10000].map(hz=>{const i=freqs.findIndex(f=>f>=hz);return i<0?null:{hz,top:100*(1-(i+.5)/rows)};}).filter(Boolean) as {hz:number;top:number}[];
  return <div className="spectro"><div className="frequency">{ticks.map(t=><span key={t.hz} style={{top:`${t.top}%`}}>{t.hz>=1000?`${t.hz/1000}k`:t.hz}</span>)}</div><canvas ref={ref}/></div>;
}

function App(){
  const [page,setPage]=useState<Page>('library'),[trackTab,setTrackTab]=useState('cues'),[settingsTab,setSettingsTab]=useState('jobs');
  const [tracks,setTracks]=useState<Track[]>([]),[selected,setSelected]=useState<string|null>(null),[marked,setMarked]=useState<string[]>([]),[query,setQuery]=useState(''),[filter,setFilter]=useState('all');
  const [error,setError]=useState(''),[message,setMessage]=useState(''),[busy,setBusy]=useState(false),[jobs,setJobs]=useState<any[]>([]),[account,setAccount]=useState<any>({accounts:[]}),[models,setModels]=useState<any[]>([]),[model,setModel]=useState('');
  const [position,setPosition]=useState(0),[source,setSource]=useState(''),[snap,setSnap]=useState(true),[click,setClick]=useState(false),[playing,setPlaying]=useState(false);
  const [target,setTarget]=useState(''),[transitions,setTransitions]=useState<any[]>([]),[preview,setPreview]=useState<any>(null),[plans,setPlans]=useState<any[]>([]),[writePreview,setWritePreview]=useState<any>(null),[doctor,setDoctor]=useState<any>(null),[manifests,setManifests]=useState<any[]>([]);
  const [brief,setBrief]=useState<any>(()=>restoreBrief(null)),briefLoaded=useRef(false);
  const [libraryFilters,setLibraryFilters]=useState({...emptyFilters}),[showFilters,setShowFilters]=useState(false),[showPaste,setShowPaste]=useState(false);
  const [catalogue,setCatalogue]=useState<any[]>([]),[research,setResearch]=useState(false),[stream,setStream]=useState('');
  const [priority,setPriority]=useState<Record<string,any>>({}),[planJob,setPlanJob]=useState<string|null>(null),planJobRef=useRef<string|null>(null);
  const audio=useRef<HTMLAudioElement>(null),audioContext=useRef<AudioContext|null>(null);
  const track=tracks.find(t=>t.id===selected)||null;
  useEffect(()=>{window.scrollTo({top:0});},[page,selected]);
  useEffect(()=>{if(!message)return;const timer=setTimeout(()=>setMessage(''),5000);return()=>clearTimeout(timer);},[message]);
  async function refresh(){const ts=await api.call('tracks');setTracks(ts);
    api.call('review-queue',{include_done:true}).then((rows:any[])=>setPriority(Object.fromEntries((rows||[]).map(r=>[r.track_id,r])))).catch(()=>setPriority({}));setSelected(s=>s||ts[0]?.id||null);setJobs(await api.call('jobs'));const saved=await api.call('sets');setPlans(previous=>previous.length?previous.map(plan=>saved.find((p:any)=>p.id===plan.id)||plan):saved);}
  async function run(action:()=>Promise<any>,success=''){setBusy(true);setError('');try{const result=await action();if(success)setMessage(success);await refresh();return result;}catch(e:any){setError(e.message);return null;}finally{setBusy(false);}}
  useEffect(()=>{if(!api)return;refresh().catch(e=>setError(e.message));
    api.call('catalogue').then(setCatalogue).catch(e=>setError(e.message));
    api.account('status').then(a=>{setAccount(a);if(a?.selected_model)setModel(a.selected_model);}).catch(e=>setError(e.message));
    api.call('settings').then(s=>{if(s?.set_brief)setBrief(restoreBrief(s.set_brief));}).catch(e=>setError(e.message)).finally(()=>{briefLoaded.current=true;});
    return api.onEvent(event=>{
      if(event.method==='job-progress'){setJobs(js=>[event.params,...js.filter(j=>j.id!==event.params.id)]);if(['complete','partial','failed'].includes(event.params.state))refresh().catch(e=>setError(e.message));
        if(event.params.id===planJobRef.current&&['complete','failed','cancelled'].includes(event.params.state))finishPlanJob(event.params);}
      if(event.method==='import-progress')setMessage(event.params.total?`${event.params.stage}: ${event.params.completed}/${event.params.total} · ${event.params.imported} imported · ${event.params.skipped} skipped · ${event.params.failed} failed`:event.params.stage);
      if(event.method==='ai-progress')setStream(s=>s+event.params.delta);});},[]);
  useEffect(()=>{if(selected)api.media(selected).then(url=>{setSource(url);setPosition(0);setPlaying(false);}).catch(()=>setSource(''));},[selected]);
  function seek(t:number){if(audio.current){audio.current.currentTime=t;}setPosition(t);}
  function tick(){if(audio.current)setPosition(audio.current.currentTime);}
  function togglePlay(){if(!audio.current)return;if(audio.current.paused)audio.current.play().catch(e=>setError(e.message));else audio.current.pause();}
  // Keep the planner brief across sessions; wait for the saved value before writing so defaults never overwrite it.
  useEffect(()=>{if(!api||!briefLoaded.current)return;const timer=setTimeout(()=>api.call('settings',{key:'set_brief',value:brief}).catch((e:any)=>setError(e.message)),500);return()=>clearTimeout(timer);},[brief]);
  useEffect(()=>{if(tracks.length)setBrief((b:any)=>pruneBriefTracks(b,tracks.map(t=>t.id)));},[tracks]);
  useEffect(()=>{const handle=(event:KeyboardEvent)=>{const el=event.target as HTMLElement;if(page!=='track'||!track||el?.closest('input,textarea,select,[contenteditable=true]'))return;
    if((event.ctrlKey||event.metaKey)&&!event.shiftKey&&event.key.toLowerCase()==='z'){event.preventDefault();run(()=>api.call('undo',{id:track.id}));return;}
    if(el?.closest('button'))return;
    if(event.code==='Space'){event.preventDefault();togglePlay();}
  };window.addEventListener('keydown',handle);return()=>window.removeEventListener('keydown',handle);},[page,track]);

  useEffect(()=>{
    if(!click||!playing||!track)return;
    const context=audioContext.current||(audioContext.current=new AudioContext());
    context.resume();let previous=-1;const pending:OscillatorNode[]=[];
    const schedule=()=>{const player=audio.current;if(!player||player.paused||player.seeking)return;
      const time=player.currentTime,rate=player.playbackRate;
      const beats=track.analysis?.beats||[];
      beats.forEach((beat:number,index:number)=>{const delta=(beat-time)/rate;
        if(delta<0||delta>.075||index===previous)return;
        previous=index;const at=context.currentTime+delta;
        const oscillator=context.createOscillator(),gain=context.createGain();
        const down=(track.analysis?.downbeats||[]).some((t:number)=>Math.abs(t-beat)<.03);
        oscillator.frequency.value=down?1700:1100;gain.gain.setValueAtTime(.07,at);
        gain.gain.exponentialRampToValueAtTime(.001,at+.035);
        oscillator.connect(gain).connect(context.destination);oscillator.start(at);oscillator.stop(at+.04);
        pending.push(oscillator);oscillator.onended=()=>{oscillator.disconnect();gain.disconnect();const i=pending.indexOf(oscillator);if(i>=0)pending.splice(i,1);};
      });};
    const timer=setInterval(schedule,25);schedule();
    return()=>{clearInterval(timer);pending.forEach(o=>{try{o.stop();}catch{}});};
  },[click,playing,track]);
  async function pick(kind:string){const paths=await api.pick(kind);if(paths.length)await run(()=>api.call('import',{paths}),'Tracks imported');}
  async function connect(action:string,params:any={}){if(action==='sign-in')setAccount((a:any)=>({...a,pending:true}));const result=await run(async()=>{const a=await api.account(action,params);if(a?.accounts){setAccount(a);setModel(a.selected_model||'');}return a;});if(!result)setAccount((a:any)=>({...a,pending:false}));return result;}
  async function loadModels(){return run(async()=>{const ms=await api.account('models');setModels(ms);setModel(old=>ms.some((m:any)=>m.id===old)?old:ms[0]?.id||'');return ms;});}
  function setField(name:string,value:any){setBrief((b:any)=>({...b,[name]:value}));}
  // Set searches run as cancellable jobs; results arrive through job-progress events.
  function trackPlanJob(id:string|null){planJobRef.current=id;setPlanJob(id);}
  async function finishPlanJob(job:any){trackPlanJob(null);
    if(job.state==='cancelled'){setMessage('Set search cancelled');return;}
    if(job.state==='failed'){setError(job.error||'Set search failed');return;}
    const saved=await api.call('sets'),ids:string[]=job.result?.plan_ids||[];
    const found=ids.map(id=>saved.find((p:any)=>p.id===id)).filter(Boolean);setPlans(found);
    if(job.result?.conflicts?.length)setError(job.result.conflicts.join(' '));else setMessage(found.length===1?"1 set option ready":`${found.length} set options ready`);}
  async function buildPlans(){const job=await run(()=>api.call('plan-job',{brief:{...brief,track_ids:ids}}));if(!job)return;
    if(['complete','failed','cancelled'].includes(job.state))finishPlanJob(job);else trackPlanJob(job.id);}
  async function replan(plan:any,keep:number){const r=await run(()=>api.call('replan',{id:plan.id,keep,brief:{...brief,track_ids:plan.brief?.track_ids||ids}}));
    if(!r)return;if(r.plans.length){setPlans(r.plans);setMessage(`Kept the first ${keep} tracks and replanned the rest`);}if(r.conflicts.length)setError(r.conflicts.join(' '));}
  function openTrack(id:string){setSelected(id);setPage('track');}
  const unsorted=tracks.filter(t=>matchesFilters(t,libraryFilters)&&(`${t.title} ${t.artist} ${t.genres.join(' ')} ${keyLabel(t.key)} ${canonicalKey(t.key)||''} ${t.bpm||''}`).toLowerCase().includes(query.toLowerCase())&&(filter==='all'||filter==='review'&&t.review_state!=='reviewed'||filter==='ready'&&t.analysis_state==='complete'));
  // The review view puts disagreements, stale cues and high-impact cues first.
  const filtered=filter==='review'?[...unsorted].sort((a,b)=>(priority[b.id]?.score||0)-(priority[a.id]?.score||0)):unsorted;
  const playlistNames=[...new Set(tracks.flatMap(t=>(t as any).rekordbox_playlists||[]))].sort();
  const ids=analysisSelection(tracks,filtered,marked);
  const hiddenSelected=ids.filter(id=>!filtered.some(t=>t.id===id)).length;
  const activeJobs=jobs.filter(j=>['running','queued'].includes(j.state));
  const accountProps={account,connect,models,model,setModel:(id:string)=>{setModel(id);connect('model',{id});},loadModels};
  if(!api)return <main className="standalone"><h1>Autocue</h1><p>Open the desktop application to connect your local library.</p></main>;

  const head=(title:React.ReactNode,actions?:React.ReactNode)=><div className="page-head"><h1>{title}</h1>{actions&&<div className="row">{actions}</div>}</div>;

  return <div className="shell">
    {busy&&<div className="busy-bar" role="progressbar" aria-label="Working"/>}
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><Icon name="headphones" size={16}/></span><span>Autocue</span></div>
      <nav className="nav" aria-label="Main">{pages.map(p=><button key={p.id} className={page===p.id?'active':''} aria-current={page===p.id?'page':undefined} onClick={()=>setPage(p.id)} title={p.label}>
        <Icon name={p.icon} size={18}/><span className="label">{p.label}</span>{p.id==='settings'&&activeJobs.length>0&&<span className="badge" aria-label={`${activeJobs.length} active jobs`}>{activeJobs.length}</span>}</button>)}</nav>
      <div className="sidebar-foot">
        <strong>{tracks.length} tracks</strong>
        <span>{tracks.filter(t=>t.review_state==='reviewed').length} reviewed · {tracks.reduce((n,t)=>n+t.cues.filter(c=>c.review==='approved').length,0)} cues approved</span>
        {activeJobs[0]&&<button className="btn ghost sm" style={{justifyContent:'flex-start',padding:0,marginTop:6}} onClick={()=>{setPage('settings');setSettingsTab('jobs');}}>{({plan:'Set search',transitions:'Transition search','worker-setup':'Worker setup'} as any)[activeJobs[0].kind]||'Analysing'} · {Math.round((activeJobs[0].progress||0)*100)}%</button>}
      </div>
    </aside>

    <main className="content">
      {page==='library'&&<>
        {head('Library',<>
          <Button kind="ghost" icon="plus" onClick={()=>setShowPaste(!showPaste)}>Paste paths</Button>
          <Button icon="folder" onClick={()=>pick('folder')}>Add folder</Button>
          <Button onClick={()=>run(async()=>{const result=await api.call('rekordbox-import',{});setMessage(`Imported ${result.imported} rekordbox records · ${result.skipped?.length||0} skipped · ${result.failed?.length||0} failed`);return result;})}>Read rekordbox</Button>
          <Button kind="primary" icon="upload" onClick={()=>pick('files')}>Import files</Button></>)}
        {showPaste&&<ImportPaths run={run} onClose={()=>setShowPaste(false)}/>}
        {tracks.length>0&&<div className="stats">
          <Stat label="Tracks" value={tracks.length}/>
          <Stat label="Analysed" value={tracks.filter(t=>t.analysis_state==='complete').length}/>
          <Stat label="Need review" value={tracks.filter(t=>t.review_state!=='reviewed').length}/>
          <Stat label="Cues approved" value={tracks.reduce((n,t)=>n+t.cues.filter(c=>c.review==='approved').length,0)}/>
        </div>}
        <div className="toolbar">
          <div className="search"><Icon name="search"/><input aria-label="Search library" placeholder="Search title, artist, genre, key or BPM" value={query} onChange={e=>setQuery(e.target.value)}/></div>
          <Segmented label="Library view" value={filter} onChange={setFilter} options={[{id:'all',label:'All'},{id:'review',label:'Review queue'},{id:'ready',label:'Analysed'}]}/>
          <Button kind={showFilters||activeFilterCount(libraryFilters)?'':'ghost'} icon="filter" onClick={()=>setShowFilters(!showFilters)}>Filters{activeFilterCount(libraryFilters)?` · ${activeFilterCount(libraryFilters)}`:''}</Button>
        </div>
        {showFilters&&<LibraryFilters value={libraryFilters} onChange={setLibraryFilters}/>}
        {tracks.length>0&&<div className="selection-bar">
          <span>{marked.length?<><b>{ids.length}</b> checked{hiddenSelected?` · ${hiddenSelected} hidden by filters`:''}</>:<>Analyse <b>{ids.length}</b> visible tracks</>}</span>
          {marked.length>0&&<Button kind="ghost" size="sm" onClick={()=>setMarked([])}>Clear</Button>}
          <Button size="sm" disabled={busy||!ids.length} onClick={()=>run(()=>api.call('analyze',{track_ids:ids,deep:false}),'Light analysis queued')}>Light analysis</Button>
          <Button size="sm" kind="primary" icon="spark" disabled={busy||!ids.length} onClick={()=>run(()=>api.call('analyze',{track_ids:ids,deep:true}),'Deep analysis queued')}>Deep analysis</Button>
        </div>}
        <div className="table-wrap">
          {tracks.length>0&&<table>
            <thead><tr>
              <th className="check-cell"><input type="checkbox" aria-label="Select visible tracks" checked={!!filtered.length&&filtered.every(t=>marked.includes(t.id))} onChange={e=>setMarked(m=>e.target.checked?[...new Set([...m,...filtered.map(t=>t.id)])]:m.filter(id=>!filtered.some(t=>t.id===id)))}/></th>
              <th>Track</th><th>BPM</th><th>Key</th><th>Energy</th><th>Analysis</th><th>Review</th></tr></thead>
            <tbody>{filtered.map(t=><tr key={t.id} className={marked.includes(t.id)?'selected':''} onDoubleClick={()=>openTrack(t.id)}>
              <td className="check-cell"><input type="checkbox" aria-label={`Select ${t.title}`} checked={marked.includes(t.id)} onChange={e=>setMarked(m=>e.target.checked?[...m,t.id]:m.filter(i=>i!==t.id))}/></td>
              <td><button className="track-cell" onClick={()=>openTrack(t.id)}><strong>{t.title}</strong><small>{filter==='review'&&priority[t.id]?.reasons?.length?priority[t.id].reasons[0]+(priority[t.id].reasons.length>1?` · +${priority[t.id].reasons.length-1} more`:''):t.artist||t.version||clock(t.duration,false)}</small></button></td>
              <td>{t.bpm?.toFixed(1)||<span className="dim">—</span>}</td>
              <td><KeyBadge value={t.key}/></td>
              <td><EnergyMeter value={energyOf(t)}/></td>
              <td><Pill state={t.analysis_state}>{t.analysis_state}</Pill></td>
              <td><Pill state={t.review_state}>{t.review_state}</Pill></td>
            </tr>)}</tbody>
          </table>}
          {tracks.length>0&&!filtered.length&&<Empty icon="search">No tracks match these filters.</Empty>}
          {!tracks.length&&<Empty icon="library"><h2>Your collection starts here</h2><p>Import audio, a historical analysis file or your rekordbox library. Files stay where they are.</p><Button kind="primary" icon="upload" onClick={()=>pick('files')}>Import files</Button></Empty>}
        </div>
      </>}

      {page==='track'&&(!track?<Empty icon="wave">Select a track from your library.</Empty>:<>
        <div className="track-head">
          <div style={{minWidth:0}}>
            <h1>{track.title}</h1>
            <p>{[track.artist,track.version,clock(track.duration,false)].filter(Boolean).join(' · ')}</p>
            <div className="facts">
              <Stat label="Tempo" value={track.bpm?.toFixed(1)||'—'} unit={track.bpm?'BPM':undefined}/>
              <Stat label="Key" value={<KeyBadge value={track.key}/>}/>
              <Stat label="Energy" value={energyOf(track)??'—'}/>
              <Stat label="Review" value={<Pill state={track.review_state}>{track.review_state}</Pill>}/>
            </div>
          </div>
          <select aria-label="Choose track" value={selected||''} onChange={e=>setSelected(e.target.value)}>{tracks.map(t=><option key={t.id} value={t.id}>{t.title}</option>)}</select>
        </div>
        <Card className="wave-card">
          <Timeline onSection={sections=>run(()=>api.call('edit',{id:track.id,changes:{sections}}),'Section boundary corrected')} track={track} position={position} onSeek={seek} snap={snap} onMove={(c,t)=>run(()=>api.call('review',{id:track.id,cue_id:c.id,status:'proposed',position:t}),'Cue moved; approve when ready')}/>
          <div className="transport">
            <Button kind="primary" className="play-btn" icon={playing?'pause':'play'} label={playing?'Pause':'Play'} onClick={togglePlay}/>
            <span className="time">{clock(position)} / {clock(track.duration)}</span>
            <label className="check"><input type="checkbox" checked={snap} onChange={e=>setSnap(e.target.checked)}/>Snap to beats</label>
            <label className="check"><input type="checkbox" checked={click} onChange={e=>setClick(e.target.checked)}/>Click track</label>
            <span className="hint spacer">Drag a cue letter to move it · drag a section edge to correct it</span>
          </div>
          <audio ref={audio} src={source} onTimeUpdate={tick} onPlay={()=>setPlaying(true)} onPause={()=>setPlaying(false)} onEnded={()=>setPlaying(false)}/>
        </Card>
        <div style={{marginTop:20}}><Tabs value={trackTab} onChange={setTrackTab} options={[{id:'cues',label:'Cues',count:track.cues.length},{id:'profile',label:'Profile'},{id:'evidence',label:'Evidence'},{id:'stems',label:'Stems'},{id:'corrections',label:'Corrections'}]}/></div>
        {trackTab==='cues'&&<>
          <CueReview track={track} run={run} seek={seek} play={()=>audio.current?.play().catch(e=>setError(e.message))}/>
          <Card title="Write to rekordbox" actions={<>
            <Button size="sm" onClick={()=>run(async()=>{const r=await api.call('rekordbox-preview',{track_ids:[track.id]});setWritePreview(r);return r;})}>Preview memory cues</Button>
            <Button size="sm" onClick={()=>run(async()=>{const r=await api.call('rekordbox-preview',{track_ids:[track.id],hot:true});setWritePreview(r);return r;})}>Preview hot cues</Button></>}>
            <p className="muted">Only approved cues are written, after you check the exact changes. Close rekordbox first; a backup is saved.</p>
            {writePreview&&<div className="write-preview">
              <WriteChanges value={writePreview} tracks={tracks}/>
              <div className="row end" style={{marginTop:12}}>
                <Button kind="ghost" size="sm" onClick={()=>setWritePreview(null)}>Discard</Button>
                <Button kind="primary" size="sm" disabled={busy||writePreview.stale||!writePreview.changes.length} onClick={()=>run(async()=>{const r=await api.call('rekordbox-apply',{id:writePreview.id});setWritePreview(null);return r;},'Approved changes written; backup saved')}>Apply these changes</Button>
              </div>
            </div>}
          </Card>
        </>}
        {trackTab==='profile'&&<Card title="Profile"><Metadata key={track.id+track.revision+JSON.stringify(track.overrides)} track={track} onSave={changes=>run(()=>api.call('edit',{id:track.id,changes}),'Profile updated')}/></Card>}
        {trackTab==='evidence'&&<><EvidencePanel track={track}/><Card title="Spectrogram"><Spectrogram track={track}/></Card></>}
        {trackTab==='stems'&&<StemPlayer track={track} position={position} playOriginal={()=>audio.current?.play().catch(e=>setError(e.message))} pauseOriginal={()=>audio.current?.pause()}/>}
        {trackTab==='corrections'&&<>
          <StructureEditor key={track.id+track.revision+JSON.stringify(track.overrides?.sections)} track={track} onSave={changes=>run(()=>api.call('edit',{id:track.id,changes}),'Analysis corrections saved')}/>
          <Card title="More tools">
            <Disclosure summary="Locate moved audio file"><Relocate track={track} run={run}/></Disclosure>
            <Disclosure summary="Adopt earlier Autocue cues in rekordbox"><CueAdoption track={track} run={run}/></Disclosure>
            <Disclosure summary="Benchmark labels (listening-verified)"><BenchmarkReview track={track} run={run} position={position}/></Disclosure>
            <Disclosure summary="Raw analysis data"><pre>{JSON.stringify({grid:track.analysis?.grid_observation,key:track.analysis?.key,local_keys:track.analysis?.local_keys,events:track.analysis?.events,deep:track.analysis?.deep,flags:track.analysis?.flags,observations:(track as any).observations},null,2)}</pre></Disclosure>
          </Card>
        </>}
      </>)}

      {page==='transitions'&&<>
        {head('Transitions')}
        <Card>
          <div className="pair-select">
            <Field label="Outgoing"><select value={selected||''} onChange={e=>setSelected(e.target.value)}><option value="">Choose a track</option>{tracks.map(t=><option key={t.id} value={t.id}>{t.title}</option>)}</select></Field>
            <span className="pair-arrow"><Icon name="chevron" size={20}/></span>
            <Field label="Incoming"><select value={target} onChange={e=>setTarget(e.target.value)}><option value="">Best matches in library</option>{tracks.filter(t=>t.id!==selected).map(t=><option key={t.id} value={t.id}>{t.title}</option>)}</select></Field>
            <Button kind="primary" disabled={!selected||busy} onClick={()=>run(async()=>{const r=await api.call('transitions',{from:selected,to:target||undefined,brief});setTransitions(r);return r;})}>Find options</Button>
          </div>
        </Card>
        {preview&&<Card title="Rehearsal render" actions={<Button kind="ghost" size="sm" icon="x" label="Close preview" onClick={()=>setPreview(null)}/>}><audio controls src={preview.url} style={{width:'100%'}}/>{preview.note&&<p className="muted" style={{marginTop:8}}>{preview.note}</p>}</Card>}
        {transitions.length>0&&<p className="hint" style={{marginTop:16}}>Timings come from the analysed grid. Listen to every option before relying on it.</p>}
        <div className="option-grid">{transitions.map(o=><Card key={o.id}>
          <div className="option-title"><div><h3>{o.name}</h3><small>into {tracks.find(t=>t.id===o.track_ids[1])?.title}</small></div>{typeof o.score==='number'&&<span className="score" title="Suitability score">{o.score.toFixed(1)}</span>}</div>
          <div className="timing">
            <div><small>Outgoing from</small><strong>{clock(o.out_start,false)}</strong></div>
            <div><small>Incoming from</small><strong>{clock(o.in_start,false)}</strong></div>
            <div><small>Handover</small><strong>{o.wall_seconds.toFixed(0)}s</strong></div>
          </div>
          <div className="meta-line"><span>Pitch {o.pitch_pct>=0?'+':''}{o.pitch_pct.toFixed(2)}%</span><span>Key lock {o.key_lock?'on':'off'}</span><span>{o.reason.split(';').slice(-1)[0].trim()}</span></div>
          <ol className="steps">{o.actions.map((a:any,i:number)=><li key={i}><span>+{a.at.toFixed(0)}s</span>{a.action}</li>)}</ol>
          {o.warnings.map((w:string)=><p key={w} className="warning"><Icon name="alert" size={14}/>{w}</p>)}
          <div className="option-foot">
            <Button size="sm" icon="play" disabled={!o.preview_supported||busy} title={o.preview_supported?undefined:'This gesture has to be rehearsed manually'} onClick={()=>run(async()=>{const r=await api.previewMedia(o.id,click);setPreview(r);return r;})}>{o.preview_supported?'Render preview':'Manual only'}</Button>
          </div>
          <DualAudition option={o}/>
        </Card>)}</div>
        {!transitions.length&&<Empty icon="transition">Pick an outgoing track to see timed ways into the next one.</Empty>}
      </>}

      {page==='planner'&&<>
        {head('Set planner',<Button kind="ghost" icon="download" onClick={()=>run(async()=>{const files=await api.pick('json');if(!files.length)return;const r=await api.call('import-plan-file',{path:files[0]});setPlans([r]);})}>Import plan</Button>)}
        <div className="planner">
          <Card title="Brief">
            <div className="form-grid three">
              <Field label="Length (min)"><input type="number" min="1" value={brief.target_seconds/60} onChange={e=>setField('target_seconds',+e.target.value*60)}/></Field>
              <Field label="Tolerance (min)"><input type="number" min="0" value={brief.tolerance_seconds/60} onChange={e=>setField('tolerance_seconds',+e.target.value*60)}/></Field>
              <Field label="Max pitch (%)"><input type="number" min="0" max="50" value={brief.max_pitch_pct} onChange={e=>setField('max_pitch_pct',+e.target.value)}/></Field>
              <Field label="Opening track"><select value={brief.start||''} onChange={e=>setField('start',e.target.value||null)}><option value="">Any</option>{tracks.map(t=><option value={t.id} key={t.id}>{t.title}</option>)}</select></Field>
              <Field label="Closing track"><select value={brief.finish||''} onChange={e=>setField('finish',e.target.value||null)}><option value="">Any</option>{tracks.map(t=><option value={t.id} key={t.id}>{t.title}</option>)}</select></Field>
              <Field label="Technique level"><select value={brief.skill} onChange={e=>setField('skill',+e.target.value)}><option value="1">Foundational</option><option value="2">Intermediate</option><option value="3">Advanced</option></select></Field>
              <Field label="Energy arc (1–10)" hint="Comma separated, start to finish"><input key={brief.energy_arc.join(',')} defaultValue={brief.energy_arc.join(', ')} onBlur={e=>setField('energy_arc',e.target.value.split(',').map(Number))}/></Field>
              <Field label="Mood"><input placeholder="e.g. driving, warm" value={brief.mood} onChange={e=>setField('mood',e.target.value)}/></Field>
              <Field label="Key lock"><label className="check" style={{height:34}}><input type="checkbox" checked={brief.key_lock} onChange={e=>setField('key_lock',e.target.checked)}/>Keep original key</label></Field>
              <Field label={`Smooth blends · ${Math.round(brief.smoothness*100)}%`} hint="Prefer long, level overlaps"><input type="range" min="0" max="1" step=".1" value={brief.smoothness} onChange={e=>setField('smoothness',+e.target.value)}/></Field>
              <Field label={`Contrast · ${Math.round(brief.contrast*100)}%`} hint="Prefer cuts, drops and energy changes"><input type="range" min="0" max="1" step=".1" value={brief.contrast} onChange={e=>setField('contrast',+e.target.value)}/></Field>
              <Field label="rekordbox playlists" hint={playlistNames.length?`Known: ${playlistNames.slice(0,4).join(', ')}${playlistNames.length>4?'…':''}`:'Read rekordbox to load playlist membership'}><input key={brief.playlists.join('|')} placeholder="Any playlist" defaultValue={brief.playlists.join(', ')} onBlur={e=>setField('playlists',e.target.value.split(',').map(x=>x.trim()).filter(Boolean))}/></Field>
              <TrackPicker label="Must play" tracks={tracks} value={brief.must_play} onChange={v=>setField('must_play',v)}/>
              <TrackPicker label="Exclude" tracks={tracks} value={brief.excluded} onChange={v=>setField('excluded',v)}/>
            </div>
            <div style={{marginTop:6}}>
              <Disclosure summary={`Techniques & equipment${brief.techniques.length?` · ${brief.techniques.length} selected`:''}`}>
                <p className="muted" style={{marginBottom:10}}>Leave techniques empty to allow everything your equipment and level support.</p>
                <div className="form-grid three">{catalogue.map(t=><label className="check" key={t.id}><input type="checkbox" checked={brief.techniques.includes(t.id)} onChange={e=>setField('techniques',e.target.checked?[...brief.techniques,t.id]:brief.techniques.filter((id:string)=>id!==t.id))}/>{t.name}</label>)}</div>
                <hr className="divider"/>
                <div className="form-grid three">{['eq','filter','loop','echo','reverb','stems','hot_cues','tempo'].map(equipment=><label className="check" key={equipment}><input type="checkbox" checked={brief.equipment.includes(equipment)} onChange={e=>setField('equipment',e.target.checked?[...brief.equipment,equipment]:brief.equipment.filter((x:string)=>x!==equipment))}/>{equipment.replace('_',' ')}</label>)}</div>
              </Disclosure>
            </div>
            <div className="row" style={{marginTop:14}}>
              <span className="muted grow">Plans from {ids.length} {marked.length?'checked':'visible'} library tracks</span>
              <Button kind="ghost" onClick={()=>run(()=>api.export({kind:'ai-context',brief:{...brief,track_ids:ids},track_ids:ids,name:'autocue-chatgpt-context.json'}))}>Export context</Button>
              {planJob?<Button onClick={()=>run(()=>api.call('job-control',{id:planJob,action:'cancel'}))}>Cancel search</Button>
                :<Button kind="primary" disabled={busy||!ids.length} onClick={buildPlans}>Build 3 options</Button>}
            </div>
          </Card>
          <Card title="ChatGPT adviser">
            {account.active?<div className="stack">
              <Field label="Model"><div className="row" style={{flexWrap:'nowrap'}}><select className="grow" value={model} onChange={e=>accountProps.setModel(e.target.value)}><option value="">Select a model</option>{models.map((m:any)=><option key={m.id} value={m.id}>{m.name}</option>)}{model&&!models.some((m:any)=>m.id===model)&&<option value={model}>{model}</option>}</select><Button icon="refresh" label="Load models" onClick={loadModels}/></div></Field>
              <label className="check"><input type="checkbox" checked={research} onChange={e=>setResearch(e.target.checked)}/>Research exact releases online</label>
              <div className="row">
                <Button kind="primary" disabled={!model||busy} onClick={()=>run(async()=>{setStream('');const r=await api.account('request',{task:'set',track_ids:ids,brief:{...brief,track_ids:ids},model,research});if(r.alternatives){setPlans(r.alternatives);setError(`ChatGPT proposal rejected: ${r.rejected_ai}. Showing validated local replacements. ${r.conflicts.join(' ')}`);}else{setPlans([r]);setMessage('ChatGPT proposal validated');}return r;})}>Suggest a set</Button>
                <Button disabled={!model||busy} onClick={()=>run(async()=>{setStream('');return api.account('request',{task:'enrichment',track_ids:ids,model,research});},'Track descriptions saved with provenance')}>Enrich profiles</Button>
              </div>
              <p className="hint">Sends selected profiles and validated options — never audio.</p>
              {stream&&<Disclosure summary="Response"><pre className="adviser-stream">{stream}</pre></Disclosure>}
            </div>:<div className="stack">
              <p className="muted">Connect a ChatGPT account to get set suggestions and sourced track descriptions.</p>
              <div className="row"><Button kind="primary" onClick={()=>connect('sign-in')}>Continue with ChatGPT</Button>{account.pending&&<Button kind="ghost" onClick={()=>connect('cancel')}>Cancel</Button>}</div>
            </div>}
          </Card>
        </div>
        {plans.length>0&&<div className="plans">{plans.map((plan,index)=><PlanCard key={plan.id} plan={plan} index={index} tracks={tracks} run={run} pinned={brief.pinned}
          onPin={(id:string,position:number,enabled:boolean)=>setField('pinned',enabled?[...brief.pinned.filter((p:any)=>p.position!==position),{track_id:id,position}]:brief.pinned.filter((p:any)=>p.track_id!==id||p.position!==position))}
          onPinTransition={(id:string,enabled:boolean)=>setField('pinned',enabled?[...brief.pinned,{transition_id:id}]:brief.pinned.filter((p:any)=>p.transition_id!==id))}
          onUpdated={(updated:any)=>setPlans(ps=>ps.map(p=>p.id===plan.id?updated:p))}
          onReplan={(keep:number)=>replan(plan,keep)} transpose={brief.transpose}
          onTranspose={(id:string,value:number)=>{const next={...brief.transpose};if(value)next[id]=value;else delete next[id];setField('transpose',next);setMessage('Key shift saved; rebuild or replan to apply it');}}
          onUseBrief={()=>{setBrief(pruneBriefTracks(restoreBrief(plan.brief),tracks.map(t=>t.id)));setMessage('Brief restored from this plan');}}/>)}</div>}
      </>}

      {page==='settings'&&<>
        {head('Settings')}
        <div className="settings">
          <nav className="settings-nav" aria-label="Settings sections">{[['jobs','Jobs'],['account','ChatGPT'],['analysis','Analysis engine'],['energy','Energy calibration'],['data','Data & backups']].map(([id,label])=><button key={id} className={settingsTab===id?'active':''} onClick={()=>setSettingsTab(id)}>{label}</button>)}</nav>
          <div>
            {settingsTab==='jobs'&&<Card title="Analysis jobs" actions={<Button size="sm" kind="ghost" onClick={()=>api.call('quit')}>Checkpoint jobs and quit</Button>}>
              {jobs.map(j=><div className="job" key={j.id}>
                <div className="job-head"><strong>{({analyze:j.params?.deep===false?'Light analysis':'Deep analysis','worker-setup':'Worker setup',plan:'Set search',transitions:'Transition search'} as any)[j.kind]||j.kind}{j.params?.track_ids?` · ${j.params.track_ids.length} ${j.params.track_ids.length===1?'track':'tracks'}`:''}</strong><Pill state={j.state}>{j.state}</Pill>
                  <div className="row spacer">
                    {['running','queued'].includes(j.state)&&<Button size="sm" icon="pause" onClick={()=>run(()=>api.call('job-control',{id:j.id,action:'pause'}))}>Pause</Button>}
                    {['paused','partial','failed','cancelled'].includes(j.state)&&<Button size="sm" icon="play" onClick={()=>run(()=>api.call('job-control',{id:j.id,action:'resume'}))}>Resume</Button>}
                    {['queued','running','paused'].includes(j.state)&&<Button size="sm" kind="ghost" onClick={()=>run(()=>api.call('job-control',{id:j.id,action:'cancel'}))}>Cancel</Button>}
                  </div></div>
                <div className="progress"><b style={{width:`${Math.round((j.progress||0)*100)}%`}}/></div>
                <small>{j.stage} · {Math.round((j.progress||0)*100)}%</small>
                {j.error&&<p className="warning"><Icon name="alert" size={14}/>{j.error}</p>}
                {j.failures?.map((f:any)=><p className="warning" key={f.track_id}><Icon name="alert" size={14}/>{tracks.find(t=>t.id===f.track_id)?.title}: {f.error}</p>)}
              </div>)}
              {!jobs.length&&<Empty icon="spark">No analysis jobs yet.</Empty>}
            </Card>}
            {settingsTab==='account'&&<Card title="ChatGPT account"><Account {...accountProps}/></Card>}
            {settingsTab==='analysis'&&<>
              <Card title="Local analysis runtime" actions={<><Button size="sm" onClick={()=>run(async()=>{const r=await api.call('doctor');setDoctor(r);return r;})}>Check environment</Button><Button size="sm" onClick={()=>run(()=>api.call('worker-setup',{}),'Worker setup queued')}>Set up WSL worker</Button></>}>
                <p className="muted">Deep analysis runs in an isolated WSL2 worker; models download on first use.</p>
                {doctor&&<div style={{marginTop:14}}><KeyValues rows={Object.entries(doctor).map(([k,v])=>[k.replaceAll('_',' '),typeof v==='object'?<span className="mono">{JSON.stringify(v)}</span>:String(v)])}/></div>}
              </Card>
              <ExperimentalModels run={run}/>
              <ResourceSettings run={run}/>
            </>}
            {settingsTab==='energy'&&<Card title="Energy calibration" actions={<Button size="sm" onClick={()=>run(()=>api.call('calibrate-energy',{}),'Library energy reference updated')}>Recalibrate library</Button>}>
              <p className="muted" style={{marginBottom:16}}>Compare tracks by ear, then recalibrate. Ratings become relative to your collection; your own ratings stay separate.</p>
              <EnergyCompare tracks={tracks} run={run}/>
            </Card>}
            {settingsTab==='data'&&<>
              <WorkspaceRecovery run={run}/>
              <PortableImport run={run}/>
              <CacheRelocation run={run}/>
              <Card title="rekordbox write history" actions={<Button size="sm" icon="refresh" onClick={()=>run(async()=>{const r=await api.call('manifests');setManifests(r);return r;})}>Load backups</Button>}>
                {manifests.map(m=><div className="list-row" key={m.id}><span className="grow">{m.created} · {m.written.length} tracks written</span><Button size="sm" onClick={()=>run(()=>api.call('restore',{id:m.id}),'Database restored; previous state backed up')}>Restore</Button></div>)}
                {!manifests.length&&<p className="muted">Load backups to restore rekordbox to a state before an Autocue write.</p>}
              </Card>
              <Card title="Export">
                <div className="row">
                  <Button icon="download" onClick={()=>run(()=>api.export({kind:'profiles',name:'autocue-profiles.json'}))}>Track profiles</Button>
                  <Button icon="download" onClick={()=>run(()=>api.export({kind:'workspace',name:'autocue-workspace.json'}))}>Workspace (JSON)</Button>
                  <Button icon="download" onClick={()=>run(()=>api.export({kind:'benchmark',name:'autocue-benchmark.json'}))}>Benchmark template</Button>
                </div>
              </Card>
            </>}
          </div>
        </div>
      </>}
    </main>

    <div className="toast-stack" aria-live="polite">
      {error&&<div className="toast error" role="alert"><Icon name="alert"/><span>{error}</span><button aria-label="Dismiss error" onClick={()=>setError('')}><Icon name="x" size={14}/></button></div>}
      {message&&<div className="toast"><Icon name="check"/><span>{message}</span><button aria-label="Dismiss message" onClick={()=>setMessage('')}><Icon name="x" size={14}/></button></div>}
    </div>
  </div>;
}

function Metadata({track,onSave}:{track:Track;onSave:(changes:any)=>void}){
  const [title,setTitle]=useState(track.title),[artist,setArtist]=useState(track.artist),[genre,setGenre]=useState(track.genres.join(', ')),[notes,setNotes]=useState(track.notes),[key,setKey]=useState(typeof track.key==='string'?track.key:track.key?.name||''),[rating,setRating]=useState(track.energy?.toString()||''),[themes,setThemes]=useState((track.themes||[]).join(', ')),[hooks,setHooks]=useState((track.hooks||[]).filter(h=>h.verified).map(h=>h.time).join(', ')),[acceptGrid,setAcceptGrid]=useState(!!track.overrides?.accept_rekordbox_grid_conflict),[moodTags,setMoodTags]=useState((track.mood_tags||[]).join(', ')),[familiarity,setFamiliarity]=useState(track.familiarity?.toString()||'');
  const list=(s:string)=>s.split(',').map(x=>x.trim()).filter(Boolean);
  return <div className="form-grid">
    <Field label="Title"><input value={title} onChange={e=>setTitle(e.target.value)}/></Field>
    <Field label="Artist"><input value={artist} onChange={e=>setArtist(e.target.value)}/></Field>
    <Field label="Key" hint="Standard (A minor) or Camelot (8A)"><input value={key} onChange={e=>setKey(e.target.value)}/></Field>
    <Field label="Your energy rating (1–10)"><input type="number" min="1" max="10" value={rating} onChange={e=>setRating(e.target.value)}/></Field>
    <Field label="Genres"><input placeholder="Comma separated" value={genre} onChange={e=>setGenre(e.target.value)}/></Field>
    <Field label="Mood"><input placeholder="Comma separated" value={moodTags} onChange={e=>setMoodTags(e.target.value)}/></Field>
    <Field label="Familiarity (0–1)" hint="How well your crowd knows it"><input type="number" min="0" max="1" step=".1" value={familiarity} onChange={e=>setFamiliarity(e.target.value)}/></Field>
    <Field label="Hook entrances (s)" hint="Verified by listening; comma separated"><input value={hooks} onChange={e=>setHooks(e.target.value)}/></Field>
    <Field label="Themes" full hint="Verified lyrics or themes, used for wordplay transitions"><input value={themes} onChange={e=>setThemes(e.target.value)}/></Field>
    <Field label="Notes" full><textarea value={notes} onChange={e=>setNotes(e.target.value)}/></Field>
    {track.overrides?.grid&&<label className="check full"><input type="checkbox" checked={acceptGrid} onChange={e=>setAcceptGrid(e.target.checked)}/>I checked the rekordbox grid and accept cue export against it</label>}
    <div className="row full end"><Button kind="primary" onClick={()=>onSave({title,artist,mood_tags:list(moodTags),...(familiarity?{familiarity:+familiarity}:{}),genres:list(genre),themes:list(themes),hooks:hooks.split(',').filter(s=>s.trim()).map(s=>({time:+s,verified:true})),notes,accept_rekordbox_grid_conflict:acceptGrid,...(key?{key}:{}),...(rating?{energy:+rating}:{})})}>Save profile</Button></div>
  </div>;
}
function Account({account,connect,models,model,setModel,loadModels}:any){
  return <div className="stack">
    {account.accounts?.map((a:any)=><div className="account-row" key={a.id}>
      <input type="radio" aria-label={`Use ${a.email||'this account'}`} checked={account.active===a.id} onChange={()=>connect('select',{id:a.id})}/>
      <span className="avatar">{(a.email||'?')[0].toUpperCase()}</span>
      <div className="grow"><strong>{a.email||'Connected account'}</strong><small>{a.plan_enabled?'Plan usage enabled':'Sign-in only'}</small></div>
      <Button size="sm" kind="ghost" onClick={()=>connect('disconnect',{id:a.id})}>Disconnect</Button>
    </div>)}
    {account.active&&<Field label="Model"><div className="row" style={{flexWrap:'nowrap'}}><select className="grow" value={model} onChange={e=>setModel(e.target.value)}><option value="">Select a model</option>{models.map((m:any)=><option key={m.id} value={m.id}>{m.name}</option>)}{model&&!models.some((m:any)=>m.id===model)&&<option value={model}>{model}</option>}</select><Button icon="refresh" onClick={loadModels}>Load models</Button></div></Field>}
    <div className="row">
      <Button kind={account.accounts?.length?'':'primary'} onClick={()=>connect('sign-in')}>{account.accounts?.length?'Add account':'Continue with ChatGPT'}</Button>
      {account.pending&&<Button kind="ghost" onClick={()=>connect('cancel')}>Cancel sign-in</Button>}
      <Button kind="ghost" icon="external" onClick={()=>connect('manage')}>Manage usage</Button>
    </div>
  </div>;
}
function WriteChanges({value,tracks}:{value:any;tracks:Track[]}){
  return <div>{value.changes.map((c:any)=><div key={c.track_id}>
    <h4>{tracks.find(t=>t.id===c.track_id)?.title}</h4>
    {c.operations.map((o:any)=>{const kind=o.target?(o.delete.length?'replace':'add'):'delete';return <div className="op" key={o.role}>
      <span className={`tag ${kind}`}>{kind}</span><b style={{background:roleColor[o.role]||'#cccccc'}}>{o.role}</b>
      <span className="mono">{o.target?clock(o.target.InMsec/1000):''}{o.target?.OutMsec>=0?` → ${clock(o.target.OutMsec/1000)}`:''}</span></div>;})}
    {c.comment_after!==c.comment_before&&<div className="op"><span className="tag replace">comment</span><span>Update the Autocue comment block</span></div>}
  </div>)}
  {value.conflicts.map((c:any,i:number)=><p className="warning" key={i}><Icon name="alert" size={14}/>{c.role||'Track'}: {c.reason}</p>)}
  {!value.changes.length&&<p className="muted">No approved changes to write.</p>}</div>;
}

class ReviewBoundary extends React.Component<{children:React.ReactNode},{error:string}>{
  state={error:''};
  static getDerivedStateFromError(error:Error){return {error:error.message||'Unexpected interface error'};}
  render(){return this.state.error?<main className="standalone"><h1>Autocue needs to reload</h1><p>Your library and decisions are saved in the local workspace.</p><Button kind="primary" onClick={()=>location.reload()}>Reload</Button><Disclosure summary="Details"><pre>{this.state.error}</pre></Disclosure></main>:this.props.children;}
}
createRoot(document.getElementById('root')!).render(<ReviewBoundary><App/></ReviewBoundary>);
