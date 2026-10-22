import React,{useState} from 'react';
import {Button,Card,Field,Segmented} from './ui';

/** Everything the visuals' launcher tabs held beyond the main Show page. Each card edits
 * the shared settings through `change`, which saves and updates a running show. */
type Settings={[key:string]:any};
type Change=(patch:Settings)=>void;
export type Catalogue={scenes:{name:string;title:string;blurb:string;cost:number;auto:boolean}[];gpus:string[];audio_inputs:string[]};

function Slider({label,hint,value,min,max,step=.05,onChange,format=(v:number)=>v.toFixed(2)}:{label:string;hint?:string;value:number;min:number;max:number;step?:number;onChange:(v:number)=>void;format?:(v:number)=>string}){
  const [draft,setDraft]=useState<number|null>(null);
  const shown=draft??value;
  return <Field label={`${label} · ${format(shown)}`} hint={hint}>
    <input type="range" min={min} max={max} step={step} value={shown}
      onChange={e=>setDraft(+e.target.value)}
      onPointerUp={()=>{if(draft!=null){onChange(draft);setDraft(null);}}}
      onKeyUp={()=>{if(draft!=null){onChange(draft);setDraft(null);}}}/>
  </Field>;
}
function Check({label,checked,onChange}:{label:string;checked:boolean;onChange:(v:boolean)=>void}){
  return <label className="check"><input type="checkbox" checked={checked} onChange={e=>onChange(e.target.checked)}/>{label}</label>;
}
const pct=(v:number)=>`${Math.round(v*100)}%`;

/** The effect sliders, in the launcher's groups and words. Moving one makes the style Custom,
 * starting from the current style's values. */
export const EFFECTS:{group:string;items:{key:string;label:string;hint:string;max:number}[]}[]=[
  {group:'Movement',items:[
    {key:'reactivity',label:'Audio reactivity',hint:'How far the scenes move with the bass, mids, treble and drums.',max:2},
    {key:'shake',label:'Camera shake',hint:'The picture jolts on kicks during drops.',max:1},
    {key:'emphasis',label:'Overlay pulse',hint:'The card and HUD grow for a moment on a new track or a drop.',max:1}]},
  {group:'Build-ups and phrase changes',items:[
    {key:'buildup',label:'Build-ups',hint:'Light bars sweep across the screen before a drop, faster as it nears, with a slow push in and a colour cycle.',max:1},
    {key:'moves',label:'Phrase slides',hint:'On every new phrase the picture slides and a band of light crosses the screen.',max:1}]},
  {group:'Light',items:[
    {key:'flashes',label:'Flashes',hint:'White flash at drops, tinted flashes when the playhead passes a cue.',max:1},
    {key:'bloom',label:'Glow',hint:'Bloom around bright light.',max:1.5},
    {key:'trails',label:'Trails',hint:'Scenes that use feedback leave light trails behind.',max:1}]},
  {group:'Colour and surprises',items:[
    {key:'colour',label:'Colour changes',hint:'Hue swings at drops and new phrases; the palette always starts from the cover.',max:1},
    {key:'inversions',label:'Inversions',hint:'Light and dark swap for a beat at some drops.',max:1},
    {key:'glitch',label:'Glitches',hint:'Slices of the picture tear on track changes and cues.',max:1},
    {key:'chaos',label:'Randomness',hint:'How often OnSet surprises: unexpected colour turns, glitches, bolder scene picks.',max:1},
    {key:'grain',label:'Film grain',hint:'A little texture over the whole picture.',max:1}]},
];
export function withEffect(fx:Settings,key:string,value:number|boolean):Settings{return {preset:'custom',fx:{...fx,[key]:value}};}

export function EffectsCard({fx,change}:{fx:Settings;change:Change}){
  return <Card title="Effects">
    <p className="hint">Moving a slider makes the style Custom, starting from the current style.</p>
    {EFFECTS.map(g=><div className="stack" key={g.group}>
      <h4 className="show-sub">{g.group}</h4>
      {g.items.map(i=><Slider key={i.key} label={i.label} hint={i.hint} value={fx[i.key]??0} min={0} max={i.max} format={pct} onChange={v=>change(withEffect(fx,i.key,v))}/>)}
    </div>)}
    <h4 className="show-sub">Strobe</h4>
    <Check label="Strobe after drops. Off by default: strobing can trigger seizures in people with photosensitive epilepsy" checked={!!fx.strobe} onChange={v=>change(withEffect(fx,'strobe',v))}/>
  </Card>;
}

/** Scenes in the Auto rotation; `null` means each scene's own default. */
export function inRotation(rotation:string[]|null,scene:{name:string;auto:boolean}){return rotation?rotation.includes(scene.name):scene.auto;}
export function toggledRotation(rotation:string[]|null,scenes:{name:string;auto:boolean}[],name:string,on:boolean){
  const current=scenes.filter(s=>inRotation(rotation,s)).map(s=>s.name);
  return on?[...current.filter(n=>n!==name),name]:current.filter(n=>n!==name);
}

export function ScenesCard({s,change,catalogue}:{s:Settings;change:Change;catalogue:Catalogue|null}){
  const scenes=catalogue?.scenes||[];
  const [picked,setPicked]=useState(scenes[0]?.name||'tunnel');
  const tweak={speed:1,intensity:1,hue:0,...(s.tweaks?.[picked]||{})};
  const setTweak=(key:string,v:number)=>change({tweaks:{...(s.tweaks||{}),[picked]:{...tweak,[key]:v}}});
  return <Card title="Scenes" actions={s.rotation&&<Button size="sm" kind="ghost" onClick={()=>change({rotation:null})}>Use the defaults</Button>}>
    <p className="hint">Scenes Auto may pick. Heavy scenes join Auto only at High quality and above.</p>
    <div className="stack">{scenes.map(sc=><Check key={sc.name} label={`${sc.title}: ${sc.blurb}`} checked={inRotation(s.rotation,sc)} onChange={on=>change({rotation:toggledRotation(s.rotation,scenes,sc.name,on)})}/>)}</div>
    <h4 className="show-sub">Fine-tune a scene</h4>
    <Field label="Scene"><select value={picked} onChange={e=>setPicked(e.target.value)}>{scenes.map(sc=><option key={sc.name} value={sc.name}>{sc.title}</option>)}</select></Field>
    <Slider label="Speed" value={tweak.speed} min={.25} max={2} format={v=>`${v.toFixed(2)}×`} onChange={v=>setTweak('speed',v)}/>
    <Slider label="Audio response" value={tweak.intensity} min={0} max={2} format={v=>`${v.toFixed(2)}×`} onChange={v=>setTweak('intensity',v)}/>
    <Slider label="Colour shift" value={tweak.hue} min={0} max={1} format={pct} onChange={v=>setTweak('hue',v)}/>
    {s.tweaks?.[picked]&&<Button size="sm" kind="ghost" onClick={()=>{const next={...s.tweaks};delete next[picked];change({tweaks:next});}}>Reset this scene</Button>}
  </Card>;
}

const CORNERS=[{id:'top_left',label:'Top left'},{id:'top_right',label:'Top right'},{id:'bottom_left',label:'Bottom left'},{id:'bottom_right',label:'Bottom right'}];
const CARD_FIELDS:[string,string][]=[['artwork','Cover'],['album','Album'],['year','Year'],['key','Key'],['bpm','BPM'],['genre','Genre'],['label','Label'],['rating','Rating'],['tags','My tags'],['comment','Comment'],['play_count','Play count']];
const HUD_FIELDS:[string,string][]=[['scene','Scene'],['performance','Frame time and graphics card'],['transport','rekordbox link'],['tempo','Tempo'],['key','Key'],['phrase','Phrase'],['drop','Drop countdown'],['next_cue','Next cue'],['analysis','Analysis source']];

export function OnScreenCard({s,change,setLogo}:{s:Settings;change:Change;setLogo:(clear:boolean)=>void}){
  const card=s.card||{},hud=s.hud||{};
  return <Card title="On screen">
    <h4 className="show-sub">Now Playing card</h4>
    <Field label="Corner"><Segmented label="Card corner" value={card.corner} options={CORNERS} onChange={id=>change({card:{...card,corner:id}})}/></Field>
    <Slider label="Size" value={card.size??1} min={.6} max={1.6} format={v=>`${v.toFixed(2)}×`} onChange={v=>change({card:{...card,size:v}})}/>
    <div className="row">{CARD_FIELDS.map(([k,l])=><Check key={k} label={l} checked={!!card[k]} onChange={v=>change({card:{...card,[k]:v}})}/>)}</div>
    <h4 className="show-sub">Stats (HUD)</h4>
    <Field label="Corner"><Segmented label="HUD corner" value={hud.corner} options={CORNERS} onChange={id=>change({hud:{...hud,corner:id}})}/></Field>
    <Slider label="Size" value={hud.size??1} min={.6} max={2} format={v=>`${v.toFixed(2)}×`} onChange={v=>change({hud:{...hud,size:v}})}/>
    <div className="row">{HUD_FIELDS.map(([k,l])=><Check key={k} label={l} checked={!!hud[k]} onChange={v=>change({hud:{...hud,[k]:v}})}/>)}</div>
    <h4 className="show-sub">Blackout</h4>
    <p className="hint">B on the projector blacks the screen out; a logo can sit in the middle.</p>
    <div className="row">
      <Button size="sm" onClick={()=>setLogo(false)}>{s.blackout_logo?'Change logo…':'Choose a logo…'}</Button>
      {s.blackout_logo&&<><span className="mono dim">{String(s.blackout_logo).split(/[\\/]/).pop()}</span><Button size="sm" kind="ghost" onClick={()=>setLogo(true)}>Remove</Button></>}
    </div>
    {s.blackout_logo&&<Slider label="Logo size" value={s.blackout_logo_size??.5} min={.1} max={1} format={pct} onChange={v=>change({blackout_logo_size:v})}/>}
  </Card>;
}

const STYLES=[{id:'clean',label:'Clean',blurb:'White text, the line being sung only. Reads anywhere.'},{id:'glow',label:'Glow',blurb:"The sung line glows in the track's colours; the next line waits below."},{id:'karaoke',label:'Karaoke',blurb:'Each word lights up as it is sung.'}];
const PLACES=[{id:'top',label:'Top'},{id:'centre',label:'Centre'},{id:'lower',label:'Lower third'}];

export function LyricsCard({s,change}:{s:Settings;change:Change}){
  const l=s.lyrics||{};
  const set=(patch:Settings)=>change({lyrics:{...l,...patch}});
  return <Card title="Lyrics">
    <p className="hint">Synced lyrics, line by line, timed to the playhead of the MASTER deck.</p>
    <Check label="Show lyrics" checked={!!l.enabled} onChange={v=>set({enabled:v})}/>
    <Check label="Look lyrics up online when a track has none saved" checked={!!l.online} onChange={v=>set({online:v})}/>
    <Field label="Style"><Segmented label="Lyric style" value={l.style} options={STYLES} onChange={id=>set({style:id})}/></Field>
    <p className="hint">{STYLES.find(x=>x.id===l.style)?.blurb}</p>
    <Field label="Place"><Segmented label="Lyric place" value={l.place} options={PLACES} onChange={id=>set({place:id})}/></Field>
    <Slider label="Size" value={l.size??1} min={.6} max={2} format={v=>`${v.toFixed(2)}×`} onChange={v=>set({size:v})}/>
    <Slider label="Effects at drops" value={l.drop_fx??.6} min={0} max={1} format={pct} onChange={v=>set({drop_fx:v})}/>
    <Slider label="Timing" hint="Move the lyrics earlier (−) or later (+) when they run off the music." value={l.offset_s??0} min={-2} max={2} step={.05} format={v=>`${v>0?'+':''}${v.toFixed(2)} s`} onChange={v=>set({offset_s:v})}/>
  </Card>;
}

export function OutputCard({s,change,catalogue}:{s:Settings;change:Change;catalogue:Catalogue|null}){
  return <Card title="Output and audio">
    <Field label="Graphics card" hint="The fastest is picked unless you choose one. Applies from the next show.">
      <select value={s.gpu??''} onChange={e=>change({gpu:e.target.value||null})}>
        <option value="">Fastest available</option>
        {(catalogue?.gpus||[]).map(g=><option key={g} value={g}>{g}</option>)}
      </select>
    </Field>
    <Field label="Frame timing">
      <Segmented label="Present mode" value={s.present_mode} options={[{id:'fifo',label:'VSync'},{id:'mailbox',label:'Low latency'}]} onChange={id=>change({present_mode:id})}/>
    </Field>
    <p className="hint">VSync never tears. Low latency shows the newest frame where the graphics card supports it.</p>
    <Slider label="Render scale" hint="Scenes render at this share of the screen and are scaled up; lower is lighter." value={s.internal_scale??1} min={.5} max={1} format={pct} onChange={v=>change({internal_scale:v})}/>
    <Field label="Audio input" hint="OnSet listens to the mix to follow the drums and the spectrum. Automatic prefers the DJ controller's recording input, then what the computer plays.">
      <select value={s.audio_device??''} onChange={e=>change({audio_device:e.target.value||null})}>
        <option value="">Automatic</option>
        {(catalogue?.audio_inputs||[]).map(a=><option key={a} value={a}>{a}</option>)}
      </select>
    </Field>
  </Card>;
}
