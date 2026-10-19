import {describe,it,expect} from 'vitest';
import {validateCallback,equalState} from './auth.js';
describe('ChatGPT callback validation',()=>{
 it('requires a matching state and issued client ID',()=>{
   expect(validateCallback(new URL('http://127.0.0.1/auth/callback?state=abc&code=x&client_id=oaiapp_1'),'abc')).toEqual({client:'oaiapp_1',code:'x'});
   expect(()=>validateCallback(new URL('http://127.0.0.1/auth/callback?state=bad&code=x&client_id=oaiapp_1'),'abc')).toThrow();
   expect(()=>validateCallback(new URL('http://127.0.0.1/auth/callback?state=abc&code=x'),'abc')).toThrow();
 });
 it('rejects changed returning registrations and denied authorization',()=>{
   expect(()=>validateCallback(new URL('http://127.0.0.1/auth/callback?state=abc&code=x&client_id=oaiapp_2'),'abc','oaiapp_1')).toThrow();
   expect(()=>validateCallback(new URL('http://127.0.0.1/auth/callback?state=abc&error=access_denied'),'abc')).toThrow();
   expect(equalState('','long')).toBe(false);
 });
});
