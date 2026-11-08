import type {DesktopAction, DesktopSnapshot} from './desktop';

export function mediaKeyAction(event:Pick<KeyboardEvent,'key'|'code'|'repeat'>):DesktopAction|undefined {
  if(event.repeat)return;
  const actions:Record<string,DesktopAction>={MediaPlayPause:{type:'toggle'},MediaPlay:{type:'play'},MediaPause:{type:'pause'},MediaStop:{type:'pause'},MediaTrackNext:{type:'next'},MediaTrackPrevious:{type:'previous'}};
  return actions[event.key]??actions[event.code];
}

// WebView2 can receive the hardware key instead of the native SMTC session.
// Both routes must control the engine, rather than one of its pooled audio tags.
export function connectMediaSession(dispatch:(action:DesktopAction)=>void, session:MediaSession|undefined=navigator.mediaSession) {
  const actions:Partial<Record<MediaSessionAction,DesktopAction>>={
    play:{type:'play'}, pause:{type:'pause'}, stop:{type:'pause'},
    previoustrack:{type:'previous'}, nexttrack:{type:'next'},
  };
  for(const [name,action] of Object.entries(actions)) {
    try {session?.setActionHandler(name as MediaSessionAction,()=>dispatch(action));} catch {/* Unsupported action on this platform. */}
  }
  let identity='';
  return {
    update(state:DesktopSnapshot) {
      if(!session)return;
      session.playbackState=!state.hasStation?'none':state.playing?'playing':'paused';
      const key=JSON.stringify([state.title,state.artist,state.album,state.station,state.art,state.hasStation]);
      if(key!==identity){
        identity=key;
        session.metadata=state.hasStation?new MediaMetadata({title:state.title||state.station,artist:state.artist||state.station,album:state.album,artwork:state.art?[{src:state.art}]:[]}):null;
      }
      // Broadcast time belongs to the station clock; don't advertise seeking.
      try {if(state.hasStation&&state.duration>0)session.setPositionState({duration:state.duration,position:Math.min(state.duration,Math.max(0,state.seconds)),playbackRate:1});else session.setPositionState();} catch {/* Older WebView implementations may not expose timeline updates. */}
    },
    dispose(){for(const name of Object.keys(actions)){try{session?.setActionHandler(name as MediaSessionAction,null);}catch{}}if(session){session.metadata=null;session.playbackState='none';}},
  };
}
