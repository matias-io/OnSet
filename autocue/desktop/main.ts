import {app, BrowserWindow, ipcMain, dialog, shell, safeStorage, protocol, net, Tray, Menu, nativeImage} from 'electron';
import {existsSync} from 'node:fs';
import {readFile, writeFile, mkdir} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {Rpc} from './rpc.js';
import {ChatGPT} from './auth.js';

protocol.registerSchemesAsPrivileged([{scheme:'autocue-audio',privileges:{standard:true,secure:true,stream:true,supportFetchAPI:true}}]);
const here=path.dirname(fileURLToPath(import.meta.url));
let window:BrowserWindow|null=null, rpc:Rpc, chat:ChatGPT, quitting=false, tray:Tray;
const media=new Map<string,string>();
const calls=new Set(['workspace-health','workspace-backups','workspace-backup','workspace-restore-preview','workspace-restore','doctor','tracks','track','import','analyze','edit','review','review-bulk','undo','jobs','job-control','catalogue','transitions','plan','validate-plan','import-plan','import-plan-file','preview','export','calibrate-energy','energy-judgment','benchmark','evidence','ai-context','enrichment-import','rekordbox-preview','rekordbox-apply','restore','rekordbox-import','manifests','settings','worker-setup']);
const qaBuild=path.basename(process.execPath).toLowerCase()==='autocueqa.exe';
const home=qaBuild?path.resolve(path.dirname(process.execPath),'qa-data'):
  process.env.AUTOCUE_HOME || path.join(process.env.LOCALAPPDATA || app.getPath('userData'),'Autocue');
const root=app.isPackaged?path.join(process.resourcesPath,'python-src'):path.resolve(here,'..');
function requireSender(event:Electron.IpcMainInvokeEvent) {
  if(!window || event.sender!==window.webContents || event.senderFrame!==window.webContents.mainFrame)throw new Error('Unknown application sender');
}
async function mediaUrl(file:string) {
  const id=crypto.randomUUID();media.set(id,path.resolve(file));return `autocue-audio://local/${id}`;
}

async function start() {
  await mkdir(home,{recursive:true});
  const python=app.isPackaged?path.join(process.resourcesPath,'runtime','python.exe'):
    [process.env.AUTOCUE_PYTHON,path.join(root,'.venv','Scripts','python.exe'),path.join(root,'.runtime','Scripts','python.exe')].find(p=>p && existsSync(p));
  if(!python || !existsSync(python)) {
    await dialog.showMessageBox({type:'error',message:'Autocue runtime is missing',detail:'Run scripts/bootstrap.ps1, or prepare the runtime before packaging.'});
    app.quit();return;
  }
  rpc=new Rpc(python,root,home);
  chat=new ChatGPT(home,{
    encrypt:s=>{if(!safeStorage.isEncryptionAvailable())throw new Error('Windows credential protection is unavailable');return safeStorage.encryptString(s);},
    decrypt:b=>safeStorage.decryptString(b)
  },url=>shell.openExternal(url));
  await chat.load();
  window=new BrowserWindow({width:1440,height:950,minWidth:1060,minHeight:700,backgroundColor:'#101416',title:'Autocue',
    webPreferences:{preload:path.join(here,'preload.cjs'),contextIsolation:true,nodeIntegration:false,sandbox:true}});
  window.webContents.setWindowOpenHandler(()=>({action:'deny'}));
  window.webContents.on('will-navigate',event=>event.preventDefault());
  window.on('close',event=>{if(!quitting){event.preventDefault();window?.hide();}});
  rpc.on('event',e=>window?.webContents.send('autocue:event',e));
  protocol.handle('autocue-audio',request=>{
    const id=new URL(request.url).pathname.slice(1), file=media.get(id);
    if(!file)return new Response('Unknown media',{status:404});
    return net.fetch(pathToFileURL(file).toString(),{headers:request.headers});
  });
  const icon=nativeImage.createFromDataURL('data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==');
  tray=new Tray(icon);tray.setToolTip('Autocue — DJ preparation');
  tray.setContextMenu(Menu.buildFromTemplate([{label:'Open Autocue',click:()=>window?.show()},{label:'Quit',click:()=>app.quit()}]));
  tray.on('double-click',()=>window?.show());
  ipcMain.handle('autocue:call',async(event,method:string,params:any={})=>{
    requireSender(event);
    if(method==='quit'){setImmediate(()=>app.quit());return {quitting:true};}
    if(method==='open-source'){const source=new URL(params.url);if(source.protocol!=='https:'||source.username||source.password)throw new Error('Source links must use HTTPS without credentials');await shell.openExternal(source.href);return {opened:true};}
    if(!calls.has(method)&&!['sets','reliability','replan-order','transition','benchmark-label','relocate','transition-alternatives','replan','review-queue','plan-job','transitions-job','import-workspace','relocate-cache'].includes(method))throw new Error('Unknown service command');
    if(method==='import'){
      // Import paths originate from a native file/folder picker, or a saved known path.
      if(!Array.isArray(params.paths))throw new Error('Expected import paths');
    }
    return rpc.call(method,params);
  });
  ipcMain.handle('autocue:pick',async(event,kind:string)=>{
    requireSender(event);
    const result=await dialog.showOpenDialog(window!,{properties:kind==='folder'?['openDirectory']:kind==='workspace'?['openFile']:['openFile','multiSelections'],
      filters:kind==='json'?[{name:'JSON',extensions:['json']}]:kind==='workspace'?[{name:'Autocue workspace export',extensions:['json','db']}]:[{name:'Music and analysis',extensions:['wav','flac','mp3','m4a','aiff','aif','ogg','opus','json']}]});
    return result.canceled?[]:result.filePaths;
  });
  ipcMain.handle('autocue:media',async(event,trackId:string,stem?:string)=>{
    requireSender(event); const track=await rpc.call('track',{id:trackId});
    if(stem && !['drums','bass','vocals','other'].includes(stem))throw new Error('Unknown stem');
    const file=stem?track.analysis?.deep?.stems?.[stem]:track.path;
    if(!file)throw new Error('This stem is unavailable; complete local deep analysis first');
    if(!existsSync(file))throw new Error(stem?'Local stem files are missing; rerun deep analysis':'Audio is missing; use Locate moved audio to verify its new path');
    return mediaUrl(file);
  });
  ipcMain.handle('autocue:preview-media',async(event,id:string,click:boolean)=>{
    requireSender(event);const result=await rpc.call('preview',{id,click});return {...result,url:await mediaUrl(result.path)};
  });
  ipcMain.handle('autocue:export',async(event,params:any)=>{
    requireSender(event);
    const result=await dialog.showSaveDialog(window!,{defaultPath:params.name||'autocue.json'});
    if(result.canceled||!result.filePath)return null;
    if(path.extname(result.filePath).toLowerCase()==='.pdf'){
      const temp=path.join(home,'set-sheet.html');
      await rpc.call('export',{...params,path:temp});
      const print=new BrowserWindow({show:false,webPreferences:{sandbox:true,nodeIntegration:false,contextIsolation:true}});
      try{await print.loadFile(temp);await writeFile(result.filePath,await print.webContents.printToPDF({printBackground:true}));}
      finally{print.destroy();}
      return {path:result.filePath};
    }
    return rpc.call('export',{...params,path:result.filePath});
  });
  ipcMain.handle('autocue:account',async(event,action:string,params:any={})=>{
    requireSender(event);
    if(action==='status')return chat.status();
    if(action==='sign-in')return chat.signIn(params.id);
    if(action==='cancel')return chat.cancel();
    if(action==='select')return chat.select(params.id);
    if(action==='disconnect')return chat.disconnect(params.id);
    if(action==='models')return chat.models();
    if(action==='model')return chat.chooseModel(params.id);
    if(action==='manage'){await shell.openExternal('https://chatgpt.com/#settings');return null;}
    if(action==='request'){
      const context=await rpc.call('ai-context',{track_ids:params.track_ids,brief:params.brief});
      const result=await chat.request({...context,task:params.task},params.model,params.research,delta=>window?.webContents.send('autocue:event',{method:'ai-progress',params:{delta}}));
      if(params.task==='enrichment')return rpc.call('enrichment-import',{...result,allowed_ids:context.tracks.map((t:any)=>t.id)});
      try{return await rpc.call('import-plan',{plan:{...result,brief:params.brief,revisions:Object.fromEntries(context.tracks.map((t:any)=>[t.id,t.revision]))}});}
      catch(error:any){const replacement=await rpc.call('plan',{brief:params.brief});return {rejected_ai:error.message,alternatives:replacement.plans,conflicts:replacement.conflicts};}
    }
    throw new Error('Unknown account action');
  });
  if(process.env.AUTOCUE_DEV_URL)await window.loadURL(process.env.AUTOCUE_DEV_URL);
  else await window.loadFile(path.join(app.getAppPath(),'dist','index.html'));
  if(qaBuild)window.setTitle('Autocue QA — isolated verification workspace');
  if(process.env.AUTOCUE_SMOKE==='1') {
    await new Promise(resolve=>setTimeout(resolve,2500));
    const report=await window.webContents.executeJavaScript(`({title:document.title,bridge:!!window.autocue,text:document.body.innerText.slice(0,500)})`);
    await writeFile(path.join(home,'smoke.json'),JSON.stringify(report,null,2));
    await writeFile(path.join(home,'desktop.png'),(await window.webContents.capturePage()).toPNG());
    await window.webContents.executeJavaScript(`Array.from(document.querySelectorAll('.track-link')).find(b=>b.textContent?.includes('STAY'))?.click()`);
    for(let attempt=0;attempt<40;attempt++){
      await new Promise(resolve=>setTimeout(resolve,250));
      if(await window.webContents.executeJavaScript(`(document.querySelector('audio')?.readyState||0)>=2`))break;
    }
    await writeFile(path.join(home,'track-review.png'),(await window.webContents.capturePage()).toPNG());
    const playback=await window.webContents.executeJavaScript(`({ready:document.querySelector('audio')?.readyState,duration:document.querySelector('audio')?.duration,source:document.querySelector('audio')?.currentSrc,network:document.querySelector('audio')?.networkState,errors:document.querySelector('audio')?.error?.message})`);
    await writeFile(path.join(home,'playback.json'),JSON.stringify(playback));
    app.quit();
  }
}
app.on('before-quit',event=>{
  event.preventDefault();
  if(quitting)return;
  quitting=true;chat?.cancel();tray?.destroy();
  window?.setTitle('Autocue - checkpointing jobs');
  (rpc?rpc.stop():Promise.resolve()).finally(()=>app.exit(0));
});
app.on('activate',()=>window?.show());
// Isolated QA and production can coexist; each has one desktop process.
if(qaBuild)app.setPath('userData',path.join(app.getPath('userData'),'qa'));
if(!app.requestSingleInstanceLock())app.quit();
else {
  app.on('second-instance',()=>{if(window?.isMinimized())window.restore();window?.show();window?.focus();});
  app.whenReady().then(start).catch(async error=>{await dialog.showMessageBox({type:'error',message:'Autocue could not start',detail:String(error.message)});app.quit();});
}
