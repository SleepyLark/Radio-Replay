import {test} from 'node:test';
import assert from 'node:assert/strict';
import {connectMediaSession,mediaKeyAction} from '../ui/media-session.ts';

test('focused media keys map in either key or code without repeating',()=>{
  assert.deepEqual(mediaKeyAction({key:'MediaPlayPause',repeat:false}),{type:'toggle'});
  assert.deepEqual(mediaKeyAction({key:'Unidentified',code:'MediaTrackNext',repeat:false}),{type:'next'});
  assert.deepEqual(mediaKeyAction({key:'MediaTrackPrevious',repeat:false}),{type:'previous'});
  assert.deepEqual(mediaKeyAction({key:'MediaStop',repeat:false}),{type:'pause'});
  assert.equal(mediaKeyAction({key:'MediaPlayPause',repeat:true}),undefined);
  assert.equal(mediaKeyAction({key:'a',repeat:false}),undefined);
});

test('hardware media actions control the engine, with idempotent play and pause',()=>{
  const handlers=new Map(), actions=[];
  const session={setActionHandler:(key,handler)=>handlers.set(key,handler)};
  const media=connectMediaSession(action=>actions.push(action.type),session);
  for(const key of ['pause','play','nexttrack','previoustrack','stop'])handlers.get(key)();
  assert.deepEqual(actions,['pause','play','next','previous','pause']);
  assert.equal(handlers.has('seekto'),false);
  media.dispose();
  assert.ok([...handlers.values()].every(handler=>handler===null));
  assert.equal(session.playbackState,'none');
});

test('media routing tolerates platforms without Media Session support',()=>{
  const media=connectMediaSession(()=>assert.fail('unexpected action'),null);
  media.update({hasStation:false});
  media.dispose();
});
