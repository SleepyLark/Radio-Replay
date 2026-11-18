import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const json=file=>JSON.parse(readFileSync(new URL(`../src-tauri/${file}`,import.meta.url),'utf8'));
const directives=csp=>new Map(csp.split(';').map(part=>part.trim().split(/\s+/)).filter(([name])=>name).map(([name,...sources])=>[name,sources]));

test('native windows can listen to state but cannot emit forged events or grant legacy audio access',()=>{
  const main=json('capabilities/main.json'),mini=json('capabilities/mini.json');
  for(const capability of [main,mini]){
    assert.ok(capability.permissions.includes('core:event:allow-listen'));
    assert.ok(capability.permissions.includes('core:event:allow-unlisten'));
    assert.ok(!capability.permissions.includes('core:default'));
    assert.ok(!capability.permissions.includes('core:event:default'));
    assert.ok(!capability.permissions.some(p=>p.startsWith('core:event:allow-emit')));
    assert.ok(!capability.permissions.includes('allow-prepare-station'));
    assert.ok(!capability.permissions.includes('allow-resolve-tuning-sound'));
  }
  assert.ok(mini.permissions.includes('core:window:allow-start-dragging'));
  assert.deepEqual(json('tauri.conf.json').app.security.capabilities,['main-window','mini-player']);
});

test('only the explicit web comparison configuration permits browser audio and asset fetching',()=>{
  const native=directives(json('tauri.conf.json').app.security.csp);
  const web=json('tauri.web.conf.json').app.security;
  for(const policy of [native,directives(web.csp)]){
    for(const directive of ['object-src','base-uri','form-action'])assert.deepEqual(policy.get(directive),["'none'"]);
  }
  assert.deepEqual(native.get('media-src'),["'none'"]);
  assert.deepEqual(native.get('connect-src'),['ipc:','http://ipc.localhost']);
  assert.ok(native.get('img-src').includes('asset:'));
  assert.ok(directives(web.csp).get('media-src').includes('asset:'));
  const legacy=web.capabilities.find(c=>typeof c==='object');
  assert.deepEqual(legacy.windows,['main']);
  assert.deepEqual(legacy.permissions,['allow-prepare-station','allow-resolve-tuning-sound']);
});
