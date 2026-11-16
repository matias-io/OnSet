import {defineConfig} from 'vite';
import react from '@vitejs/plugin-react';
// The dev server's React refresh preamble is an inline script; the strict CSP stays in every production build.
const devCsp={name:'dev-csp',apply:'serve' as const,transformIndexHtml:(html:string)=>html.replace(/<meta http-equiv="Content-Security-Policy"[^>]*>/,'')};
// The brand fonts live in the repository's shared assets/ folder, outside this one.
export default defineConfig({plugins: [react(), devCsp], base: './', server: {fs: {allow: ['..']}}, build: {outDir: 'dist', rollupOptions: {input: {main: 'index.html', live: 'live.html'}}}, test: {include: ['src/*.test.ts']}} as any);
