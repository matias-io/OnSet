// Keep the exact URL supplied by the completed response. Only recognize a
// documented resource identity for Apple Music song links, not arbitrary slugs.
function citationIdentity(value:string):string|null {
  try {
    const url=new URL(value);
    if(url.protocol!=='https:'||url.username||url.password)return null;
    const path=url.pathname.replace(/\/$/,'');
    if(url.hostname==='music.apple.com'&&!url.port&&!url.search){
      const match=path.match(/^\/([a-z]{2})\/song\/(?:[^/]+\/)?([0-9]+)$/);
      if(match)return `apple-song:${match[1]}:${match[2]}`;
    }
    url.hash='';return url.href;
  }catch{return null;}
}

export function verifiedCitation(requested:unknown,citations:Set<string>):string|null {
  if(typeof requested!=='string')return null;
  if(citations.has(requested)&&citationIdentity(requested))return requested;
  const identity=citationIdentity(requested);
  if(!identity)return null;
  return [...citations].find(source=>citationIdentity(source)===identity)||null;
}

export function retainEnrichmentEvidence(result:any,citations:Set<string>,model:string,research:boolean){
  for(const record of result.records||[]){
    record.research={requested:research,model,completed:true,consulted_sources:[...citations].sort()};
    for(const claim of record.claims||[]){
      const requested={source_type:claim.source_type,url:claim.url};
      const source=claim.source_type==='web'?verifiedCitation(claim.url,citations):null;
      claim.citation_check={requested_source_type:requested.source_type,requested_url:requested.url,
        status:source?'retained':claim.source_type==='web'?'unverified':'not_requested',
        reason:source?'URL matches completed response research evidence':claim.source_type==='web'?
          (citations.size?'Claim URL was not among the response research sources':'Completed response supplied no research source URLs'):'Model interpretation; no sourced fact asserted'};
      if(source)claim.url=source;
      else if(claim.source_type==='web'){claim.source_type='model';claim.url=null;claim.confidence='unverified';}
    }
  }
  return result;
}
