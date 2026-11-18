import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// Node 24 provides dirname; keep Node-only types out of the renderer tsconfig.
const directory=(import.meta as ImportMeta & {dirname:string}).dirname;
export default defineConfig(({mode})=>({
  plugins:[svelte()],clearScreen:false,
  resolve:{alias:mode==='native'?[{find:'./playback/web',replacement:directory+'/ui/playback/web-disabled.ts'}]:[]},
}));
