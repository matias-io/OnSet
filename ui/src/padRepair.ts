/** rekordbox's pad for a djmdCue.Kind: 1–3 are A–C, 5–9 are D–H, and Kind 4 (a memory loop) has none. */
export function padLetter(kind:number):string{
  return ({1:'A',2:'B',3:'C',5:'D',6:'E',7:'F',8:'G',9:'H'} as Record<number,string>)[kind]??'no pad';
}

export type PadRepairRow={title:string,moves:string[],removed:string[],left:string[]};

/** One row per track of a hot pad repair preview: what moves where, what is removed, what stays. */
export function padRepairRows(preview:{moves:any[],drops?:any[],conflicts:any[]}):PadRepairRow[]{
  const rows=new Map<string,PadRepairRow>();
  const row=(title:string|null)=>{
    const key=title||'Untitled';
    if(!rows.has(key))rows.set(key,{title:key,moves:[],removed:[],left:[]});
    return rows.get(key)!;
  };
  for(const m of preview.moves)row(m.title).moves.push(`${m.before.Comment}: ${padLetter(m.before.Kind)} → ${padLetter(m.kind)}`);
  for(const d of preview.drops||[])row(d.title).removed.push(`${d.before.Comment} (pad ${padLetter(d.before.Kind)})`);
  for(const c of preview.conflicts)row(c.title).left.push(c.cue);
  return [...rows.values()].sort((a,b)=>a.title.localeCompare(b.title));
}
