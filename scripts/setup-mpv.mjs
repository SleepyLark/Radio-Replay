import {mkdirSync,readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.platform!=='win32'||process.arch!=='x64') {
  if(process.argv.includes('--ensure')) {
    console.log('Native audio requires a system libmpv (client API 2), or RADIO_REPLAY_MPV_LIBRARY pointing to it.');
    process.exit(0);
  }
  throw new Error('This runtime setup supports Windows x64. Use a system libmpv on other platforms.');
}
const manifest=JSON.parse(readFileSync(path.join(root,'runtime/mpv-windows-x64.json'),'utf8'));
const cache=path.join(root,'.mpv-cache');
mkdirSync(cache,{recursive:true});
const archive=path.join(cache,`libmpv-${manifest.version}-lgpl.zip`);
execFileSync('powershell.exe',['-NoLogo','-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass',
  '-File',path.join(root,'runtime/Install-AudioRuntime.ps1'),
  '-Destination',path.join(root,'vendor/mpv'),'-ArchivePath',archive],{stdio:'inherit',windowsHide:true});
