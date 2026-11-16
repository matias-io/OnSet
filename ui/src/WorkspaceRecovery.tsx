import React,{useEffect,useState} from 'react';
import {Button,Card,Disclosure,Icon,Pill} from './ui';

export function WorkspaceRecovery({run}:{run:(action:()=>Promise<any>,message?:string)=>Promise<any>}){
  const [health,setHealth]=useState<any>(null),[backups,setBackups]=useState<any[]>([]),[preview,setPreview]=useState<any>(null),[approved,setApproved]=useState(false);
  async function refresh(){setHealth(await window.onset.call('workspace-health'));setBackups(await window.onset.call('workspace-backups'));}
  useEffect(()=>{refresh().catch(()=>{});},[]);
  return <Card title="Workspace" actions={<>
    <Button size="sm" kind="ghost" icon="refresh" label="Refresh workspace health" onClick={()=>run(refresh)}/>
    <Button size="sm" disabled={health&&!health.healthy} onClick={()=>run(async()=>{await window.onset.call('workspace-backup');await refresh();},'Workspace backup verified')}>Back up now</Button>
    <Button size="sm" icon="download" onClick={()=>run(()=>window.onset.export({kind:'workspace-backup',name:'onset-workspace.db'}))}>Export full history</Button></>}>
    {health&&<div className="row" style={{marginBottom:6}}>
      <Pill state={health.healthy?'complete':'failed'}>{health.healthy?'Healthy':'Needs recovery review'}</Pill>
      <span className="muted">{health.counts.tracks??'?'} tracks · {health.counts.revisions??'?'} revisions · {health.counts.jobs??'?'} jobs</span>
    </div>}
    {health?.warnings.map((warning:string)=><p className="warning" key={warning}><Icon name="alert" size={14}/>{warning}</p>)}
    <p className="hint" style={{margin:'8px 0 4px'}}>Snapshots include profiles, corrections, undo history, jobs and plans — not audio, model caches, credentials or rekordbox. One is saved automatically when OnSet opens and closes.</p>
    {backups.length>0&&<Disclosure summary={`${backups.length} snapshots`}>
      {backups.map(backup=><div className="list-row" key={backup.id}>
        <span className="grow">{backup.created} <span className="dim">· {backup.counts.tracks} tracks · {backup.counts.revisions} revisions · {backup.reason}</span></span>
        <Button size="sm" onClick={()=>run(async()=>{setApproved(false);setPreview(await window.onset.call('workspace-restore-preview',{id:backup.id}));})}>Preview restore</Button>
      </div>)}
    </Disclosure>}
    {preview&&<div className="write-preview">
      <h4>Restore this snapshot</h4>
      <p className="muted" style={{marginBottom:10}}>{preview.replaces}. The current state is saved first.</p>
      <div className="table-wrap"><table><thead><tr><th>Records</th><th>Now</th><th>After restore</th></tr></thead><tbody>
        {Object.keys(preview.before).map(table=><tr key={table}><td>{table}</td><td>{preview.before[table]}</td><td>{preview.after[table]}</td></tr>)}
      </tbody></table></div>
      <div className="row" style={{marginTop:12}}>
        <label className="check grow"><input type="checkbox" checked={approved} onChange={e=>setApproved(e.target.checked)}/>Replace the current workspace with this snapshot</label>
        <Button size="sm" kind="ghost" onClick={()=>{setPreview(null);setApproved(false);}}>Cancel</Button>
        <Button size="sm" kind="danger" disabled={!approved} onClick={()=>run(async()=>{await window.onset.call('workspace-restore',{id:preview.id,token:preview.token,approved:true});setPreview(null);setApproved(false);await refresh();},'Workspace restored; previous state retained in a rollback snapshot')}>Restore</Button>
      </div>
    </div>}
  </Card>;
}
