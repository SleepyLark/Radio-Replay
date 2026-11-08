<script lang="ts">
  import ControlIcon from './ControlIcon.svelte';
  let {volume = .8, muted = false, onvolume, onmute}: {volume?:number; muted?:boolean; onvolume:(value:number)=>void; onmute:()=>void}=$props();
  const id=$props.id();
  const level=$derived(muted?0:volume);
  function wheel(event:WheelEvent){event.preventDefault();event.stopPropagation();if(event.deltaY)onvolume(Math.max(0,Math.min(1,volume+(event.deltaY<0?.05:-.05))));}
</script>

<div class="volume-module volume" title="Volume: + / − · M to mute · Scroll to adjust" onwheel={wheel}>
  <button class="mute-button" aria-label={muted?'Unmute':'Mute'} aria-pressed={muted} data-silent={level===0} onclick={onmute}><ControlIcon name="speaker" silent={level===0}/></button>
  <label for={id}>VOL</label>
  <div class="volume-meter"><div class="volume-segments" aria-hidden="true">{#each Array(12) as _,index}<i class:lit={index<Math.round(level*12)} style:height={`${6+index*18/11}px`}></i>{/each}</div><input id={id} class="volume-input" aria-label="Volume" type="range" min="0" max="1" step=".01" value={level} oninput={event=>onvolume(Number(event.currentTarget.value))}/></div>
  <output class="volume-value" for={id}>{Math.round(level*100)}%</output>
</div>
