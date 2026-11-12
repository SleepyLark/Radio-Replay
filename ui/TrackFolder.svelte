<script lang="ts">
  import {untrack} from 'svelte';
  import type {Track} from './library';

  let {folder,items,total,initialOpen,reorder,previous,next,onmove,onmovefolder,onremove}: {
    folder:string; items:{track:Track;index:number}[]; total:number; initialOpen:boolean;
    reorder:boolean; previous:string|undefined; next:string|undefined;
    onmovefolder:(folder:string,target:string|undefined,position:'before'|'after'|'first')=>void;
    onmove:(index:number,direction:number)=>void; onremove:(paths:string[])=>void;
  }=$props();
  let open=$state(untrack(()=>initialOpen));
  let limit=$state(100);
  const name=$derived(folder==='Audio files'?folder:folder.split('/').filter(Boolean).at(-1));
</script>

<details class="file-folder" {open} ontoggle={event=>{open=event.currentTarget.open;if(!open)limit=100;}}>
  <summary>
    <span title={folder}>{name}</span>
    <small>{items.length} files</small>
    {#if reorder}
      <button type="button" aria-label={`Move ${name} folder up`} title="Move folder up" disabled={!previous} onclick={event=>{event.preventDefault();onmovefolder(folder,previous,'before');}}>↑</button>
      <button type="button" aria-label={`Move ${name} folder down`} title="Move folder down" disabled={!next} onclick={event=>{event.preventDefault();onmovefolder(folder,next,'after');}}>↓</button>
      <button type="button" aria-label={`Move ${name} folder to first`} title="Move folder to first" onclick={event=>{event.preventDefault();onmovefolder(folder,undefined,'first');}}>⇈ First</button>
    {/if}
    <button type="button" onclick={event=>{event.preventDefault();onremove(items.map(item=>item.track.path));}}>Remove folder</button>
  </summary>
  {#if open}
    <div class="folder-files">
      {#each items.slice(0,limit) as {track,index} (track.path)}
        <div class="file-row">
          <span>{#if reorder}<small class="file-number">{index+1}</small>{/if}<span class="file-label" title={track.path}>{track.name}</span></span>
          <div class="file-row-actions">
            {#if reorder}<button type="button" aria-label={`Move ${track.name} up`} disabled={index===0} onclick={()=>onmove(index,-1)}>↑</button>
            <button type="button" aria-label={`Move ${track.name} down`} disabled={index===total-1} onclick={()=>onmove(index,1)}>↓</button>{/if}
            <button type="button" aria-label={`Remove ${track.name}`} onclick={()=>onremove([track.path])}>Remove</button>
          </div>
        </div>
      {/each}
      {#if items.length>limit}<button type="button" class="file-more" onclick={()=>limit+=100}>Show {Math.min(100,items.length-limit)} more files</button>{/if}
    </div>
  {/if}
</details>
