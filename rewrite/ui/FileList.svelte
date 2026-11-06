<script lang="ts">
  import type {Track} from './library';
  let {tracks,onmove,onremove}: {tracks:Track[]; onmove:(index:number,direction:number)=>void; onremove:(paths:string[])=>void}=$props();
  const folders=$derived.by(()=>{const result=new Map<string,{track:Track,index:number}[]>();tracks.forEach((track,index)=>{const normalized=track.path.replaceAll('\\','/');const folder=normalized.slice(0,normalized.lastIndexOf('/')+1)||'Audio files';if(!result.has(folder))result.set(folder,[]);result.get(folder)!.push({track,index});});return [...result];});
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus and scroll this named file region.) -->
<div id="fileList" class="editor-tracks" tabindex="0" role="region" aria-label="Audio files grouped by folder">
  {#each folders as [folder,items]}<details class="file-folder" open><summary><span title={folder}>{folder==='Audio files'?folder:folder.split('/').filter(Boolean).at(-1)}</span><small>{items.length} files</small><button type="button" onclick={event=>{event.preventDefault();onremove(items.map(item=>item.track.path));}}>Remove folder</button></summary>
    <div class="folder-files">{#each items as {track,index} (track.path)}<div class="file-row"><span><small class="file-number">{index+1}</small><span class="file-label" title={track.path}>{track.name}</span></span><div class="file-row-actions"><button type="button" aria-label={`Move ${track.name} up`} disabled={index===0} onclick={()=>onmove(index,-1)}>↑</button><button type="button" aria-label={`Move ${track.name} down`} disabled={index===tracks.length-1} onclick={()=>onmove(index,1)}>↓</button><button type="button" aria-label={`Remove ${track.name}`} onclick={()=>onremove([track.path])}>Remove</button></div></div>{/each}</div>
  </details>{:else}<div class="file-drop-zone">Drop audio files or folders here, or use Add files / Add folder.</div>{/each}
</div>
