import {test} from 'node:test';
import assert from 'node:assert/strict';
import {NativePlayback} from '../ui/playback/native.ts';
import {createHash} from 'node:crypto';

const flush=()=>new Promise(resolve=>setImmediate(resolve));
const station=key=>({key,station:key,mode:'longset',tracks:[],crossfade:0});
const track=path=>({path,name:path,size:100,modified:1,duration:10});
function setup(prepare=async key=>({tracks:[track(`${key}-a`),track(`${key}-b`)],problems:[]}),random=()=>0) {
  let event,progress;
  let state={generation:0,sequence:0,index:0,path:'',seconds:0,duration:0,playing:false,loading:false,volume:.8,muted:false,error:'',errorPath:'',issueRevision:0};
  const calls=[];
  let prepared=new Map();
  const rpc=async(command,args={})=>{
    calls.push({command,args});
    if(command==='native_prepare'){
      const value=await prepare(args.key);let slot=0;const next=new Map();
      const compact=track=>{const index=track.slot??slot++;next.set(index,track);return {slot:index,id:createHash('sha256').update(track.path).digest('hex').slice(0,16),name:track.name,duration:track.duration};};
      const result={revision:createHash('sha256').update(JSON.stringify([value.tracks.map(t=>[t.path,t.size,t.modified]),(value.bumpers??[]).map(t=>[t.path,t.size,t.modified])])).digest('hex'),tracks:value.tracks.map(compact),bumpers:(value.bumpers??[]).map(compact),problems:value.problems};
      prepared=next;return result;
    }
    if(command==='remember_duration')return;
    if(command==='native_begin')state={...state,generation:state.generation+1,loading:true,playing:false,path:'',issueRevision:0,error:''};
    if(command==='native_load'){
      assert.equal(args.generation,state.generation);
      assert.equal(args.paths,undefined,'Native load must not send paths');
      state={...state,index:args.index,path:prepared.get(args.slots[args.index]).path,seconds:args.seconds,duration:10,playing:true,loading:false};
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
  const player=new NativePlayback({onState(){},invoke:rpc,listen:async callback=>{event=callback;return ()=>{};},listenProgress:async callback=>{progress=callback;return ()=>{};},now:()=>0,random});
  return {player,calls,emit:patch=>{state={...state,...patch,sequence:state.sequence+1};event(state);},progress:value=>progress(value),snapshot:()=>state};
}

test('preparation progress belongs to the active tune and clears on completion, stop and failure',async()=>{
  let finish;
  const {player,progress,snapshot}=setup(async()=>new Promise(resolve=>finish=resolve));
  try {
    let tuning=player.tune(station('first'));await flush();
    const first=snapshot().generation;
    progress({generation:first,done:3,total:10,phase:'measuring'});
    assert.deepEqual(player.state.preparation,{done:3,total:10,phase:'measuring'});
    progress({generation:first-1,done:9,total:10,phase:'measuring'});
    progress({generation:first,done:11,total:10,phase:'measuring'});
    assert.equal(player.state.preparation.done,3);
    progress({generation:first,done:0,total:2,phase:'probing'});
    assert.equal(player.state.preparation.phase,'probing');
    finish({tracks:[track('first')],problems:[]});await tuning;
    assert.equal(player.state.preparation,undefined);
    progress({generation:first,done:2,total:2,phase:'probing'});
    assert.equal(player.state.preparation,undefined);
    tuning=player.tune(station('second'));await flush();
    const second=snapshot().generation;
    progress({generation:first,done:1,total:10,phase:'measuring'});
    assert.equal(player.state.preparation,undefined);
    progress({generation:second,done:1,total:10,phase:'measuring'});
    player.stop();assert.equal(player.state.preparation,undefined);
    progress({generation:second,done:2,total:10,phase:'measuring'});
    assert.equal(player.state.preparation,undefined);
    finish({tracks:[track('second')],problems:[]});await tuning;
    tuning=player.tune(station('empty'));await flush();
    progress({generation:snapshot().generation,done:1,total:1,phase:'measuring'});
    finish({tracks:[],problems:[]});await tuning;
    assert.equal(player.state.loading,false);assert.equal(player.state.preparation,undefined);
  }finally{player.dispose();}
});

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

test('retained station clocks use the latest preparation slots and names',async()=>{
  let second=false;
  const {player,calls}=setup(async()=>({tracks:[{...track('a'),slot:second?0:10,name:second?'Updated name':'Old name'},{...track('b'),slot:second?1:11}],problems:[]}));
  try {
    await player.tune(station('radio'));second=true;await player.tune(station('radio'));
    const plan=calls.filter(c=>c.command==='native_load').at(-1).args;
    assert.deepEqual(plan.slots,[0,1]);assert.equal(player.state.title,'Updated name');
    assert.equal(calls.filter(c=>c.command==='remember_duration').length,0,'Prepared native durations must not be rewritten from playback events');
  }finally{player.dispose();}
});
test('changed lineup revisions rebuild clocks and native progress never rewrites prepared durations',async()=>{
  let modified=1,random=0;
  const {player,calls,emit}=setup(async()=>({tracks:[{...track('a'),modified},{...track('b'),modified}],problems:[]}),()=>random);
  try {
    await player.tune(station('radio'));
    modified=2;random=.5;await player.tune(station('radio'));
    const plans=()=>calls.filter(c=>c.command==='native_load').map(c=>c.args);
    assert.equal(plans()[0].index,0);assert.equal(plans()[1].index,1);
    random=.9;await player.tune(station('radio'));assert.equal(plans()[2].index,1,'An unchanged revision must retain its clock');
    emit({index:0,path:'a',seconds:1,duration:10.2});emit({index:1,path:'b',seconds:2,duration:9.8});
    assert.equal(player.state.duration,9.8);
    assert.equal(calls.filter(c=>c.command==='remember_duration').length,0);
  }finally{player.dispose();}
});

test('device errors do not become removable file problems and clear after retry',async()=>{
  const {player,emit}=setup();
  try {
    await player.tune(station('radio'));
    emit({playing:false,loading:false,errorPath:'',error:'Audio output unavailable. Check your sound device, then press Play.',issueRevision:1});
    assert.equal(player.state.playing,false);
    assert.equal(player.state.notice.severity,'error');
    assert.deepEqual(player.state.notice.problems,[]);
    assert.equal(player.state.path,'radio-a');
    await player.play();
    emit({error:'',errorPath:'',issueRevision:2});
    assert.equal(player.state.notice,null);
    assert.equal(player.state.error,'');
  }finally{player.dispose();}
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
    assert.deepEqual(new Set(plan.slots.slice(0,2)),new Set([0,1]));
    assert.equal(plan.slots[2],2);
    assert.equal(plan.bumperCount,1);
    assert.equal(plan.fade,5,'A short bumper must not reduce music-to-music fades');
    emit({index:2,path:'clip',seconds:.4,duration:1,bumper:true});
    assert.equal(player.state.title,'Bumper');assert.equal(player.state.bumper,true);
    const name=['song-a','song-b'][plan.slots[1]];
    emit({index:1,path:name,seconds:0,duration:10,bumper:false});
    assert.equal(player.state.title,name);assert.equal(player.state.bumper,false);
    player.stop();assert.equal(player.state.bumper,false);
  }finally{player.dispose();}
});

test('continuous broadcasts never load a bumper catalogue',async()=>{
  const {player,calls}=setup(async()=>({tracks:[track('broadcast')],bumpers:[track('clip')],problems:[]}));
  try {
    await player.tune(station('radio'));
    const plan=calls.find(call=>call.command==='native_load').args;
    assert.deepEqual(plan.slots,[0]);assert.equal(plan.bumperCount,0);
  }finally{player.dispose();}
});

test('network stations shuffle URLs without probing, virtual seeking, fades or duration writes',async()=>{
  const urls=['https://radio.example.org/song.mp3','https://radio.example.org/live'];
  const {player,calls,emit}=setup(async()=>({tracks:urls.map(path=>({...track(path),duration:null,source:'network'})),problems:[]}));
  try{
    await player.tune({...station('radio'),mode:'network',crossfade:5000});
    const plan=calls.find(c=>c.command==='native_load').args;
    assert.deepEqual(plan.slots,[1,0]);assert.equal(plan.index,0);assert.equal(plan.seconds,0);assert.equal(plan.fade,0);assert.equal(plan.bumperCount,0);
    assert.equal(player.state.network,true);emit({index:1,path:urls[plan.slots[1]],duration:0,seconds:5,playing:true});assert.equal(player.state.path,urls[plan.slots[1]]);assert.equal(player.state.title,urls[plan.slots[1]]);
    assert.equal(calls.filter(c=>c.command==='remember_duration').length,0);
  }finally{player.dispose();}
});

test('network retuning draws a fresh order without modifying the saved playlist',async()=>{
  const tracks=['https://radio.example.org/a','https://radio.example.org/b','https://radio.example.org/c'].map(path=>({...track(path),duration:null}));
  const original=structuredClone(tracks);let random=0;
  const {player,calls}=setup(async()=>({tracks,problems:[]}),()=>random);
  try{
    const radio={...station('radio'),mode:'network'};
    await player.tune(radio,{tuningTransition:false});
    random=.999;
    await player.tune(radio,{tuningTransition:false});
    const plans=calls.filter(c=>c.command==='native_load').map(c=>c.args);
    assert.equal(plans.length,2);assert.notDeepEqual(plans[0].slots,plans[1].slots);
    for(const plan of plans){assert.deepEqual(new Set(plan.slots),new Set([0,1,2]));assert.equal(plan.index,0);assert.equal(plan.seconds,0);}
    assert.deepEqual(tracks,original);
  }finally{player.dispose();}
});
