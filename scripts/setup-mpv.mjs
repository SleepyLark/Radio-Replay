import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
if(process.platform!=='win32'||process.arch!=='x64')throw new Error('This runtime installer supports Windows x64. Set RADIO_REPLAY_MPV_LIBRARY to a compatible system libmpv on other platforms.');
const manifest=JSON.parse(readFileSync(path.join(root,'runtime/mpv-windows-x64.json'),'utf8'));
const cache=path.join(root,'.mpv-cache');
mkdirSync(cache,{recursive:true});
const archive=path.join(cache,`libmpv-${manifest.version}-lgpl.zip`);
if(!existsSync(archive)) {
  console.log(`Downloading official LGPL libmpv ${manifest.version}…`);
  const response=await fetch(manifest.url);
  if(!response.ok)throw new Error(`Runtime download failed: HTTP ${response.status}`);
  writeFileSync(archive,Buffer.from(await response.arrayBuffer()));
}
const hash=createHash('sha256').update(readFileSync(archive)).digest('hex');
if(hash!==manifest.sha256)throw new Error(`Checksum mismatch: delete ${archive} and retry. No files were extracted.`);
const destination=path.join(root,'vendor/mpv');
const quote=value=>"'"+value.replaceAll("'","''")+"'";
execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',`$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath ${quote(archive)} -DestinationPath ${quote(destination)} -Force`],{stdio:'inherit',windowsHide:true});
if(!existsSync(path.join(destination,manifest.library)))throw new Error('Verified archive did not contain the expected runtime DLL');
console.log(`Verified libmpv ${manifest.version}: ${destination}`);
