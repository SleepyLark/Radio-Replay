import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const read=file=>readFileSync(new URL(`../${file}`,import.meta.url),'utf8');
test('Windows installers include the native audio DLL and license instead of an audio download step',()=>{
  const resources=JSON.parse(read('src-tauri/tauri.windows.conf.json')).bundle.resources;
  assert.equal(resources['../vendor/mpv/libmpv-2.dll'],'audio/libmpv-2.dll');
  assert.equal(resources['../vendor/mpv/LICENSE.LGPL'],'audio/LICENSE.LGPL');
  assert.equal(resources['../runtime/SOURCES.txt'],'runtime/SOURCES.txt');
  assert.ok(!Object.keys(resources).some(path=>/Install-AudioRuntime/.test(path)));
  assert.doesNotMatch(read('src-tauri/windows-hooks.nsh'),/nsExec|ExecWait|powershell\.exe|Invoke-WebRequest/i);
});
