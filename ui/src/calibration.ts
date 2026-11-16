// The rekordbox link panel's model: what the calibrator's report lines
// (crates/onset-transport/src/memory/calibrator.rs) mean for the DJ at the decks.

/** `onset:calibration` events from crates/onset-desktop/src/calibration.rs. */
export type CalibrationEvent=
  |{kind:'line';text:string}
  |{kind:'done';path:string;backup:string|null}
  |{kind:'failed';error:string};

export type CalibrationStatus={supported:boolean;rekordbox_running:boolean;version?:string|null;problem?:string|null;calibrated:boolean;calibration_path?:string|null;offsets_dir?:string;calibrating:boolean;steps:string[]};

export type CalibrationBridge={
  calibrationStatus:()=>Promise<CalibrationStatus>;
  startCalibration:()=>Promise<void>;
  cancelCalibration:()=>Promise<void>;
  onCalibration:(callback:(e:CalibrationEvent)=>void)=>()=>void;
};

/** What the DJ does at each step, in the panel's words. Step 5 is OnSet finishing on its own. */
export const calibrationSteps=[
  {title:'Keep the deck paused',hint:'OnSet is noting what rekordbox runs while nothing plays. Don’t press play yet.'},
  {title:'Press play now',hint:'Leave it playing while OnSet finds the deck.'},
  {title:'Pause the deck',hint:'Press play/pause once.'},
  {title:'Press play again',hint:'Same deck, same place.'},
  {title:'Finishing',hint:'OnSet is checking it can find the deck again. You can leave the decks as they are.'},
];

export type CalibrationProgress={
  /** 1–5: the step in `calibrationSteps` that is under way. */
  step:number;
  /** The calibrator has seen the DJ do this step and is settling before the next. */
  seen:boolean;
  /** A long read of rekordbox's memory is running (20–40 s). */
  scanning:boolean;
};

export function calibrationProgress(lines:string[]):CalibrationProgress{
  let step=1,seen=false,scanning=false;
  for(const line of lines){
    const m=line.match(/^Step (\d):/);
    if(m){step=+m[1];seen=false;scanning=false;continue;}
    const text=line.trim();
    if(text==='seen'){
      seen=true;
      // After the second play nothing more is asked of the DJ.
      if(step===4){step=5;seen=false;}
      continue;
    }
    scanning=text.endsWith('...');
  }
  return {step,seen,scanning};
}

/** The calibrator's "Step N:" prompts end with "(waiting up to 240 s)"; the panel shows its own timer. */
export function plainLine(line:string){
  return line.replace(/\s*\(waiting up to \d+ s\)$/,'');
}
