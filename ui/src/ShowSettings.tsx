import React,{useEffect,useState} from 'react';
import {Button,Card,Field,Pill,Segmented,Tabs} from './ui';
import {type Catalogue,EffectsCard,LyricsCard,OnScreenCard,OutputCard,ScenesCard} from './ShowDetails';

/** The visuals' settings file as the desktop app reads it (crates/onset-settings). */
export type ShowConfig={
  output_monitor:{kind:'primary'}|{kind:'index';value:number}|{kind:'name_contains';value:string};
  fullscreen:boolean; preset:'chill'|'club'|'festival'|'custom';
  quality:'low'|'medium'|'high'|'ultra'; auto:boolean;
  auto_change:'off'|'tracks'|'drops'|'phrases'; scene:string;
  show_card:boolean; show_hud:boolean; adaptive_resolution:boolean;
  [key:string]:unknown;
};
type Monitor={name:string|null;width:number;height:number;primary:boolean};
type Loaded={settings:ShowConfig;effective_fx:{[key:string]:any};read_only:boolean;path:string};
type Onset={
  showSettings:()=>Promise<Loaded>;
  saveShowSettings:(s:ShowConfig)=>Promise<Loaded>;
  showCatalogue:()=>Promise<Catalogue>;
  showLogo:(clear:boolean)=>Promise<Loaded>;
  showMonitors:()=>Promise<Monitor[]>;
  startVisuals:()=>Promise<'started'|'running'>;
  endVisuals:()=>Promise<void>;
  visualsRunning:()=>Promise<boolean>;
  previewShow:(name:string)=>Promise<void>;
  onShow:(callback:(s:{running:boolean})=>void)=>()=>void;
};
const onset=():Onset|undefined=>(window as any).onset?.showSettings?(window as any).onset:undefined;

export const SCENES=['tunnel','synthwave','ribbons','kaleido','lasers','nebula','zerog','crystal','liquid','cover','pulse','ring','warp','voronoi'];
export const PRESETS:{id:ShowConfig['preset'];label:string;blurb:string}[]=[
  {id:'chill',label:'Chill',blurb:'Smooth and calm: slow colour turns, soft build-ups, no shake or inversions.'},
  {id:'club',label:'Club',blurb:'Punchy: light sweeps before drops, a soft flash when they land, a slide on every new phrase.'},
  {id:'festival',label:'Festival',blurb:'Everything big: full build-ups, bold drops with sway, inversions and glitches.'},
  {id:'custom',label:'Custom',blurb:'Your own mix of the effect settings.'},
];
const QUALITY:{id:ShowConfig['quality'];label:string;blurb:string}[]=[
  {id:'low',label:'Low',blurb:'Half resolution, simple scenes. For integrated graphics.'},
  {id:'medium',label:'Medium',blurb:'Three-quarter resolution; heavy scenes stay out of Auto.'},
  {id:'high',label:'High',blurb:'Full resolution, every scene. Good for a gaming laptop.'},
  {id:'ultra',label:'Ultra',blurb:'Full resolution with extra ray-march detail.'},
];
const CHANGES:{id:ShowConfig['auto_change'];label:string}[]=[
  {id:'tracks',label:'Each track'},{id:'drops',label:'Tracks and drops'},{id:'phrases',label:'Every few phrases'},
];

/** The output setting for a screen picked in the list: by name, as the visuals match it. */
export function monitorChoice(m:Monitor):ShowConfig['output_monitor']{
  return m.primary||!m.name?{kind:'primary'}:{kind:'name_contains',value:m.name};
}
/** Which listed screen a saved choice means. */
export function chosenMonitor(choice:ShowConfig['output_monitor'],monitors:Monitor[]):number{
  if(choice.kind==='primary')return monitors.findIndex(m=>m.primary);
  if(choice.kind==='index')return choice.value<monitors.length?choice.value:-1;
  const want=choice.value.toLowerCase();
  return monitors.findIndex(m=>(m.name||'').toLowerCase().includes(want));
}
export function sceneTitle(name:string){return name==='zerog'?'Zero-G':name[0].toUpperCase()+name.slice(1);}

/** Show setup in the desktop app: what the visuals' own launcher page used to hold. */
export function ShowSettings(){
  const api=onset();
  const [state,setState]=useState<Loaded|null>(null),[tab,setTab]=useState('show'),[catalogue,setCatalogue]=useState<Catalogue|null>(null);
  const [monitors,setMonitors]=useState<Monitor[]>([]),[message,setMessage]=useState(''),[error,setError]=useState(''),[showOn,setShowOn]=useState(false);
  useEffect(()=>{if(!api)return;api.visualsRunning().then(setShowOn).catch(()=>{});return api.onShow(s=>setShowOn(s.running));},[]);
  useEffect(()=>{if(!api)return;api.showSettings().then(setState).catch(e=>setError(String(e.message||e)));api.showMonitors().then(setMonitors).catch(()=>setMonitors([]));api.showCatalogue().then(setCatalogue).catch(()=>setCatalogue(null));},[]);
  if(!api)return <Card title="Show"><p className="muted">Show settings need the OnSet desktop app.</p></Card>;
  if(!state)return <Card title="Show"><p className="muted">{error||'Loading the show settings…'}</p></Card>;
  const s=state.settings;
  const change=(patch:Partial<ShowConfig>)=>{
    const next={...s,...patch};
    setState({...state,settings:next});setError('');
    api.saveShowSettings(next).then(saved=>{setState(saved);setMessage(showOn?(patch.output_monitor?'Saved. The show moves to that screen next time it starts.':'Saved; the show uses it now.'):'Saved.');})
      .catch(e=>setError(String(e.message||e)));
  };
  const picked=chosenMonitor(s.output_monitor,monitors);
  const quality=QUALITY.find(q=>q.id===s.quality);
  const preset=PRESETS.find(p=>p.id===s.preset);
  return <>
    {state.read_only&&<p className="bad">{state.path} could not be read, so changes are not saved over it. Fix or remove that file first.</p>}
    <Tabs value={tab} onChange={setTab} options={[{id:'show',label:'Show'},{id:'effects',label:'Effects'},{id:'scenes',label:'Scenes'},{id:'screen',label:'On screen'},{id:'lyrics',label:'Lyrics'},{id:'output',label:'Output and audio'}]}/>
    {tab==='effects'&&<EffectsCard fx={state.effective_fx} change={change}/>}
    {tab==='scenes'&&<ScenesCard s={s} change={change} catalogue={catalogue}/>}
    {tab==='screen'&&<OnScreenCard s={s} change={change} setLogo={clear=>api.showLogo(clear).then(setState).catch(e=>setError(String(e.message||e)))}/>}
    {tab==='lyrics'&&<LyricsCard s={s} change={change}/>}
    {tab==='output'&&<OutputCard s={s} change={change} catalogue={catalogue} rescan={()=>api.showCatalogue().then(setCatalogue).catch(()=>{})}/>}
    {tab==='show'&&<>
    <Card title="Where the show goes">
      <div className="stack">
        <Field label="Screen" hint="Pick the projector or TV. The laptop screen stays free for rekordbox and OnSet.">
          <select value={picked} onChange={e=>{const m=monitors[+e.target.value];if(m)change({output_monitor:monitorChoice(m)});}}>
            {picked<0&&<option value={-1}>Saved screen not connected</option>}
            {monitors.map((m,i)=><option key={i} value={i}>{(m.name||`Screen ${i+1}`).replace('\\\\.\\','')} · {m.width}×{m.height}{m.primary?' · main screen':''}</option>)}
          </select>
        </Field>
        <label className="check"><input type="checkbox" checked={s.fullscreen} onChange={e=>change({fullscreen:e.target.checked})}/>Fullscreen on that screen</label>
        <p className="hint">On the screen this window is on, the show opens as a window instead, so it never covers OnSet or rekordbox.</p>
      </div>
    </Card>
    <Card title="Style">
      <Segmented label="Style" value={s.preset} options={PRESETS.map(p=>({id:p.id,label:p.label}))} onChange={id=>change({preset:id as ShowConfig['preset']})}/>
      <p className="hint">{preset?.blurb}</p>
    </Card>
    <Card title="Scenes">
      <div className="stack">
        <label className="check"><input type="checkbox" checked={s.auto} onChange={e=>change({auto:e.target.checked})}/>Auto: OnSet picks scenes that suit each track and changes them with the music</label>
        {s.auto?<Field label="Change scenes">
          <Segmented label="When scenes change" value={s.auto_change==='off'?'tracks':s.auto_change} options={CHANGES} onChange={id=>change({auto_change:id as ShowConfig['auto_change']})}/>
        </Field>:<Field label="Scene">
          <select value={s.scene} onChange={e=>change({scene:e.target.value})}>{(catalogue?.scenes.map(c=>[c.name,c.title])||SCENES.map(n=>[n,sceneTitle(n)])).map(([n,t])=><option key={n} value={n}>{t}</option>)}</select>
        </Field>}
      </div>
    </Card>
    <Card title="On screen">
      <div className="stack">
        <label className="check"><input type="checkbox" checked={s.show_card} onChange={e=>change({show_card:e.target.checked})}/>Now Playing card: cover, title and artist</label>
        <label className="check"><input type="checkbox" checked={s.show_hud} onChange={e=>change({show_hud:e.target.checked})}/>Stats (HUD): frame time, graphics card, rekordbox link. For you, not the crowd</label>
      </div>
    </Card>
    <Card title="Performance">
      <div className="stack">
        <Segmented label="Quality" value={s.quality} options={QUALITY.map(q=>({id:q.id,label:q.label}))} onChange={id=>change({quality:id as ShowConfig['quality']})}/>
        <p className="hint">{quality?.blurb}</p>
        <label className="check"><input type="checkbox" checked={s.adaptive_resolution} onChange={e=>change({adaptive_resolution:e.target.checked})}/>Render smaller for a while when frames fall behind</label>
      </div>
    </Card>
    </>}
    <div className="row show-actions">
      {showOn
        ?<Button onClick={()=>api.endVisuals().then(()=>setMessage('Show ended.')).catch(e=>setError(String(e.message||e)))}>End show</Button>
        :<Button kind="primary" onClick={()=>api.startVisuals().then(r=>setMessage(r==='running'?'The show is already on.':'Show started. Esc on the projector also ends it.')).catch(e=>setError(String(e.message||e)))}>Start show</Button>}
      {message&&<Pill state="good">{message}</Pill>}
    </div>
    {showOn&&<div className="row"><span className="muted">Try it without music:</span>
      {([['buildup','Build-up and drop'],['drop','Drop'],['phrase','New phrase'],['breakdown','Breakdown'],['track','Track change']] as const).map(([id,label])=>
        <Button key={id} size="sm" kind="ghost" onClick={()=>api.previewShow(id).catch(e=>setError(String(e.message||e)))}>{label}</Button>)}
    </div>}
    <p className="hint">On the projector itself (click the show first), even if this window is busy: <kbd>Esc</kbd> ends the show · <kbd>B</kbd> blackout · <kbd>F</kbd> fullscreen · <kbd>H</kbd> stats · <kbd>C</kbd> Now Playing card · <kbd>←</kbd><kbd>→</kbd> scene. Untick Fullscreen to preview the show in a window on this screen.</p>
    <div className="row">
      {error&&<span className="bad">{error}</span>}
    </div>
  </>;
}
