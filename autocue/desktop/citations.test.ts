import {describe,it,expect} from 'vitest';
import {verifiedCitation,retainEnrichmentEvidence} from './citations';
describe('consulted citation matching',()=>{
  it('retains the actual consulted Apple song URL when only the optional slug differs',()=>{
    const actual='https://music.apple.com/us/song/1572859302';
    expect(verifiedCitation('https://music.apple.com/us/song/fragments/1572859302',new Set([actual]))).toBe(actual);
  });
  it('does not match a different song, storefront, host, resource type or query',()=>{
    const cited=new Set(['https://music.apple.com/us/song/1572859302']);
    for(const value of ['https://music.apple.com/us/song/111','https://music.apple.com/ca/song/1572859302','https://evil.example/us/song/1572859302','https://music.apple.com/us/album/1572859302','https://music.apple.com/us/song/1572859302?i=111'])expect(verifiedCitation(value,cited)).toBeNull();
  });
  it('rejects unconsulted links and credentials, and keeps an exact consulted HTTPS URL',()=>{
    expect(verifiedCitation('https://example.com/claim',new Set())).toBeNull();
    expect(verifiedCitation('https://user:secret@example.com/claim',new Set(['https://user:secret@example.com/claim']))).toBeNull();
    expect(verifiedCitation('https://example.com/claim',new Set(['https://example.com/claim']))).toBe('https://example.com/claim');
  });
});

it('retains actual citations and explains downgraded claims without losing their provenance',()=>{
 const actual='https://music.apple.com/us/song/1572859302';
 const result=retainEnrichmentEvidence({records:[{track_id:'a',claims:[
   {source_type:'web',url:'https://music.apple.com/us/song/fragments/1572859302',confidence:'medium'},
   {source_type:'web',url:'https://example.com/not-consulted',confidence:'high'},
   {source_type:'model',url:null,confidence:'low'}
 ]}]},new Set([actual]),'selected-model',true);
 const record=result.records[0];expect(record.research.consulted_sources).toEqual([actual]);
 expect(record.claims[0]).toMatchObject({source_type:'web',url:actual,citation_check:{status:'retained'}});
 expect(record.claims[1]).toMatchObject({source_type:'model',url:null,confidence:'unverified',citation_check:{status:'unverified',requested_url:'https://example.com/not-consulted'}});
 expect(record.claims[2].citation_check.status).toBe('not_requested');
});
