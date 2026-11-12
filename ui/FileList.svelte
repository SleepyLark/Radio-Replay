<script lang="ts">
  import type {Track} from './library';
  import TrackFolder from './TrackFolder.svelte';
  let {tracks,onmove,onremove}: {tracks:Track[]; onmove:(index:number,direction:number)=>void; onremove:(paths:string[])=>void}=$props();
  const folders=$derived.by(()=>{const result=new Map<string,{track:Track,index:number}[]>();tracks.forEach((track,index)=>{const normalized=track.path.replaceAll('\\','/');const folder=normalized.slice(0,normalized.lastIndexOf('/')+1)||'Audio files';if(!result.has(folder))result.set(folder,[]);result.get(folder)!.push({track,index});});return [...result];});
  let folderLimit=$state(50);
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus and scroll this named file region.) -->
<div id="fileList" class="editor-tracks" tabindex="0" role="region" aria-label="Audio files grouped by folder">
  {#each folders.slice(0,folderLimit) as [folder,items],index (folder)}
    <TrackFolder {folder} {items} total={tracks.length} initialOpen={index===0} {onmove} {onremove}/>
  {:else}<div class="file-drop-zone">Drop audio files or folders here, or use Add files / Add folder.</div>{/each}
  {#if folders.length>folderLimit}<button type="button" class="folder-more" onclick={()=>folderLimit+=50}>Show {Math.min(50,folders.length-folderLimit)} more folders</button>{/if}
</div>
