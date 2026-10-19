import {createReadStream} from 'node:fs';
import {stat} from 'node:fs/promises';
import path from 'node:path';
import {Readable} from 'node:stream';

const types:Record<string,string>={'.wav':'audio/wav','.flac':'audio/flac','.mp3':'audio/mpeg','.m4a':'audio/mp4','.aac':'audio/aac',
  '.aif':'audio/aiff','.aiff':'audio/aiff','.ogg':'audio/ogg','.opus':'audio/ogg'};

// Parses a single "bytes=" range. Returns null when absent, 'invalid' when unsatisfiable.
export function parseRange(header:string|null,size:number):{start:number;end:number}|null|'invalid'{
  if(!header)return null;
  const match=/^bytes=(\d*)-(\d*)$/.exec(header.trim());
  if(!match||(!match[1]&&!match[2])||size<=0)return 'invalid';
  let start:number,end:number;
  if(!match[1]){const suffix=+match[2];if(!suffix)return 'invalid';start=Math.max(0,size-suffix);end=size-1;}
  else{start=+match[1];end=match[2]?Math.min(+match[2],size-1):size-1;}
  return start>end||start>=size?'invalid':{start,end};
}

// Serves a local file with byte-range support. Chromium only treats media as seekable
// when the server answers range requests with 206 Partial Content.
export async function serveFile(file:string,rangeHeader:string|null):Promise<Response>{
  const size=(await stat(file)).size;
  const type=types[path.extname(file).toLowerCase()]||'application/octet-stream';
  const range=parseRange(rangeHeader,size);
  if(range==='invalid')return new Response(null,{status:416,headers:{'Content-Range':`bytes */${size}`}});
  const {start,end}=range||{start:0,end:size-1};
  const body=size?Readable.toWeb(createReadStream(file,{start,end})) as ReadableStream:null;
  const headers:Record<string,string>={'Content-Type':type,'Accept-Ranges':'bytes','Content-Length':String(size?end-start+1:0)};
  if(range)headers['Content-Range']=`bytes ${start}-${end}/${size}`;
  return new Response(body,{status:range?206:200,headers});
}
