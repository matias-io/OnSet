import {spawn} from 'node:child_process';
import path from 'node:path';
import {readFile} from 'node:fs/promises';
const root=process.cwd(),home=path.join(root,'.test-data','desktop-smoke');
const env={...process.env,AUTOCUE_HOME:home,AUTOCUE_SMOKE:'1'};
delete env.ELECTRON_RUN_AS_NODE;
const child=spawn(path.join(root,'node_modules','electron','dist','electron.exe'),[root,'--disable-gpu'],{env,windowsHide:true,stdio:'pipe'});
let errors='';child.stderr.on('data',b=>errors+=b.toString());
const timer=setTimeout(()=>{child.kill();console.error('Desktop smoke test timed out');process.exitCode=1;},30000);
child.on('error',e=>{clearTimeout(timer);console.error(e.message);process.exitCode=1;});
child.on('exit',async code=>{
  clearTimeout(timer);
  if(code){console.error(errors.slice(-2000));process.exitCode=1;return;}
  try{const result=JSON.parse(await readFile(path.join(home,'smoke.json'),'utf8'));if(!result.bridge||!result.text.includes('Library'))throw new Error('Desktop bridge did not load');console.log(JSON.stringify(result));}
  catch(e){console.error(e.message);process.exitCode=1;}
});
