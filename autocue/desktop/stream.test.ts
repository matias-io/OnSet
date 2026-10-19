import {describe,it,expect} from 'vitest';
import {readRecommendationStream} from './stream.js';
function stream(events:any[],split=9999){const bytes=new TextEncoder().encode(events.map(e=>'data: '+JSON.stringify(e)+'\r\n\r\n').join(''));return new ReadableStream<Uint8Array>({start(controller){for(let i=0;i<bytes.length;i+=split)controller.enqueue(bytes.slice(i,i+split));controller.close();}});}
describe('structured recommendation streams',()=>{
 it('requires completion even when partial JSON looks valid',async()=>{await expect(readRecommendationStream(stream([{type:'response.output_text.delta',delta:'{"schema_version":1}'}]))).rejects.toThrow('Interrupted');});
 it('handles split CRLF delimiters and records verified source URLs',async()=>{const result=await readRecommendationStream(stream([{type:'response.output_text.delta',delta:'{"ok":true}'},{type:'response.completed',response:{status:'completed',output:[{action:{sources:[{url:'https://example.com/track'}]}}]}}],1));expect(JSON.parse(result.text)).toEqual({ok:true});expect(result.citations.has('https://example.com/track')).toBe(true);});
 it.each(['response.failed','response.incomplete','error'])('rejects %s',async type=>{await expect(readRecommendationStream(stream([{type}]))).rejects.toThrow('did not complete');});
 it('retains opened pages and incremental citations when completion omits those items',async()=>{
   const result=await readRecommendationStream(stream([
     {type:'response.output_item.done',item:{type:'web_search_call',action:{type:'open_page',url:'https://music.apple.com/us/song/1572859302'}}},
     {type:'response.output_item.done',item:{type:'web_search_call',action:{type:'find_in_page',url:'https://example.com/credits'}}},
     {type:'response.output_text.annotation.added',annotation:{type:'url_citation',url:'https://example.com/release'}},
     {type:'response.output_text.delta',delta:'{"ok":true}'},
     {type:'response.completed',response:{status:'completed',output:[]}}
   ],3));
   expect([...result.citations].sort()).toEqual(['https://example.com/credits','https://example.com/release','https://music.apple.com/us/song/1572859302']);
 });
 it('takes canonical completed text and rejects unsafe citation metadata',async()=>{
   const result=await readRecommendationStream(stream([
     {type:'response.output_text.delta',delta:'{"partial":'},
     {type:'response.completed',response:{status:'completed',output:[{content:[{type:'output_text',text:'{"ok":true}',annotations:[{type:'url_citation',url:'https://example.com/facts'},{type:'url_citation',url:'javascript:alert(1)'},{type:'url_citation',url:'https://user:secret@example.com'},{type:'file_citation',url:'https://example.com/invented'}]}]}]}}
   ]));
   expect(JSON.parse(result.text)).toEqual({ok:true});expect([...result.citations]).toEqual(['https://example.com/facts']);
 });
 it('does not return intermediate research from an incomplete stream',async()=>{
   await expect(readRecommendationStream(stream([{type:'response.output_item.done',item:{action:{type:'open_page',url:'https://example.com'}}},{type:'response.incomplete'}]))).rejects.toThrow('did not complete');
 });
});
