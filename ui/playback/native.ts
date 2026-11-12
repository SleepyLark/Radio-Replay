import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {Station,PreparedStation,TrackProblem} from '../library';
import type {PlaybackState,PlaybackNotice} from './engine';
import {clockFor,fadeSeconds,position,shuffle,type Clock} from './timeline.ts';
import {tuningMilliseconds} from './tuning.ts';

export interface NativeSnapshot {
  generation:number;sequence:number;index:number;path:string;seconds:number;duration:number;
  playing:boolean;loading:boolean;volume:number;muted:boolean;error:string;errorPath:string;issueRevision:number;
  tuning?:boolean;tuningPlaying?:boolean;nextPath?:string;transitioning?:boolean;transitionProgress?:number;
}
type Invoke=<T>(command:string,args?:Record<string,unknown>)=>Promise<T>;
interface Options {
  onState:(state:PlaybackState)=>void;
  invoke?:Invoke;
  listen?:(callback:(state:NativeSnapshot)=>void)=>Promise<()=>void>;
  now?:()=>number;
  random?:()=>number;
}

/** UI/clock adapter only. No Audio elements, AudioContext, decoding or media URLs. */
export class NativePlayback {
  state:PlaybackState={key:'',station:'',title:'',path:'',playing:false,loading:false,seconds:0,duration:0,volume:.8,muted:false,error:'',notice:null,engine:'native'};
  private rpc:Invoke;
  private ready:Promise<void>;
  private unlisten?:()=>void;
  private disposed=false;
  private token=0;
  private generation=0;
  private sequence=0;
  private issueRevision=0;
  private station:Station|null=null;
  private clock:Clock|null=null;
  private clocks=new Map<string,{signature:string;clock:Clock}>();
  private problems=new Map<string,TrackProblem>();
  private measured=new Set<string>();
  private nextNotice=0;
  private lastPath='';
  private now:()=>number;
  private random:()=>number;
  private options:Options;
  constructor(options:Options) {
    this.options=options;
    this.rpc=options.invoke??invoke;
    this.now=options.now??(()=>performance.now());
    this.random=options.random??Math.random;
    this.ready=(options.listen??(callback=>listen<NativeSnapshot>('native-audio-state',event=>callback(event.payload))))(state=>this.apply(state)).then(async stop=>{
      if(this.disposed){stop();return;}
      this.unlisten=stop;
      // A dev reload must not leave an unrepresented previous audio session playing.
      const state=await this.rpc<NativeSnapshot>('native_control',{control:{type:'stop'}});
      if(!this.disposed){this.generation=state.generation;this.apply(state);}
    });
    void this.ready.catch(error=>{if(!this.disposed)this.failed(error);});
  }
  private emit(update:Partial<PlaybackState>={}){this.state={...this.state,...update};this.options.onState({...this.state});}
  private notice(message:string,severity:'warning'|'error'='warning') {
    const problems=[...this.problems.values()];
    const parts=[message,problems.length?`${problems.length} file or cache problem${problems.length===1?'':'s'} reported.`:''].filter(Boolean);
    const text=parts.join(' ');
    const notice:PlaybackNotice|null=text?{id:++this.nextNotice,message:text,stationKey:this.station?.key??'',station:this.station?.station??'',problems,severity}:null;
    this.emit({error:text,notice});
  }
  private failed(error:unknown){this.emit({playing:false,loading:false});this.notice(String(error),'error');}
  private apply(value:NativeSnapshot) {
    if(this.disposed||value.generation!==this.generation||value.sequence<this.sequence)return;
    this.sequence=value.sequence;
    const track=this.clock?.order[value.index];
    this.emit({key:this.station?.key??'',station:this.station?.station??'',title:track?.name??(this.station?'Tuning…':''),path:value.path,playing:value.playing,loading:value.loading,seconds:value.seconds,duration:value.duration,volume:value.volume,muted:value.muted});
    if(value.path&&value.path!==this.lastPath&&this.clock&&!value.loading){
      this.lastPath=value.path;
      this.clock.origin=this.now();
      this.clock.offset=this.clock.steps.slice(0,value.index).reduce((a,b)=>a+b,0)+value.seconds;
    }
    if(value.issueRevision!==this.issueRevision){
      this.issueRevision=value.issueRevision;
      if(value.error){
        const failedTrack=this.clock?.order.find(track=>track.path===value.errorPath);
        this.problems.set(`file:${value.errorPath}`,{path:value.errorPath,name:failedTrack?.name??value.errorPath,reason:value.error,kind:'file'});
        this.notice(value.error,!value.loading&&!value.playing?'error':'warning');
      }
    }
    if(track&&value.path===track.path&&value.duration>0&&!value.loading){
      const signature=`${track.path}:${track.size}:${track.modified}`;
      if(!this.measured.has(signature)){
        this.measured.add(signature);
        if(this.measured.size>20000)this.measured.delete(this.measured.keys().next().value!);
        const token=this.token;
        void this.rpc('remember_duration',{key:this.station!.key,path:track.path,size:track.size,modified:track.modified,duration:value.duration}).catch(error=>{
          if(token!==this.token||this.disposed)return;
          this.problems.set(`cache:${track.path}`,{path:track.path,name:track.name,reason:`Could not save native duration: ${String(error)}`,kind:'cache'});
          this.notice('Playback can continue, but a duration was not cached.');
        });
      }
    }
  }
  async tune(station:Station,settings:Record<string,unknown>={},group='all') {
    const token=++this.token;
    this.station=station;this.clock=null;this.lastPath='';this.sequence=0;this.issueRevision=0;this.problems.clear();
    this.emit({key:station.key,station:station.station,title:'Tuning…',path:'',playing:false,loading:true,error:'',notice:null,seconds:0,duration:0});
    try {
      await this.ready;if(token!==this.token||this.disposed)return;
      const started=await this.rpc<NativeSnapshot>('native_begin',{delayMs:tuningMilliseconds(settings,this.random)});
      if(token!==this.token||this.disposed)return;
      this.generation=started.generation;this.sequence=0;this.apply(started);
      const prepared=await this.rpc<PreparedStation>('native_prepare',{generation:this.generation,key:station.key,group});
      if(token!==this.token||this.disposed)return;
      for(const problem of prepared.problems)this.problems.set(`${problem.kind}:${problem.path}`,problem);
      const tracks=prepared.tracks.filter(track=>typeof track.duration==='number'&&track.duration>0);
      if(!tracks.length)throw new Error('No playable tracks in this station.');
      const requestedFade=fadeSeconds(station,settings);
      const signature=JSON.stringify([tracks.map(track=>[track.path,track.size,track.modified]),station.mode,requestedFade]);
      const key=`${group}:${station.key}`;const existing=this.clocks.get(key);
      const order=existing?.signature===signature?existing.clock.order:station.mode==='longset'?tracks:shuffle(tracks,this.random);
      const durations=new Map(tracks.map(track=>[track.path,track.duration!]));
      this.clock=existing?.signature===signature?existing.clock:clockFor(order,order.map(track=>durations.get(track.path)!),requestedFade,this.now(),this.random);
      this.clocks.delete(key);this.clocks.set(key,{signature,clock:this.clock});
      let retained=[...this.clocks.values()].reduce((sum,value)=>sum+value.clock.order.length,0);
      for(const [id,value] of this.clocks){if(this.clocks.size<=32&&retained<=20000)break;if(id===key)continue;this.clocks.delete(id);retained-=value.clock.order.length;}
      this.notice('');
      const at=position(this.clock,this.now());
      const loaded=await this.rpc<NativeSnapshot>('native_load',{generation:this.generation,key:station.key,paths:order.map(track=>track.path),index:at.index,seconds:at.seconds,fade:this.clock.fade});
      if(token===this.token&&!this.disposed)this.apply(loaded);
    }catch(error){
      if(token===this.token&&!this.disposed){
        try{const stopped=await this.rpc<NativeSnapshot>('native_control',{control:{type:'stop'},generation:this.generation});if(token!==this.token||this.disposed)return;this.generation=stopped.generation;this.apply(stopped);}catch{/* Keep the original tuning error if the runtime is unavailable. */}
        if(token===this.token&&!this.disposed)this.failed(error);
      }
    }
  }
  private async control(control:{type:string;value?:number}) {
    const token=this.token;
    try{await this.ready;if(this.disposed)return;const state=await this.rpc<NativeSnapshot>('native_control',{control});if(token===this.token)this.apply(state);}catch(error){if(token===this.token&&!this.disposed)this.failed(error);}
  }
  async pause(){await this.control({type:'pause'});}
  async play(){if(this.station)await this.control({type:'play'});}
  mute(){void this.control({type:'mute'});}
  setVolume(value:number){if(Number.isFinite(value))void this.control({type:'volume',value:Math.max(0,Math.min(1,value))});}
  stop(){const generation=this.generation;this.token++;this.station=null;this.clock=null;this.problems.clear();this.emit({key:'',station:'',title:'',path:'',playing:false,loading:false,seconds:0,duration:0,error:'',notice:null});void this.ready.then(()=>this.rpc<NativeSnapshot>('native_control',{control:{type:'stop'},generation})).then(state=>{if(!this.station&&!this.disposed){this.generation=state.generation;this.apply(state);}}).catch(()=>{});}
  clearCache(){this.clocks.clear();this.measured.clear();}
  invalidate(){this.stop();this.clearCache();}
  dispose(){this.stop();this.disposed=true;this.unlisten?.();}
}
