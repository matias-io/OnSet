export const emptyFilters={bpmMin:'',bpmMax:'',energyMin:'',energyMax:'',key:'',genre:'',vocals:'any',analysis:'any'};
export function canonicalKey(value:any):string|null{
 let name=typeof value==='string'?value:value?.camelot||value?.name||'';
 name=name.trim().replaceAll('♭','b').replaceAll('♯','#');
 if(/^(1[0-2]|[1-9])[AB]$/i.test(name))return name.toUpperCase();
 const match=name.match(/^([a-g][#b]?)\s*(major|minor|maj|min|m)?$/i);if(!match)return null;
 let note=match[1][0].toUpperCase()+match[1].slice(1);note=({Db:'C#','D#':'Eb',Gb:'F#','G#':'Ab','A#':'Bb',Cb:'B','B#':'C',Fb:'E','E#':'F'} as any)[note]||note;
 const minor=match[2]!=='M'&&['minor','min','m'].includes((match[2]||'').toLowerCase());
 const wheel=minor?['Ab','Eb','Bb','F','C','G','D','A','E','B','F#','C#']:['B','F#','C#','Ab','Eb','Bb','F','C','G','D','A','E'];
 const i=wheel.indexOf(note);return i<0?null:`${i+1}${minor?'A':'B'}`;
}
export function matchesFilters(track:any,f:typeof emptyFilters){
 const rating=track.energy??track.analysis?.energy_profile?.rating;
 const range=(value:any,min:string,max:string)=>(!min&&!max)||typeof value==='number'&&Number.isFinite(value)&&(!min||value>=+min)&&(!max||value<=+max);
 if(!range(track.bpm,f.bpmMin,f.bpmMax)||!range(rating,f.energyMin,f.energyMax))return false;
 if(f.key&&canonicalKey(track.key)!==canonicalKey(f.key))return false;
 if(f.genre&&!track.genres.some((g:string)=>g.toLowerCase().includes(f.genre.toLowerCase())))return false;
 if(f.analysis!=='any'&&track.analysis_state!==f.analysis)return false;
 const windows=track.overrides?.sections||track.analysis?.structure?.windows||[];
 const known=windows.filter((w:any)=>typeof w.vocals==='boolean');
 const vocal=known.some((w:any)=>w.vocals===true);
 return f.vocals==='any'||f.vocals==='unknown'&&!known.length||f.vocals==='present'&&vocal||f.vocals==='absent'&&known.length>0&&!vocal;
}

export function analysisSelection(tracks:{id:string}[],visible:{id:string}[],marked:string[]):string[]{
 const existing=new Set(tracks.map(t=>t.id));
 const checked=[...new Set(marked)].filter(id=>existing.has(id));
 return checked.length?checked:visible.map(t=>t.id);
}

export const defaultBrief={schema_version:1,target_seconds:3600,tolerance_seconds:120,max_pitch_pct:6,key_lock:true,skill:2,equipment:['eq','filter','loop','echo','reverb'],energy_arc:[4,6,8,7],must_play:[] as string[],excluded:[] as string[],techniques:[] as string[],genres:[] as string[],mood:'',variety:.5,exploration:.5,smoothness:.5,contrast:.5,playlists:[] as string[],transpose:{} as Record<string,number>,pinned:[] as any[],start:null as string|null,finish:null as string|null};

// Restore a saved brief field by field: wrong-typed values fall back to defaults and, when the library is known, removed tracks are dropped.
export function restoreBrief(saved:any,trackIds?:string[]){
 const brief:any={...defaultBrief,equipment:[...defaultBrief.equipment],energy_arc:[...defaultBrief.energy_arc]};
 if(!saved||typeof saved!=='object'||(saved.schema_version??1)!==1)return brief;
 const number=(v:any,min:number,max:number)=>typeof v==='number'&&Number.isFinite(v)&&v>=min&&v<=max;
 const strings=(v:any)=>Array.isArray(v)&&v.every(x=>typeof x==='string');
 if(number(saved.target_seconds,1,86400))brief.target_seconds=saved.target_seconds;
 if(number(saved.tolerance_seconds,0,86400))brief.tolerance_seconds=saved.tolerance_seconds;
 if(number(saved.max_pitch_pct,0,50))brief.max_pitch_pct=saved.max_pitch_pct;
 if(typeof saved.key_lock==='boolean')brief.key_lock=saved.key_lock;
 if([1,2,3].includes(saved.skill))brief.skill=saved.skill;
 if(Array.isArray(saved.energy_arc)&&saved.energy_arc.length&&saved.energy_arc.every((v:any)=>number(v,1,10)))brief.energy_arc=[...saved.energy_arc];
 for(const key of ['equipment','techniques','genres','must_play','excluded','playlists'])if(strings(saved[key]))brief[key]=[...saved[key]];
 if(typeof saved.mood==='string')brief.mood=saved.mood;
 if(number(saved.variety,0,1))brief.variety=saved.variety;
 if(number(saved.exploration,0,1))brief.exploration=saved.exploration;
 if(number(saved.smoothness,0,1))brief.smoothness=saved.smoothness;
 if(number(saved.contrast,0,1))brief.contrast=saved.contrast;
 if(saved.transpose&&typeof saved.transpose==='object'&&!Array.isArray(saved.transpose))brief.transpose=Object.fromEntries(Object.entries(saved.transpose).filter(([,v])=>Number.isInteger(v)&&Math.abs(v as number)<=12&&v!==0));
 if(Array.isArray(saved.pinned))brief.pinned=saved.pinned.filter((p:any)=>p&&typeof p==='object'&&(typeof p.transition_id==='string'||typeof p.track_id==='string'&&Number.isInteger(p.position)&&p.position>=0));
 for(const key of ['start','finish'])if(typeof saved[key]==='string')brief[key]=saved[key];
 if(strings(saved.track_ids)&&saved.track_ids.length)brief.track_ids=[...saved.track_ids];
 return trackIds?pruneBriefTracks(brief,trackIds):brief;
}

export function pruneBriefTracks(brief:any,trackIds:string[]){
 const known=new Set(trackIds),keep=(id:string)=>known.has(id);
 const next={...brief,must_play:brief.must_play.filter(keep),excluded:brief.excluded.filter(keep),
  pinned:brief.pinned.filter((p:any)=>!p.track_id||keep(p.track_id)),
  start:brief.start&&keep(brief.start)?brief.start:null,finish:brief.finish&&keep(brief.finish)?brief.finish:null,
  transpose:Object.fromEntries(Object.entries(brief.transpose||{}).filter(([id])=>keep(id)))};
 if(brief.track_ids)next.track_ids=brief.track_ids.filter(keep);
 return JSON.stringify(next)===JSON.stringify(brief)?brief:next;
}

// Corrected grids take precedence over analysed beats for snapping and keyboard steps.
export function snapGrid(track:any):number[]{
 return track?.overrides?.grid||track?.analysis?.beats||track?.analysis?.downbeats||[];
}
export function nearest(values:number[],t:number){
 return values.length?values.reduce((a,b)=>Math.abs(b-t)<Math.abs(a-t)?b:a):t;
}

// Single-letter review shortcuts must not fire for browser/application chords such as Ctrl+A or Ctrl+R.
export function plainKey(event:{ctrlKey?:boolean;metaKey?:boolean;altKey?:boolean}){
 return !event.ctrlKey&&!event.metaKey&&!event.altKey;
}

// Cue role colours shared by the waveform, cue list and write previews.
export const roleColor:Record<string,string>={A:'#34d399',B:'#a3e635',C:'#facc15',D:'#fb923c',E:'#60a5fa',F:'#a78bfa',G:'#f472b6',I:'#2dd4bf',L:'#67e8f9'};
export function clock(seconds:number=0,tenths=true){
 const s=Math.max(0,seconds||0),m=Math.floor(s/60),rest=s%60;
 return `${m}:${tenths?rest.toFixed(1).padStart(4,'0'):Math.floor(rest).toString().padStart(2,'0')}`;
}

// Zero-based bar containing a playback position, for labelling events while listening.
export function barAt(track:any,seconds:number):number|null{
 // A corrected grid stores downbeats, matching the planner's beat_times().
 const db:number[]=track?.overrides?.grid||track?.analysis?.downbeats||[];
 if(!db.length||seconds<db[0])return db.length?0:null;
 let lo=0,hi=db.length-1;
 while(lo<hi){const mid=(lo+hi+1)>>1;if(db[mid]<=seconds)lo=mid;else hi=mid-1;}
 return lo;
}

export const benchmarkEvents=[['first_full_groove','First full groove'],['first_breakdown','First breakdown'],['later_drop','Later drop'],['outro','Outro']] as const;
