import {spawn, ChildProcessWithoutNullStreams} from 'node:child_process';
import {createInterface} from 'node:readline';
import {EventEmitter} from 'node:events';

export class Rpc extends EventEmitter {
  process: ChildProcessWithoutNullStreams;
  sequence = 0;
  pending = new Map<number, {resolve:(v:any)=>void; reject:(e:Error)=>void}>();
  constructor(python:string, root:string, home:string) {
    super();
    this.process = spawn(python, ['-u', '-m', 'autocue_app', '--home', home, 'rpc'], {
      cwd: root, windowsHide: true, env: {...process.env, PYTHONPATH: root, PYTHONUTF8:'1', NUMBA_CACHE_DIR: `${home}/numba`}
    });
    createInterface({input:this.process.stdout}).on('line', line => {
      try {
        const value = JSON.parse(line);
        if (value.method) {this.emit('event', value); return;}
        const pending = this.pending.get(value.id);
        if (pending) {
          this.pending.delete(value.id);
          value.error ? pending.reject(new Error(value.error.message)) : pending.resolve(value.result);
        }
      } catch {this.emit('diagnostic', 'Invalid service response');}
    });
    this.process.stderr.on('data', () => {}); // Do not forward arbitrary library logs to renderer.
    const fail = () => {for (const p of this.pending.values()) p.reject(new Error('Analysis service stopped')); this.pending.clear();};
    this.process.on('error', fail);
    this.process.on('exit', fail);
  }
  call(method:string, params:unknown={}) {
    return new Promise<any>((resolve, reject) => {
      const id = ++this.sequence;
      this.pending.set(id, {resolve,reject});
      this.process.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n', error => {
        if (error) {this.pending.delete(id); reject(error);}
      });
    });
  }
  stop(timeoutMs=30000):Promise<void> {
    if(this.process.exitCode!==null)return Promise.resolve();
    return new Promise(resolve=>{
      const finished=()=>{clearTimeout(timer);resolve();};
      const timer=setTimeout(()=>{this.process.kill();resolve();},timeoutMs);
      this.process.once('exit',finished);
      this.process.once('error',finished);
      this.process.stdin.end();
    });
  }
}
