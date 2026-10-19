import {afterEach,describe,it,expect,vi} from 'vitest';
import {mkdtemp,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {generateKeyPair,SignJWT,jwtVerify} from 'jose';
import {ChatGPT} from './auth.js';
const nativeFetch=globalThis.fetch;
afterEach(()=>vi.unstubAllGlobals());
async function fixture(){
 const home=await mkdtemp(join(tmpdir(),'autocue-auth-'));
 const keys=await generateKeyPair('RS256');let opened='',subject='subject-a',nonceOverride:string|undefined,refreshCalls=0;
 async function sign(client:string,nonce?:string){return new SignJWT({sub:subject,email:'fixture@example.invalid',...(nonce?{nonce}: {})}).setProtectedHeader({alg:'RS256'}).setIssuer('https://auth.openai.com').setAudience(client).setIssuedAt().setExpirationTime('5m').sign(keys.privateKey);}
 const account=new ChatGPT(home,{encrypt:s=>Buffer.from(s).map(b=>b^0x55),decrypt:b=>Buffer.from(b).map(v=>v^0x55).toString()},async url=>{opened=url;},(token,audience)=>jwtVerify(token,keys.publicKey,{issuer:'https://auth.openai.com',audience,algorithms:['RS256']}));
 await account.load();
 vi.stubGlobal('fetch',async(url:any,init:any)=>{
   if(String(url).includes('/oauth/token')){const params=init.body as URLSearchParams;if(params.get('grant_type')==='refresh_token')refreshCalls++;
     const nonce=params.get('grant_type')==='authorization_code'?(nonceOverride||new URL(opened).searchParams.get('nonce')!):undefined;
     return new Response(JSON.stringify({access_token:'ACCESS_FIXTURE',refresh_token:'REFRESH_FIXTURE',id_token:await sign(params.get('client_id')!,nonce),expires_in:3600,scope:'openid profile chatgpt.tokens.use.direct'}),{status:200});}
   if(String(url).endsWith('/models'))return new Response(JSON.stringify({models:[{slug:'visible',display_name:'Visible',visibility:'list'},{slug:'hidden',display_name:'Hidden',visibility:'hidden'}]}));
   throw Error('Unexpected network request in isolated fixture: '+url);
 });
 async function login(id='oaiapp_fixture'){
   opened='';const pending=account.signIn();await vi.waitFor(()=>expect(opened).not.toBe(''));
   const authorization=new URL(opened);const callback=new URL(authorization.searchParams.get('redirect_uri')!);
   callback.searchParams.set('state',authorization.searchParams.get('state')!);callback.searchParams.set('client_id',id);callback.searchParams.set('code','fixture');
   const response=nativeFetch(callback);const result=await pending;await response;return result;
 }
 return {account,home,login,get opened(){return opened;},set subject(value:string){subject=value;},set nonce(value:string){nonceOverride=value;},get refreshCalls(){return refreshCalls;}};
}
describe('isolated account lifecycle',()=>{
 it('uses PKCE, filters models, persists selection, and exposes no tokens',async()=>{const f=await fixture();await f.login();const url=new URL(f.opened);expect(url.searchParams.get('code_challenge_method')).toBe('S256');expect(url.searchParams.get('code_challenge')).toBeTruthy();expect((await f.account.models()).map((m:any)=>m.id)).toEqual(['visible']);await f.account.chooseModel('visible');expect(f.account.status().selected_model).toBe('visible');expect(JSON.stringify(f.account.status())).not.toContain('ACCESS_FIXTURE');expect((await readFile(join(f.home,'accounts.enc'))).toString()).not.toContain('ACCESS_FIXTURE');});
 it('cancels an open sign-in without saving an account',async()=>{const f=await fixture();const pending=f.account.signIn();const rejected=expect(pending).rejects.toThrow('cancelled');await vi.waitFor(()=>expect(f.account.status().pending).toBe(true));f.account.cancel();await rejected;expect(f.account.status().accounts).toEqual([]);});
 it('rejects a signed token with the wrong nonce',async()=>{const f=await fixture();f.nonce='wrong';await expect(f.login()).rejects.toThrow('identity verification');expect(f.account.status().accounts).toEqual([]);});
 it('serializes refresh and supports switching and disconnecting accounts',async()=>{const f=await fixture();await f.login();f.account.vault.profiles[0].expires_at=0;await Promise.all([f.account.models(),f.account.models()]);expect(f.refreshCalls).toBe(1);f.subject='subject-b';await f.login('oaiapp_second');await f.account.select('oaiapp_fixture');expect(f.account.status().active).toBe('oaiapp_fixture');await f.account.disconnect('oaiapp_fixture');expect(f.account.status().active).toBeNull();expect(f.account.status().accounts).toHaveLength(1);});
 it('rejects an identity change during refresh',async()=>{const f=await fixture();await f.login();f.subject='wrong-account';f.account.vault.profiles[0].expires_at=0;await expect(f.account.models()).rejects.toThrow('identity did not match');});
});
