<script lang="ts">
  import type { PlaybackState } from './playback/engine';
  let {playback, enabled, showProgress, toggle, step, mute, volume}: {playback: PlaybackState | null; enabled: boolean; showProgress: boolean; toggle: () => void; step: (direction: number) => void; mute: () => void; volume: (value: number) => void} = $props();
  function time(seconds: number) {const n=Math.floor(Math.max(0,seconds));return `${Math.floor(n/60)}:${String(n%60).padStart(2,'0')}`;}
</script>

<div class="radio-controls">
  <div class="transport">
    <button aria-label="Previous station" disabled={!enabled} onclick={()=>step(-1)}>⏮</button>
    <button class="play-button" aria-label={playback?.playing?'Pause':'Play'} disabled={!playback?.key || playback.loading} onclick={toggle}>{playback?.playing?'Ⅱ':'▶'}</button>
    <button aria-label="Next station" disabled={!enabled} onclick={()=>step(1)}>⏭</button>
  </div>
  <div class="volume">
    <button aria-label={playback?.muted?'Unmute':'Mute'} onclick={mute}>{playback?.muted?'⊘':'◖'}</button>
    <input aria-label="Volume" type="range" min="0" max="1" step=".01" value={playback?.muted?0:playback?.volume??.8} oninput={e=>volume(Number(e.currentTarget.value))}/>
    <span>{Math.round((playback?.muted?0:playback?.volume??.8)*100)}%</span>
  </div>
  {#if showProgress}<div class="progress"><span>{time(playback?.seconds??0)}</span><progress aria-label="Recording progress" max="1" value={playback?.duration?playback.seconds/playback.duration:0}></progress><span>{time(playback?.duration??0)}</span></div>{/if}
</div>
