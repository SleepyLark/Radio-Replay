import {test} from 'node:test';
import assert from 'node:assert/strict';
import {DesktopPublisher} from '../ui/desktop-publisher.ts';

const snapshot=(patch={})=>({station:'Station',trackId:'a',title:'Song',artist:'',album:'',art:'',banner:'',bannerColor:'',accent:'#c7f464',playing:true,hasStation:true,canNavigate:true,seconds:0,duration:180,volume:.8,muted:false,...patch});

test('clock IPC is limited to one per second; controls, metadata and force flush immediately',async()=>{
  let now=0;const sent=[];
  const publisher=new DesktopPublisher(async value=>{sent.push(value);},()=>now);
  await publisher.publish(snapshot());
  for(let i=1;i<=50;i++){now=i*20;await publisher.publish(snapshot({seconds:now/1000}));}
  assert.equal(sent.length,2);
  now=1001;
  await publisher.publish(snapshot({seconds:1,playing:false}));
  assert.equal(sent.length,3);
  for(let i=0;i<60;i++){now+=1000;await publisher.publish(snapshot({seconds:1,playing:false}));}
  assert.equal(sent.length,3,'Settled pause should not publish');
  await publisher.publish(snapshot({seconds:1,playing:false,title:'New title'}));
  await publisher.publish(snapshot({seconds:1,playing:false,title:'New title',volume:.3}));
  assert.equal(sent.length,5);
  await publisher.publish(sent.at(-1),true);
  assert.equal(sent.length,6,'Opening mini can force a current snapshot');
  publisher.dispose();
});

test('slow IPC coalesces queued snapshots and awaits the latest control',async()=>{
  const sent=[];let release;
  const publisher=new DesktopPublisher(value=>{sent.push(value);return sent.length===1?new Promise(resolve=>release=resolve):Promise.resolve();});
  const first=publisher.publish(snapshot());
  await Promise.resolve();
  publisher.publish(snapshot({seconds:1,volume:.4}));
  const paused=publisher.publish(snapshot({seconds:2,volume:.4,playing:false}),true);
  assert.equal(sent.length,1);
  release();await first;await paused;
  assert.equal(sent.length,2);
  assert.equal(sent[1].playing,false);
  assert.equal(sent[1].volume,.4);
  assert.equal(sent[1].seconds,2);
  publisher.dispose();
});

test('failed IPC retries the latest state and disposal cancels retries',async t=>{
  t.mock.timers.enable({apis:['setTimeout']});
  const sent=[];let fail=true;
  const publisher=new DesktopPublisher(async value=>{sent.push(value);if(fail)throw Error('Transient IPC error');});
  await assert.rejects(publisher.publish(snapshot()),/Transient/);
  fail=false;t.mock.timers.tick(1000);
  await publisher.publish(snapshot({playing:false}));
  assert.equal(sent.at(-1).playing,false);
  const count=sent.length;
  publisher.dispose();t.mock.timers.tick(10000);
  await publisher.publish(snapshot());
  assert.equal(sent.length,count);
});

test('a newer control is still delivered when the preceding request fails',async()=>{
  const sent=[];let reject;
  const publisher=new DesktopPublisher(value=>{sent.push(value);return sent.length===1?new Promise((_,fail)=>reject=fail):Promise.resolve();});
  const first=publisher.publish(snapshot());await Promise.resolve();
  const latest=publisher.publish(snapshot({playing:false}));
  reject(Error('Old request failed'));await first;await latest;
  assert.equal(sent.length,2);assert.equal(sent[1].playing,false);
  publisher.dispose();
});
