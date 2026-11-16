import React,{useEffect,useRef,useState} from 'react';
import {Button,Card,Disclosure,Icon,KeyValues,Pill} from './ui';
import {calibrationProgress,calibrationSteps,plainLine,type CalibrationBridge,type CalibrationEvent,type CalibrationStatus} from './calibration';

type LiveLine={state:string;status:string};
const api=window.onset as unknown as Partial<CalibrationBridge>&{onLive?:(cb:(s:LiveLine)=>void)=>()=>void};

type Outcome={kind:'done';path:string;backup:string|null}|{kind:'failed';error:string}|null;

// Teaches OnSet where this rekordbox version keeps its decks, so the live view can follow them.
export function RekordboxLink(){
  const [status,setStatus]=useState<CalibrationStatus|null>(null);
  const [running,setRunning]=useState(false),[lines,setLines]=useState<string[]>([]),[outcome,setOutcome]=useState<Outcome>(null);
  const [startError,setStartError]=useState(''),[live,setLive]=useState<LiveLine|null>(null);
  const [stepStarted,setStepStarted]=useState(Date.now()),[now,setNow]=useState(Date.now());
  const progress=calibrationProgress(lines);
  const lastStep=useRef(progress.step);

  const refresh=()=>api.calibrationStatus?.().then(s=>{setStatus(s);if(s.calibrating)setRunning(true);}).catch(e=>setStartError(String(e?.message||e)));
  useEffect(()=>{refresh();},[]);
  // Notice rekordbox starting or closing while the panel is open.
  useEffect(()=>{if(running)return;const t=setInterval(refresh,4000);return()=>clearInterval(t);},[running]);
  useEffect(()=>api.onLive?.(s=>setLive({state:s.state,status:s.status})),[]);
  useEffect(()=>api.onCalibration?.((e:CalibrationEvent)=>{
    if(e.kind==='line')setLines(ls=>[...ls,e.text]);
    else{setOutcome(e);setRunning(false);refresh();}
  }),[]);
  useEffect(()=>{if(!running)return;const t=setInterval(()=>setNow(Date.now()),500);return()=>clearInterval(t);},[running]);
  useEffect(()=>{if(progress.step!==lastStep.current){lastStep.current=progress.step;setStepStarted(Date.now());}},[progress.step]);

  const start=async()=>{
    setLines([]);setOutcome(null);setStartError('');lastStep.current=1;setStepStarted(Date.now());
    try{await api.startCalibration!();setRunning(true);}
    catch(e:any){setStartError(String(e?.message||e));}
  };

  if(!api.calibrationStatus)return <Card title="rekordbox link"><p className="muted">Calibration needs the OnSet desktop app.</p></Card>;
  if(!status)return <Card title="rekordbox link"><p className="muted">Checking rekordbox…</p></Card>;

  const ready=status.supported&&status.rekordbox_running&&!!status.version;
  const elapsed=Math.max(0,Math.round((now-stepStarted)/1000));
  const current=calibrationSteps[progress.step-1];

  return <>
    <Card title="rekordbox link" actions={!running&&<Button size="sm" kind="ghost" icon="refresh" onClick={refresh}>Check again</Button>}>
      <p className="muted" style={{marginBottom:12}}>The live view follows rekordbox’s decks by reading its memory; nothing is written to rekordbox. Each rekordbox version keeps its decks somewhere new, so OnSet learns where once per version.</p>
      <KeyValues rows={[
        ['rekordbox',status.rekordbox_running?<Pill state="complete">{status.version?`Running · ${status.version}`:'Running'}</Pill>:<Pill state="partial">Not running</Pill>],
        ['Calibration',status.calibrated?<Pill state="complete">Ready for this version</Pill>:status.version?<Pill state="partial">Needed for {status.version}</Pill>:<Pill state="pending">Start rekordbox to check</Pill>],
        ['Live view',live?<span className="calib-live">{live.status}</span>:undefined],
        ['Saved in',status.calibration_path||status.offsets_dir],
      ]}/>
      {status.problem&&!status.rekordbox_running&&<p className="warning" style={{marginTop:10}}><Icon name="alert" size={14}/>{status.supported?'Open rekordbox in Performance mode, then check again.':status.problem}</p>}
    </Card>

    {!running&&!outcome&&<Card title={status.calibrated?'Calibrate again':'Calibrate'}>
      <p className="muted">Takes about three minutes at the decks; the first, paused part is the longest. Calibrate again after a rekordbox update, or if the live view shows the wrong track or time.</p>
      <ol className="calib-checklist">
        <li>rekordbox is open in <strong>Performance</strong> mode.</li>
        <li>A track is loaded on any deck and <strong>paused</strong>, at least two minutes before any loop or the end. The start isn’t needed. Keep rekordbox in front while the deck plays.</li>
        <li>Press play <strong>only when OnSet asks</strong>.</li>
      </ol>
      <div className="row">
        <Button kind="primary" icon="play" disabled={!ready} onClick={start}>Start calibration</Button>
        {!ready&&<small>{status.supported?'Waiting for rekordbox':'Needs Windows'}</small>}
      </div>
      {startError&&<p className="error-text" style={{marginTop:8}}>{startError}</p>}
    </Card>}

    {(running||outcome)&&<Card title="Calibration" actions={running?<Button size="sm" kind="danger" icon="x" onClick={()=>api.cancelCalibration?.()}>Cancel</Button>:<Button size="sm" icon="refresh" onClick={start} disabled={!ready}>{outcome?.kind==='failed'?'Try again':'Calibrate again'}</Button>}>
      <ol className="calib-steps" aria-label="Calibration steps">
        {calibrationSteps.map((s,i)=>{
          const n=i+1,state=outcome?.kind==='done'||n<progress.step||(n===progress.step&&progress.seen)?'done':n===progress.step&&running?'active':outcome?.kind==='failed'&&n===progress.step?'failed':'';
          return <li key={n} className={state}><span className="calib-dot">{state==='done'?<Icon name="check" size={12}/>:n}</span>{s.title}</li>;
        })}
      </ol>
      {running&&<div className="calib-prompt" role="status" aria-live="assertive">
        <div className="calib-now">{progress.seen?'Got it':current.title}</div>
        <p>{progress.seen?'Hold on for the next step.':current.hint}</p>
        <div className="row calib-meta">
          {progress.scanning&&<span className="calib-busy"><span className="calib-spinner" aria-hidden="true"/>{progress.step===1?"Reading rekordbox’s memory (a minute or two)":"Reading rekordbox’s memory (up to 40 s)"}</span>}
          <small>{elapsed} s</small>
        </div>
      </div>}
      {outcome?.kind==='done'&&<div className="calib-result good" role="status">
        <Icon name="check" size={18}/>
        <div><strong>Calibrated.</strong> The live view is reconnecting to rekordbox with the new calibration.{outcome.backup&&<small className="calib-path">The previous calibration was kept as {outcome.backup}</small>}</div>
      </div>}
      {outcome?.kind==='failed'&&<div className="calib-result bad" role="alert">
        <Icon name="alert" size={18}/>
        <div><strong>{outcome.error.includes('calibration cancelled')?'Cancelled. Nothing was saved.':'Calibration didn’t finish. Nothing was saved.'}</strong>
          {!outcome.error.includes('calibration cancelled')&&<><small className="calib-path">{outcome.error}</small>
          <small>Start again with the deck paused, wait for “Press play now”, and keep the track away from loops and its end.</small></>}
        </div>
      </div>}
      {lines.length>0&&<div style={{marginTop:12}}><Disclosure summary="Details">
        <pre className="calib-log">{lines.map(plainLine).join('\n')}</pre>
      </Disclosure></div>}
    </Card>}
  </>;
}
