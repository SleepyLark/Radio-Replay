<script lang="ts">
  import {onMount} from 'svelte';
  import {getCurrentWindow} from '@tauri-apps/api/window';
  import {getDesktopSnapshot,initialSnapshot,listenMini,restoreMain,sendDesktopAction,type DesktopAction} from './desktop';
  let snapshot=$state(initialSnapshot());let error=$state('');
  const accent=$derived(/^#[0-9a-f]{6}$/i.test(snapshot.accent)?snapshot.accent:'#c7f464');
  async function action(command:DesktopAction){try{await sendDesktopAction(command);error='';}catch(e){error=String(e);}}
  async function restore(){try{await restoreMain();}catch(e){error=String(e);}}
  onMount(()=>{
    let disposed=false;let unlisten:(()=>void)|undefined;
    void listenMini(value=>snapshot=value).then(async stop=>{if(disposed){stop();return;}unlisten=stop;const latest=await getDesktopSnapshot();if(!disposed)snapshot=latest;}).catch(e=>error=String(e));
    return()=>{disposed=true;unlisten?.();};
  });
</script>
<svelte:window onkeydown={event=>{if(event.target instanceof HTMLElement&&event.target.closest('input,button'))return;if(event.code==='Space'){event.preventDefault();void action({type:'toggle'});}if(event.key==='Escape')void restore();if(event.key==='ArrowLeft')void action({type:'previous'});if(event.key==='ArrowRight')void action({type:'next'});}}/>
<div class="mini-player" style={`--accent:${accent}`}>
  <header class="mini-drag" role="toolbar" tabindex="0" aria-label="Mini player window" onpointerdown={event=>{if(event.button===0&&!(event.target instanceof HTMLElement&&event.target.closest('button')))void getCurrentWindow().startDragging().catch(e=>error=String(e));}}><span>RADIO REPLAY</span><button aria-label="Restore main window" onclick={restore}>↗</button></header>
  <div class="mini-content"><div class="mini-art">{#if snapshot.art}<img src={snapshot.art} alt=""/>{:else}◉{/if}</div><div class="mini-titles"><strong>{snapshot.station||'Nothing tuned yet'}</strong><small>{snapshot.title||'Choose a station in the main window'}</small><small>{snapshot.album}</small></div></div>
  <div class="mini-controls"><button aria-label="Previous station" disabled={!snapshot.canNavigate} onclick={()=>action({type:'previous'})}>|◁</button><button aria-label={snapshot.playing?'Pause':'Play'} disabled={!snapshot.hasStation} onclick={()=>action({type:'toggle'})}>{snapshot.playing?'❚❚':'▷'}</button><button aria-label="Next station" disabled={!snapshot.canNavigate} onclick={()=>action({type:'next'})}>▷|</button><button aria-label={snapshot.muted?'Unmute':'Mute'} onclick={()=>action({type:'mute'})}>{snapshot.muted?'Muted':'Volume'}</button><input aria-label="Volume" type="range" min="0" max="1" step="0.01" value={snapshot.volume} oninput={event=>action({type:'volume',value:Number(event.currentTarget.value)})}/></div>
  <progress aria-label="Recording progress" max="1" value={snapshot.duration?Math.min(1,snapshot.seconds/snapshot.duration):0}></progress>
  {#if error}<small class="mini-error" role="alert">{error}</small>{/if}
</div>
