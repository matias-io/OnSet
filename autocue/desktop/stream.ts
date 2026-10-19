export async function readRecommendationStream(body:ReadableStream<Uint8Array>,progress:(text:string)=>void=()=>{}) {
  const reader=body.getReader(),decoder=new TextDecoder(),citations=new Set<string>();
  function addUrl(value:unknown){
    if(typeof value!=='string')return;
    try{const url=new URL(value);if(url.protocol==='https:'&&!url.username&&!url.password)citations.add(value);}catch{}
  }
  function collectItem(item:any){
    for(const source of item?.action?.sources||[])addUrl(source.url);
    // An opened or searched-in page can be absent from the search source list.
    if(['open_page','find_in_page'].includes(item?.action?.type))addUrl(item.action.url);
    for(const content of item?.content||[])for(const annotation of content.annotations||[])
      if(annotation.type==='url_citation')addUrl(annotation.url);
  }
  let text='',buffer='';
  try {
    while(true){
      const chunk=await reader.read();
      if(chunk.done)throw new Error('Interrupted ChatGPT response; no recommendation saved');
      buffer=(buffer+decoder.decode(chunk.value,{stream:true})).replace(/\r\n/g,'\n');
      let end:number;
      while((end=buffer.indexOf('\n\n'))>=0){
        const block=buffer.slice(0,end);buffer=buffer.slice(end+2);
        const payload=block.split('\n').filter(l=>l.startsWith('data:')).map(l=>l.slice(5).trim()).join('\n');
        if(!payload||payload==='[DONE]')continue;
        const event=JSON.parse(payload);
        if(event.type==='response.output_text.delta'){text+=event.delta;progress(event.delta);}
        if(event.type==='response.output_item.done')collectItem(event.item);
        if(event.type==='response.output_text.annotation.added'&&event.annotation?.type==='url_citation')addUrl(event.annotation.url);
        if(['response.failed','response.incomplete','error'].includes(event.type))throw new Error(`ChatGPT did not complete: ${event.response?.error?.code||event.error?.code||event.type}`);
        if(event.type==='response.completed'){
          if(event.response?.status&&event.response.status!=='completed')throw new Error('Response completion status was invalid');
          for(const item of event.response?.output||[])collectItem(item);
          const completedText=(event.response?.output||[]).flatMap((item:any)=>item.content||[]).filter((c:any)=>c.type==='output_text').map((c:any)=>c.text||'').join('');
          if(completedText)text=completedText;
          return {text,citations};
        }
      }
    }
  } finally {await reader.cancel().catch(()=>{});reader.releaseLock();}
}
