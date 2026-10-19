import {EventEmitter} from 'node:events';
import {it,expect,vi} from 'vitest';
import {Rpc} from './rpc';
function fixture(){const process=Object.assign(new EventEmitter(),{exitCode:null as number|null,stdin:{end:vi.fn()},kill:vi.fn()});const rpc=Object.create(Rpc.prototype) as Rpc;rpc.process=process as any;return {rpc,process};}
it('waits for the owned service to exit after closing input',async()=>{const {rpc,process}=fixture();const finished=vi.fn();const pending=rpc.stop(1000).then(finished);await Promise.resolve();expect(process.stdin.end).toHaveBeenCalledOnce();expect(finished).not.toHaveBeenCalled();process.emit('exit',0);await pending;expect(process.kill).not.toHaveBeenCalled();});
it('bounds shutdown of an unresponsive owned service',async()=>{const {rpc,process}=fixture();await rpc.stop(5);expect(process.kill).toHaveBeenCalledOnce();});
it('does not close input on a service that already exited',async()=>{const {rpc,process}=fixture();process.exitCode=0;await rpc.stop();expect(process.stdin.end).not.toHaveBeenCalled();});
