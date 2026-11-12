<script lang="ts">
  import {untrack} from 'svelte';
  import type {Track} from './library';

  let {folder,items,total,initialOpen,onmove,onremove}: {
    folder:string; items:{track:Track;index:number}[]; total:number; initialOpen:boolean;
    onmove:(index:number,direction:number)=>void; onremove:(paths:string[])=>void;
  }=$props();
  let open=$state(untrack(()=>initialOpen));
  let limit=$state(100);
</script>

<details class="file-folder" {open} ontoggle={event=>{open=event.currentTarget.open;if(!open)limit=100;}}>
  <summary>
    <span title={folder}>{folder==='Audio files'?folder:folder.split('/').filter(Boolean).at(-1)}</span>
    <small>{items.length} files</small>
    <button type="button" onclick={event=>{event.preventDefault();onremove(items.map(item=>item.track.path));}}>Remove folder</button>
  </summary>
  {#if open}
    <div class="folder-files">
      {#each items.slice(0,limit) as {track,index} (track.path)}
        <div class="file-row">
          <span><small class="file-number">{index+1}</small><span class="file-label" title={track.path}>{track.name}</span></span>
          <div class="file-row-actions">
            <button type="button" aria-label={`Move ${track.name} up`} disabled={index===0} onclick={()=>onmove(index,-1)}>↑</button>
            <button type="button" aria-label={`Move ${track.name} down`} disabled={index===total-1} onclick={()=>onmove(index,1)}>↓</button>
            <button type="button" aria-label={`Remove ${track.name}`} onclick={()=>onremove([track.path])}>Remove</button>
          </div>
        </div>
      {/each}
      {#if items.length>limit}<button type="button" class="file-more" onclick={()=>limit+=100}>Show {Math.min(100,items.length-limit)} more files</button>{/if}
    </div>
  {/if}
</details>
