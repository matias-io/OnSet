import {defineConfig} from 'vite';
import react from '@vitejs/plugin-react';
// The dev server's React refresh preamble is an inline script; the strict CSP stays in every production build.
const devCsp={name:'dev-csp',apply:'serve' as const,transformIndexHtml:(html:string)=>html.replace(/<meta http-equiv="Content-Security-Policy"[^>]*>/,'')};
export default defineConfig({plugins: [react(), devCsp], base: './', build: {outDir: 'dist', rollupOptions: {input: {main: 'index.html', live: 'live.html'}}}, test: {include: ['desktop/*.test.ts','ui/*.test.ts']}} as any);
