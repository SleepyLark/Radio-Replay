import {test} from 'node:test';
import assert from 'node:assert/strict';
import {PlaybackSession} from '../ui/playback-session.ts';
import {desktopSnapshot} from '../ui/desktop-snapshot.ts';

const status=backend=>({backend,error:null,warning:null,levels:{volume:.37,muted:true}});
const options=()=>({onState(){},onAction(){},group:()=> 'favorites'});

test('native sessions keep saved levels, avoid web startup and ignore late state after disposal',async()=>{
  const received=[];let emit,disposed=0;
  const engine={dispose(){disposed++;}};
  const session=new PlaybackSession(status('mpv'),{...options(),onState:state=>received.push(state)},{
    native:settings=>{emit=settings.onState;assert.deepEqual(settings.levels,{volume:.37,muted:true});return engine;},
    loadWeb:()=>assert.fail('Native startup loaded Web Audio'),media:()=>assert.fail('Native startup registered browser media'),
  });
  assert.equal(await session.ready,engine);
  emit({playing:true});session.dispose();session.dispose();emit({playing:false});
  assert.equal(disposed,1);
  assert.deepEqual(received,[{playing:true}]);
});

test('disposing a session while its comparison module loads prevents player and media creation',async()=>{
  let loaded;
  const session=new PlaybackSession(status('web'),options(),{
    native:()=>assert.fail(),media:()=>assert.fail('Cancelled startup registered media'),
    loadWeb:()=>new Promise(resolve=>loaded=resolve),
  });
  session.dispose();
  loaded({createWebPlayback:()=>assert.fail('Cancelled startup created audio')});
  assert.equal(await session.ready,null);
});

test('comparison sessions own media routing and clean it up if player construction fails',async()=>{
  const messages=[];let dispatch;let emit;
  const media={update:state=>messages.push(state),dispose:()=>messages.push('media disposed')};
  const opts={...options(),onAction:action=>messages.push(action)};
  const session=new PlaybackSession(status('web'),opts,{
    native:()=>assert.fail(),media:callback=>{dispatch=callback;return media;},
    loadWeb:async()=>({createWebPlayback:(onState,group,levels)=>{
      assert.equal(group(),'favorites');assert.deepEqual(levels,status('web').levels);emit=onState;
      return {dispose:()=>messages.push('player disposed')};
    }}),
  });
  await session.ready;
  dispatch({type:'pause'});session.updateMedia({title:'Music'});session.dispose();
  dispatch({type:'play'});emit({playing:true});session.updateMedia({title:'Late'});
  assert.deepEqual(messages,[{type:'pause'},{title:'Music'},'media disposed','player disposed']);
  const failing=new PlaybackSession(status('web'),options(),{
    native:()=>assert.fail(),media:()=>media,
    loadWeb:async()=>({createWebPlayback:()=>{throw new Error('Audio startup failed');}}),
  });
  await assert.rejects(failing.ready,/Audio startup failed/);
  assert.equal(messages.filter(value=>value==='media disposed').length,2);
});

test('an unavailable runtime never constructs a playback backend',async()=>{
  const session=new PlaybackSession({...status('mpv'),error:'Runtime unavailable'},options(),{
    native:()=>assert.fail(),loadWeb:()=>assert.fail(),media:()=>assert.fail(),
  });
  await assert.rejects(session.ready,/Runtime unavailable/);
  session.dispose();
});

test('media cleanup errors cannot prevent player disposal or hide the startup failure',async()=>{
  let stopped=0;
  const factories={
    native:()=>assert.fail(),media:()=>({dispose(){throw Error('Media cleanup failed');},update(){}}),
    loadWeb:async()=>({createWebPlayback:()=>({dispose(){stopped++;}})}),
  };
  const session=new PlaybackSession(status('web'),options(),factories);
  await session.ready;
  assert.throws(()=>session.dispose(),/Media cleanup failed/);
  session.dispose();assert.equal(stopped,1);
  const failing=new PlaybackSession(status('web'),options(),{...factories,loadWeb:async()=>({createWebPlayback:()=>{throw Error('Original startup failure');}})});
  await assert.rejects(failing.ready,/Original startup failure/);
});

test('desktop snapshots preserve startup, loading, muted zero volume and title selection',()=>{
  const context={playback:null,station:undefined,tags:undefined,textMode:'lyrics',displayTitle:'Timed title',art:'art',banner:'banner',accent:'#abcdef',canNavigate:true};
  const empty=desktopSnapshot(context);
  assert.equal(empty.audioReady,false);assert.equal(empty.hasStation,false);
  assert.equal(empty.volume,.8);assert.equal(empty.title,'');
  const playback={key:'radio',path:'song.mp3',title:'Filename',playing:true,loading:true,volume:0,muted:true,seconds:4,duration:30};
  const station={key:'radio',station:'Radio',tracks:[],mode:'network',bannerColor:42};
  const tags={title:'  Clip name  ',artist:'Artist',album:'Album',duration:30};
  const loading=desktopSnapshot({...context,playback,station,tags});
  assert.equal(loading.trackId,'radio\0song.mp3');assert.equal(loading.audioReady,true);
  assert.equal(loading.hasStation,false);assert.equal(loading.volume,0);assert.equal(loading.muted,true);
  assert.equal(loading.network,true);assert.equal(loading.bannerColor,'');
  assert.equal(loading.title,'  Clip name  ');
  assert.equal(desktopSnapshot({...context,playback,station,tags,textMode:'titles'}).title,'Timed title');
  const bumper=desktopSnapshot({...context,playback:{...playback,loading:false,bumper:true},station,tags,textMode:'titles'});
  assert.equal(bumper.title,'Bumper · Clip name');assert.equal(bumper.hasStation,true);
  assert.equal(desktopSnapshot({...context,playback:{...playback,bumper:true},station}).title,'Bumper');
});
