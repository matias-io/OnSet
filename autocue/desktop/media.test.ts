import {describe,it,expect} from 'vitest';
import {mkdtempSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {parseRange,serveFile} from './media';

describe('local media ranges',()=>{
 it('parses open, closed, suffix and invalid ranges',()=>{
  expect(parseRange(null,100)).toBeNull();
  expect(parseRange('bytes=0-',100)).toEqual({start:0,end:99});
  expect(parseRange('bytes=10-19',100)).toEqual({start:10,end:19});
  expect(parseRange('bytes=90-500',100)).toEqual({start:90,end:99});
  expect(parseRange('bytes=-10',100)).toEqual({start:90,end:99});
  expect(parseRange('bytes=100-',100)).toBe('invalid');
  expect(parseRange('bytes=5-2',100)).toBe('invalid');
  expect(parseRange('items=0-1',100)).toBe('invalid');
 });
 it('answers range requests with 206 and the requested bytes so audio is seekable',async()=>{
  const file=path.join(mkdtempSync(path.join(tmpdir(),'autocue-media-')),'a.wav');
  writeFileSync(file,Buffer.from([...Array(100).keys()]));
  const whole=await serveFile(file,null);
  expect(whole.status).toBe(200);expect(whole.headers.get('accept-ranges')).toBe('bytes');expect(whole.headers.get('content-type')).toBe('audio/wav');
  expect((await whole.arrayBuffer()).byteLength).toBe(100);
  const part=await serveFile(file,'bytes=40-49');
  expect(part.status).toBe(206);expect(part.headers.get('content-range')).toBe('bytes 40-49/100');
  expect([...new Uint8Array(await part.arrayBuffer())]).toEqual([40,41,42,43,44,45,46,47,48,49]);
  const bad=await serveFile(file,'bytes=200-');
  expect(bad.status).toBe(416);expect(bad.headers.get('content-range')).toBe('bytes */100');
 });
});
