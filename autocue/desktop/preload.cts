import {contextBridge,ipcRenderer} from 'electron';
contextBridge.exposeInMainWorld('autocue',{
  call:(method:string,params:unknown={})=>ipcRenderer.invoke('autocue:call',method,params),
  pick:(kind:string)=>ipcRenderer.invoke('autocue:pick',kind),
  media:(id:string,stem?:string)=>ipcRenderer.invoke('autocue:media',id,stem),
  previewMedia:(id:string,click:boolean)=>ipcRenderer.invoke('autocue:preview-media',id,click),
  export:(params:unknown)=>ipcRenderer.invoke('autocue:export',params),
  account:(action:string,params:unknown={})=>ipcRenderer.invoke('autocue:account',action,params),
  onEvent:(callback:(value:unknown)=>void)=>{const listener=(_:unknown,value:unknown)=>callback(value);ipcRenderer.on('autocue:event',listener);return ()=>ipcRenderer.removeListener('autocue:event',listener);}
});
