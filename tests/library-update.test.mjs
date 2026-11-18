import {test} from 'node:test';
import assert from 'node:assert/strict';
import {applyLibraryUpdate} from '../ui/library-update.ts';
import {saveSettings,saveGroup} from '../ui/library.ts';
const library=()=>({root:'archive',custom:[{key:'a',station:'A',tracks:[{path:'a.wav',name:'A'}],futureStation:true},{key:'b',station:'B',tracks:[]}],groups:[{id:'g',name:'G',members:['a','b'],futureGroup:7}],hidden:['a','other'],logos:{a:'a.png',b:null},settings:{accent:'#123456',futureSetting:7},futureConfig:{kept:true}});
test('settings updates preserve unrelated objects and newer station data',()=>{
  const original=library();const renamed={...original.custom[0],station:'New name'};
  const current=applyLibraryUpdate(original,{stations:[renamed]});
  const next=applyLibraryUpdate(current,{settings:{accent:'#654321',futureSetting:7}});
  assert.equal(next.custom,current.custom);assert.equal(next.custom[0],renamed);assert.equal(next.custom[1],original.custom[1]);
  assert.equal(next.groups,original.groups);assert.equal(next.logos,original.logos);assert.equal(next.hidden,original.hidden);assert.equal(next.futureConfig,original.futureConfig);
  assert.equal(original.custom[0].station,'A');assert.equal(original.settings.accent,'#123456');
});
test('station/group edits merge in place; removal cleans hidden entries and logos without mutating the old library',()=>{
  const original=library();const group={...original.groups[0],members:['b']};
  const next=applyLibraryUpdate(original,{removedStations:['a'],groups:[group],logos:[{key:'b',removed:false,value:null}]});
  assert.deepEqual(next.custom,[original.custom[1]]);assert.deepEqual(next.hidden,['other']);assert.deepEqual(next.logos,{b:null});assert.equal(next.groups[0].futureGroup,7);
  assert.deepEqual(original.hidden,['a','other']);assert.equal(original.logos.a,'a.png');
  const appended={key:'c',station:'C',tracks:[]};
  const more=applyLibraryUpdate(next,{stations:[appended],removedGroups:['g'],groups:[{id:'new',name:'New',members:['c']}],logos:[{key:'b',removed:true,value:null}]});
  assert.deepEqual(more.custom.map(s=>s.key),['b','c']);assert.deepEqual(more.groups.map(g=>g.id),['new']);assert.deepEqual(more.logos,{});
  const unusual=applyLibraryUpdate(original,{logos:[{key:'__proto__',removed:false,value:{kept:true}}]});
  assert.equal(Object.getPrototypeOf(unusual.logos),Object.prototype);
  assert.equal(Object.hasOwn(unusual.logos,'__proto__'),true);assert.deepEqual(unusual.logos.__proto__,{kept:true});
});
test('UI mutation requests are serialized and a rejected save does not block a later group save',async()=>{
  const previous=globalThis.window;const calls=[];let reject;
  globalThis.window={__TAURI_INTERNALS__:{invoke:(command,args)=>{calls.push({command,args});return calls.length===1?new Promise((_,fail)=>reject=fail):Promise.resolve({groups:[]});}}};
  try {
    const first=saveSettings({accent:'#123456'});const second=saveGroup({id:'g',name:'G',members:[]});
    await new Promise(resolve=>setImmediate(resolve));assert.deepEqual(calls.map(c=>c.command),['save_settings']);
    reject(Error('Write failure'));await assert.rejects(first,/Write failure/);await second;
    assert.deepEqual(calls.map(c=>c.command),['save_settings','save_group']);
  }finally{globalThis.window=previous;}
});
