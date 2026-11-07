<script lang="ts">
  import type { PlaybackState } from './playback/engine';
  import ControlIcon from './ControlIcon.svelte';
  let {playback, enabled, showProgress, toggle, step, mute, volume, compact = false}: {playback: PlaybackState | null; enabled: boolean; showProgress: boolean; toggle: () => void; step: (direction: number) => void; mute: () => void; volume: (value: number) => void; compact?: boolean} = $props();
  const id = $props.id();
  const level = $derived(playback?.muted ? 0 : playback?.volume ?? .8);
  function time(seconds: number) {const n=Math.floor(Math.max(0,seconds));return `${String(Math.floor(n/60)).padStart(2,'0')}:${String(n%60).padStart(2,'0')}`;}
  function wheel(event: WheelEvent) {
    event.preventDefault(); event.stopPropagation();
    if (event.deltaY) volume((playback?.volume ?? .8) + (event.deltaY < 0 ? .05 : -.05));
  }
</script>

<section class="controls radio-controls" class:compact-controls={compact} aria-label={compact?'Now playing controls':'Radio controls'}>
  <div class="transport-buttons transport">
    <button aria-label="Previous station" disabled={!enabled} onclick={()=>step(-1)}><ControlIcon name="previous"/></button>
    <button class="play play-button" aria-label={playback?.playing?'Pause':'Play'} disabled={!playback?.key || playback.loading} onclick={toggle}><ControlIcon name={playback?.playing?'pause':'play'}/></button>
    <button aria-label="Next station" disabled={!enabled} onclick={()=>step(1)}><ControlIcon name="next"/></button>
  </div>
  {#if showProgress}<div class="recording-timeline progress"><output aria-label="Playback time">{time(playback?.seconds??0)} / {time(playback?.duration??0)}</output><progress aria-label="Recording progress" max="1" value={playback?.duration?playback.seconds/playback.duration:0}></progress></div>{/if}
  <div class="volume-module volume" title="Volume: + / − · M to mute · Scroll to adjust" onwheel={wheel}>
    <button class="mute-button" aria-label={playback?.muted?'Unmute':'Mute'} aria-pressed={playback?.muted??false} data-silent={level===0} onclick={mute}>
      <ControlIcon name="speaker" silent={level===0}/>
    </button>
    <label for={id}>VOL</label>
    <div class="volume-meter"><div class="volume-segments" aria-hidden="true">{#each Array(12) as _, index}<i class:lit={index < Math.round(level*12)} style:height={`${6+index*18/11}px`}></i>{/each}</div><input id={id} class="volume-input" aria-label="Volume" type="range" min="0" max="1" step=".01" value={level} oninput={e=>volume(Number(e.currentTarget.value))}/></div>
    <output class="volume-value" for={id}>{Math.round(level*100)}%</output>
  </div>
</section>
