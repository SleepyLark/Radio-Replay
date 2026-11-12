<script lang="ts">
  import {onMount} from 'svelte';
  import ControlIcon from './ControlIcon.svelte';
  import {mediaKeyAction} from './media-session';
  import VolumeControl from './VolumeControl.svelte';
  import {formatTime} from './playback/format';
  import {getCurrentWindow} from '@tauri-apps/api/window';
  import {getDesktopSnapshot,initialSnapshot,listenMini,restoreMain,sendDesktopAction,type DesktopAction} from './desktop';
  let snapshot=$state(initialSnapshot());let error=$state('');
  const accent=$derived(/^#[0-9a-f]{6}$/i.test(snapshot.accent)?snapshot.accent:'#c7f464');
  const bannerColor=$derived(/^#[0-9a-f]{6}$/i.test(snapshot.bannerColor)?snapshot.bannerColor:'#101b17');
  let failedBanner=$state('');
  let lastBanner='';
  $effect(()=>{if(snapshot.banner!==lastBanner){lastBanner=snapshot.banner;failedBanner='';}});
  async function action(command:DesktopAction){try{await sendDesktopAction(command);error='';}catch(e){error=String(e);}}
  async function restore(){try{await restoreMain();}catch(e){error=String(e);}}
  onMount(()=>{
    let disposed=false;let unlisten:(()=>void)|undefined;
    void listenMini(value=>{snapshot=value;}).then(async stop=>{if(disposed){stop();return;}unlisten=stop;const latest=await getDesktopSnapshot();if(!disposed)snapshot=latest;}).catch(e=>error=String(e));
    return()=>{disposed=true;unlisten?.();};
  });
</script>
<svelte:window onkeydown={event=>{const mediaAction=mediaKeyAction(event);if(mediaAction){event.preventDefault();void action(mediaAction);return;}if(event.target instanceof HTMLElement&&event.target.closest('input,button'))return;if(event.code==='Space'){event.preventDefault();void action({type:'toggle'});}if(event.key==='Escape')void restore();if(event.key==='ArrowLeft')void action({type:'previous'});if(event.key==='ArrowRight')void action({type:'next'});}}/>
<div class="mini-player" style={`--accent:${accent};--green:${accent};background-color:${bannerColor}`}>
  {#if snapshot.banner&&failedBanner!==snapshot.banner}{#key snapshot.banner}<img class="mini-banner" src={snapshot.banner} alt="" onerror={event=>failedBanner=event.currentTarget.getAttribute('src')??''}/>{/key}{/if}
  <header class="mini-drag" role="toolbar" tabindex="0" aria-label="Mini player window" onpointerdown={event=>{if(event.button===0&&!(event.target instanceof Element&&event.target.closest('button')))void getCurrentWindow().startDragging().catch(e=>error=String(e));}}><span>RADIO REPLAY</span><button aria-label="Restore main window" title="Restore main window" onclick={restore}><ControlIcon name="pip-exit"/></button></header>
  <div class="mini-content"><div class="mini-art">{#if snapshot.art}<img src={snapshot.art} alt=""/>{:else}◉{/if}</div><div class="mini-titles"><strong>{snapshot.station||'Nothing tuned yet'}</strong><small>{snapshot.title||'Choose a station in the main window'}</small><small>{snapshot.album}</small></div></div>
  <div class="mini-controls"><div class="mini-transport"><button aria-label="Previous station" disabled={!snapshot.canNavigate} onclick={()=>action({type:'previous'})}><ControlIcon name="previous"/></button><button aria-label={snapshot.playing?'Pause':'Play'} disabled={!snapshot.hasStation} onclick={()=>action({type:'toggle'})}><ControlIcon name={snapshot.playing?'pause':'play'}/></button><button aria-label="Next station" disabled={!snapshot.canNavigate} onclick={()=>action({type:'next'})}><ControlIcon name="next"/></button></div><VolumeControl volume={snapshot.volume} muted={snapshot.muted} onmute={()=>action({type:'mute'})} onvolume={value=>action({type:'volume',value})}/></div>
  <div class="mini-timeline"><progress aria-label="Recording progress" max="1" value={snapshot.duration?Math.max(0,Math.min(1,snapshot.seconds/snapshot.duration)):0}></progress><output aria-label="Playback time">{formatTime(snapshot.seconds)} / {formatTime(snapshot.duration)}</output></div>
  {#if error}<small class="mini-error" role="alert">{error}</small>{/if}
</div>
