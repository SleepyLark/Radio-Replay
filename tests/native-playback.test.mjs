import {test} from 'node:test';
import assert from 'node:assert/strict';
import {NativePlayback} from '../ui/playback/native.ts';

const flush=()=>new Promise(resolve=>setImmediate(resolve));
const station=key=>({key,station:key,mode:'longset',tracks:[],crossfade:0});
const track=path=>({path,name:path,size:100,modified:1,duration:10});
function setup(prepare=async key=>({tracks:[track(`${key}-a`),track(`${key}-b`)],problems:[]})) {
  let event;
  let state={generation:0,sequence:0,index:0,path:'',seconds:0,duration:0,playing:false,loading:false,volume:.8,muted:false,error:'',errorPath:'',issueRevision:0};
  const calls=[];
  const rpc=async(command,args={})=>{
    calls.push({command,args});
    if(command==='native_prepare')return prepare(args.key);
    if(command==='remember_duration')return;
    if(command==='native_begin')state={...state,generation:state.generation+1,loading:true,playing:false,path:'',issueRevision:0,error:''};
    if(command==='native_load'){
      assert.equal(args.generation,state.generation);
      state={...state,index:args.index,path:args.paths[args.index],seconds:args.seconds,duration:10,playing:true,loading:false};
    }
    if(command==='native_control'){
      const {control}=args;
      if(control.type==='stop'&&(args.generation===undefined||args.generation===state.generation))state={...state,generation:state.generation+1,playing:false,loading:false,path:''};
      if(control.type==='pause')state={...state,playing:false};
      if(control.type==='play')state={...state,playing:true};
      if(control.type==='mute')state={...state,muted:!state.muted};
      if(control.type==='volume')state={...state,volume:control.value,muted:false};
    }
    state={...state,sequence:state.sequence+1};
    return state;
  };
  const player=new NativePlayback({onState(){},invoke:rpc,listen:async callback=>{event=callback;return ()=>{};},now:()=>0,random:()=>0});
  return {player,calls,emit:patch=>{state={...state,...patch,sequence:state.sequence+1};event(state);},snapshot:()=>state};
}

test('native controller uses duration/prepared IPC and sends playback controls without browser audio',async()=>{
  const Audio=globalThis.Audio,AudioContext=globalThis.AudioContext;
  globalThis.Audio=class {constructor(){assert.fail('Native playback created browser audio');}};
  globalThis.AudioContext=class {constructor(){assert.fail('Native playback created a browser context');}};
  const {player,calls}=setup();
  try {
    await player.tune(station('radio'));
    assert.equal(player.state.engine,'native');
    assert.equal(player.state.path,'radio-a');
    assert.equal(player.state.playing,true);
    await player.pause();assert.equal(player.state.playing,false);
    await player.play();assert.equal(player.state.playing,true);
    player.mute();await flush();assert.equal(player.state.muted,true);
    player.setVolume(.25);await flush();assert.equal(player.state.muted,false);assert.equal(player.state.volume,.25);
    assert.ok(calls.some(call=>call.command==='native_load'));
    assert.ok(!calls.some(call=>call.command==='prepare_station'));
  }finally{player.dispose();globalThis.Audio=Audio;globalThis.AudioContext=AudioContext;}
});

test('a stale native preparation cannot load after a newer station tune',async()=>{
  let finishOld;
  const {player,calls}=setup(key=>key==='old'?new Promise(resolve=>{finishOld=resolve;}):Promise.resolve({tracks:[track('new')],problems:[]}));
  try {
    const old=player.tune(station('old'));await flush();
    await player.tune(station('new'));
    finishOld({tracks:[track('old')],problems:[]});await old;
    assert.equal(player.state.key,'new');
    assert.deepEqual(calls.filter(call=>call.command==='native_load').map(call=>call.args.key),['new']);
  }finally{player.dispose();}
});

test('old-generation native events cannot replace current playback or revive a stopped player',async()=>{
  const {player,emit,snapshot}=setup();
  try {
    await player.tune(station('first'));const old=snapshot().generation;
    await player.tune(station('second'));const current=snapshot().generation;
    emit({generation:old,path:'old',playing:true});assert.equal(player.state.key,'second');assert.equal(player.state.path,'second-a');
    emit({generation:current});player.stop();await flush();
    emit({generation:current,path:'old',playing:true});assert.equal(player.state.key,'');assert.equal(player.state.playing,false);
  }finally{player.dispose();}
});

test('native file notices remain stable across progress events',async()=>{
  const {player,emit}=setup(async()=>({tracks:[track('good')],problems:[{path:'missing',name:'Missing',reason:'File not found',kind:'file'}]}));
  try {
    await player.tune({...station('radio'),crossfade:5000});
    const notice=player.state.notice;
    assert.equal(notice.problems[0].path,'missing');
    emit({seconds:1});emit({seconds:2});
    assert.equal(player.state.notice,notice);
  }finally{player.dispose();}
});

test('native tuning sends the minimum delay and clamps fades to half the shortest recording',async()=>{
  const {player,calls}=setup(async()=>({tracks:[track('long'),{...track('short'),duration:4}],problems:[]}));
  try {
    await player.tune({...station('radio'),crossfade:5000},{tuningMinMs:750,tuningMaxMs:750});
    assert.equal(calls.find(call=>call.command==='native_begin').args.delayMs,750);
    assert.equal(calls.find(call=>call.command==='native_load').args.fade,2);
    assert.equal(player.state.notice,null);
    await player.tune({...station('radio'),crossfade:0},{tuningTransition:false});
    assert.equal(calls.filter(call=>call.command==='native_begin').at(-1).args.delayMs,0);
    assert.equal(calls.filter(call=>call.command==='native_load').at(-1).args.fade,0);
  }finally{player.dispose();}
});

test('native bumper catalogue stays separate from music clock and exposes the active clip',async()=>{
  const {player,calls,emit}=setup(async()=>({tracks:[track('song-a'),track('song-b')],bumpers:[{...track('clip'),duration:1}],problems:[]}));
  try {
    await player.tune({...station('radio'),mode:'tracks',crossfade:5000,bumpers:{enabled:true,folder:'clips',method:'songs',min:2,max:5}});
    const plan=calls.find(call=>call.command==='native_load').args;
    assert.deepEqual(new Set(plan.paths.slice(0,2)),new Set(['song-a','song-b']));
    assert.equal(plan.paths[2],'clip');
    assert.equal(plan.bumperCount,1);
    assert.equal(plan.fade,5,'A short bumper must not reduce music-to-music fades');
    emit({index:2,path:'clip',seconds:.4,duration:1,bumper:true});
    assert.equal(player.state.title,'Bumper');assert.equal(player.state.bumper,true);
    emit({index:1,path:plan.paths[1],seconds:0,duration:10,bumper:false});
    assert.equal(player.state.title,plan.paths[1]);assert.equal(player.state.bumper,false);
    player.stop();assert.equal(player.state.bumper,false);
  }finally{player.dispose();}
});

test('continuous broadcasts never load a bumper catalogue',async()=>{
  const {player,calls}=setup(async()=>({tracks:[track('broadcast')],bumpers:[track('clip')],problems:[]}));
  try {
    await player.tune(station('radio'));
    const plan=calls.find(call=>call.command==='native_load').args;
    assert.deepEqual(plan.paths,['broadcast']);assert.equal(plan.bumperCount,0);
  }finally{player.dispose();}
});
