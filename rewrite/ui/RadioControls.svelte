<script lang="ts">
  import type { PlaybackState } from './playback/engine';
  import ControlIcon from './ControlIcon.svelte';
  import VolumeControl from './VolumeControl.svelte';
  import {formatTime as time} from './playback/format';
  let {playback, enabled, showProgress, toggle, step, mute, volume, compact = false}: {playback: PlaybackState | null; enabled: boolean; showProgress: boolean; toggle: () => void; step: (direction: number) => void; mute: () => void; volume: (value: number) => void; compact?: boolean} = $props();
</script>

<section class="controls radio-controls" class:compact-controls={compact} aria-label={compact?'Now playing controls':'Radio controls'}>
  <div class="transport-buttons transport">
    <button aria-label="Previous station" disabled={!enabled} onclick={()=>step(-1)}><ControlIcon name="previous"/></button>
    <button class="play play-button" aria-label={playback?.playing?'Pause':'Play'} disabled={!playback?.key || playback.loading} onclick={toggle}><ControlIcon name={playback?.playing?'pause':'play'}/></button>
    <button aria-label="Next station" disabled={!enabled} onclick={()=>step(1)}><ControlIcon name="next"/></button>
  </div>
  {#if showProgress}<div class="recording-timeline progress"><output aria-label="Playback time">{time(playback?.seconds??0)} / {time(playback?.duration??0)}</output><progress aria-label="Recording progress" max="1" value={playback?.duration?playback.seconds/playback.duration:0}></progress></div>{/if}
  <VolumeControl volume={playback?.volume??.8} muted={playback?.muted??false} onvolume={volume} onmute={mute}/>
</section>
