<script lang="ts">
  import type {Station} from './library';
  let {station,members,ontoggle}: {
    station:Station; members:ReadonlySet<string>; ontoggle:(key:string)=>void;
  }=$props();
  let open=$state(false);
  let limit=$state(100);
</script>

<details {open} ontoggle={event=>{open=event.currentTarget.open;if(!open)limit=100;}}>
  <summary><label><input type="checkbox" checked={members.has(station.key)} onchange={()=>ontoggle(station.key)}/>{station.game?`${station.game} / `:''}{station.station}</label></summary>
  {#if open&&station.mode==='tracks'}
    {#each station.tracks.slice(0,limit) as track (track.path)}
      <label class="track-choice"><input type="checkbox" checked={members.has(`${station.key}|${track.path}`)} onchange={()=>ontoggle(`${station.key}|${track.path}`)}/>{track.name}</label>
    {/each}
    {#if station.tracks.length>limit}<button type="button" class="group-more" onclick={()=>limit+=100}>Show {Math.min(100,station.tracks.length-limit)} more tracks</button>{/if}
  {/if}
</details>
