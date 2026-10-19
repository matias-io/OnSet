/// <reference types="vite/client" />
// Development-only sample bridge so the renderer can be previewed in a plain browser (`npm run dev`).
// Vite removes this block from production builds; Electron always provides the real preload bridge.
if(import.meta.env.DEV&&typeof window!=='undefined'&&!(window as any).autocue){
 const titles=[['Losing It','FISHER'],['Stop It','Fisher'],['La Danza','Ramon Tapia'],['Cola','CamelPhat & Elderbrook'],['Turn Off The Lights','Chris Lake'],['Drop It','Biscits'],['Pump The Brakes','Dom Dolla'],['Rumble','Skrillex & Fred again..'],['Body Language','Fisher'],['Tell Me Why','Supermode'],['Hypnotized','Purple Disco Machine'],['Afraid To Feel','LF SYSTEM']];
 const keys=['A minor','F minor','8A','C major','G minor','D minor','11B','E minor','Bb minor','F# minor','A major','Eb major'];
 const roles=[['A','Intro loop',.02,8],['B','First groove',.12,null],['C','Build',.3,null],['D','First full groove',.36,null],['E','Breakdown',.62,null],['F','Outro mix',.84,16]];
 const rand=(seed:number)=>()=>{seed=(seed*16807)%2147483647;return seed/2147483647;};
 const tracks=titles.map(([title,artist],i)=>{
  const r=rand(i+7),bpm=[126,125,128,124,124,127,125,140,126,125,120,124][i],duration=300+Math.round(r()*120),beat=60/bpm;
  const beats=Array.from({length:Math.floor(duration/beat)},(_,n)=>+(n*beat).toFixed(3)),downbeats=beats.filter((_,n)=>n%4===0);
  const labels=['intro','groove','build','drop','breakdown','drop','outro'],bounds=[0,.1,.28,.36,.6,.7,.88,1];
  const sections=labels.map((label,n)=>({label,start:+(bounds[n]*duration).toFixed(2),end:+(bounds[n+1]*duration).toFixed(2)}));
  const level=(x:number)=>{const s=sections.find(s=>x*duration>=s.start&&x*duration<s.end)?.label;return s==='drop'?.9:s==='breakdown'?.35:s==='intro'||s==='outro'?.5:s==='build'?.65:.75;};
  const waveform=Array.from({length:600},(_,n)=>Math.min(1,level(n/600)*(.7+r()*.3)));
  const curve=(shift:number)=>Array.from({length:48},(_,n)=>Math.max(0,Math.min(1,level(n/48)+shift+(r()-.5)*.15)));
  const analysed=i<9;
  const cues=analysed?roles.map(([role,name,at,bars],n)=>({id:`${i}-${role}`,role,name,time:+(at as number*duration).toFixed(3),end:bars?+((at as number)*duration+(bars as number)*4*beat).toFixed(3):null,review:n<2&&i<3?'approved':n===3&&i===1?'rejected':'proposed',confidence:n%3?'unverified':'moderate',source:'resolved structure'})):[];
  const spectrogram={frequencies:Array.from({length:64},(_,n)=>Math.round(40*Math.pow(16000/40,n/63))),values:Array.from({length:64},(_,y)=>Array.from({length:200},(_,x)=>-70+level(x/200)*55*(1-y/90)+r()*12))};
  return {id:`track-${i}`,title,artist,version:'Original Mix',path:`C:\\Music\\${artist} - ${title}.flac`,locations:[],duration,bpm,key:keys[i],energy:analysed?[7,8,6,6,7,8,7,9,7,6,5,6][i]:undefined,genres:i%3?['Tech House']:['House','Melodic'],notes:'',revision:`r${i}`,cues,
   analysis:analysed?{waveform,beats,downbeats,structure:{sections,windows:sections.map((s,n)=>({...s,id:`w${n}`,vocals:n===2}))},energy_profile:{rating:[7,8,6,6,7,8,7,9,7][i],drive:curve(0),density:curve(-.1),intensity:curve(.05),tension:curve(-.2),contrast:curve(-.3)},spectrogram,key:{name:keys[i],strength:.62},grid_observation:{tempo_cv:.004,source:'rekordbox + Beat This!'},events:{first_full_groove:{state:'present',bar:24}},local_keys:[]}:null,
   analysis_state:analysed?(i===4?'partial':'complete'):i===10?'pending':'imported',review_state:i<2?'reviewed':analysed?'reviewing':'unreviewed',write_state:'clean',overrides:{},mood_tags:['driving'],familiarity:.6};
 });
 const catalogue=['Long blend','Quick blend','EQ / bass swap','Filter blend','Cut','Drop swap','Double drop','Breakdown blend','Echo out','Reverb out','Loop blend','Loop roll','Acapella / stem blend','Tease','Backspin / brake','Tempo bridge','Fade / end-to-start','Wordplay'].map((name,i)=>({id:name.toLowerCase().replace(/[^a-z]+/g,'_'),name,skill:1+(i%3),bars:[32,8,16,16,0,0,0,16,4,4,16,8,16,4,0,16,0,8][i],equipment:[['eq'],[],['eq'],['filter'],[],[],['eq'],[],['echo'],['reverb'],['loop'],['loop'],['stems'],['hot_cues'],[],['tempo'],[],[]][i]}));
 const option=(a:any,b:any,n:number)=>({id:`opt-${a.id}-${b.id}-${n}`,track_ids:[a.id,b.id],technique:catalogue[n*2].id,name:catalogue[[2,3,0][n]].name,out_start:a.duration*.84,out_end:a.duration*.84+32,in_start:0,in_end:32,out_rate:1,in_rate:a.bpm/b.bpm,pitch_pct:(a.bpm/b.bpm-1)*100,wall_seconds:[30.5,15.2,61][n],key_lock:true,score:3-n*.4,
  actions:[{at:0,action:`Start incoming at 0:00; outgoing at ${Math.floor(a.duration*.84/60)}:${Math.floor(a.duration*.84%60).toString().padStart(2,'0')}`},{at:15,action:'Exchange bass: remove outgoing lows as incoming lows enter'},{at:30,action:'Remove outgoing track'}],
  warnings:n===1?['Vocal overlap unverified']:n===2?['Audition time-stretch quality','Both windows contain sustained bass; plan a bass handover']:[],reason:`${catalogue[[2,3,0][n]].name}: ${[16,8,32][n]} bars; tonal compatibility 0.${9-n}0`,preview_supported:n!==2,equipment:['eq'],skill:2});
 const plan={id:'plan-1',schema_version:1,track_ids:['track-0','track-1','track-2','track-5'],transition_ids:['opt-track-0-track-1-0','opt-track-1-track-2-1','opt-track-2-track-5-0'],duration_seconds:1260,score:7.2,stale:false,explanation:'Balances valid transition options, requested duration and the energy arc.',omission_reasons:{'track-3':'Not selected by this alternative\'s duration, energy and transition tradeoffs'},brief:{schema_version:1,target_seconds:1200,tolerance_seconds:120}};
 const byId=(id:string)=>tracks.find(t=>t.id===id)!;
 const jobs=[{id:'job-1',kind:'analyze',state:'running',stage:'Separating stems (Demucs)',progress:.42,params:{track_ids:['track-9','track-10','track-11']},completed:[]},{id:'job-2',kind:'analyze',state:'paused',stage:'features',progress:.18,params:{track_ids:['track-4']},error:'Interrupted at application shutdown; resume to continue',completed:[]}];
 const responses:Record<string,(p:any)=>any>={
  tracks:()=>tracks,jobs:()=>jobs,sets:()=>[plan],catalogue:()=>catalogue,
  settings:(p:any)=>p?.key?p.value:{experimental_models:['edmformer'],light_jobs:2,heavy_jobs:1,cache_dir:'',model_dir:''},
  transitions:(p:any)=>{const a=byId(p.from),targets=p.to?[byId(p.to)]:tracks.filter(t=>t.id!==a.id).slice(0,2);return targets.flatMap(b=>[0,1,2].map(n=>option(a,b,n))).slice(0,6);},
  transition:(p:any)=>{const [,a,b,n]=p.id.match(/^opt-(track-\d+)-(track-\d+)-(\d)$/)||[];return option(byId(a),byId(b),+n);},
  evidence:()=>({enrichment:{claims:[{kind:'genre',value:'Tech house',source_type:'web',confidence:'medium',retrieved:'2026-10-19',url:'https://example.com/release',citation_check:{status:'verified',reason:'URL consulted'}}],research:{requested:true,consulted_sources:['a','b'],model:'GPT-6.1-Sol'}}}),
  benchmark:()=>({records:[]}),'workspace-health':()=>({healthy:true,warnings:[],counts:{tracks:tracks.length,revisions:5,jobs:2}}),
  'workspace-backups':()=>[{id:'b1',created:'2026-10-19 08:12',counts:{tracks:12,revisions:5},reason:'open'},{id:'b2',created:'2026-10-18 22:40',counts:{tracks:12,revisions:4},reason:'close'}],
  manifests:()=>[{id:'m1',created:'2026-10-18 21:05',written:['track-0']}],doctor:()=>({python:'3.11',ffmpeg:true,wsl:true,gpu:'none (CPU)'}),
  plan:()=>({plans:[plan],conflicts:[]}),'rekordbox-preview':(p:any)=>({id:'pv',stale:false,changes:[{track_id:p.track_ids?.[0]||'track-0',operations:[{role:'A',target:{InMsec:6000,OutMsec:21000},delete:[]},{role:'B',target:{InMsec:36000,OutMsec:-1},delete:['x']}],comment_before:'',comment_after:''}],conflicts:[]}),
 };
 (window as any).autocue={
  call:async(method:string,params?:any)=>{await new Promise(r=>setTimeout(r,120));return (responses[method]||(()=>({})))(params);},
  pick:async()=>[],media:async()=>'',previewMedia:async()=>({url:'',note:'Preview rendering is unavailable in the browser preview.'}),export:async()=>({}),
  account:async(action:string)=>action==='status'?{accounts:[{id:'acct',email:'dj@example.com',plan_enabled:true}],active:'acct',selected_model:'gpt'}:action==='models'?[{id:'gpt',name:'GPT-6.1-Sol'}]:{},
  onEvent:()=>()=>{},
 };
}
export {};
