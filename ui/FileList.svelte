<script lang="ts">
  import type {Track} from './library';
  import TrackFolder from './TrackFolder.svelte';
  import {trackFolder} from './playlist';
  let {tracks,network=false,reorder,onmove,onmovefolder,onremove}: {tracks:Track[]; network?:boolean; reorder:boolean; onmove:(index:number,direction:number)=>void; onmovefolder:(folder:string,target:string|undefined,position:'before'|'after'|'first')=>void; onremove:(paths:string[])=>void}=$props();
  const folders=$derived.by(()=>{const result=new Map<string,{track:Track,index:number}[]>();tracks.forEach((track,index)=>{const folder=trackFolder(track);if(!result.has(folder))result.set(folder,[]);result.get(folder)!.push({track,index});});return [...result];});
  let folderLimit=$state(50);
  function neighbor(index:number,direction:number){const source=folders[index][0];for(let at=index+direction;at>=0&&at<folders.length;at+=direction){if(!folders[at][0].startsWith(source))return folders[at][0];}}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus and scroll this named file region.) -->
<div id="fileList" class="editor-tracks" tabindex="0" role="region" aria-label={network?'Network stream links':'Audio files grouped by folder'}>
  {#if network}
    {#each tracks.slice(0,folderLimit) as track (track.path)}
      <div class="file-row network-link"><span><span class="file-label">{track.name}</span><small>{track.path}</small></span><button type="button" aria-label={'Remove '+track.name} onclick={()=>onremove([track.path])}>Remove</button></div>
    {:else}<div class="file-drop-zone">Add a stream link above or import a network playlist.</div>{/each}
    {#if tracks.length>folderLimit}<button type="button" onclick={()=>folderLimit+=100}>Show more links</button>{/if}
  {:else}
  {#each folders.slice(0,folderLimit) as [folder,items],index (folder)}
    <TrackFolder {folder} {items} {reorder} previous={neighbor(index,-1)} next={neighbor(index,1)} total={tracks.length} initialOpen={index===0} {onmove} {onmovefolder} {onremove}/>
  {:else}<div class="file-drop-zone">Drop audio files or folders here, or use Add files / Add folder / Import playlist.</div>{/each}
  {#if folders.length>folderLimit}<button type="button" class="folder-more" onclick={()=>folderLimit+=50}>Show {Math.min(50,folders.length-folderLimit)} more folders</button>{/if}
  {/if}
</div>

<style>.network-link>span{min-width:0;white-space:normal}.network-link .file-label{display:block}.network-link small{display:block;overflow-wrap:anywhere;color:var(--muted)}.network-link button{flex-shrink:0}</style>
