<script lang="ts">
  import { untrack } from 'svelte';
  import StationBanner from './StationBanner.svelte';
  import { fadeSeconds } from './playback/timeline';
  import { chooseAudio, chooseArtwork, saveStation, deleteStation, artworkUrl, type Config, type Station, type Track } from './library';
  let { station, logo, onsaved, oncancel }: { station: Station | null; logo: unknown; onsaved: (config: Config) => void; oncancel: () => void } = $props();
  // Parent keys this component by station: each editor starts with an independent draft.
  const initial = untrack(() => ({ station, logo }));
  const key = initial.station?.key ?? crypto.randomUUID();
  let name = $state(initial.station?.station ?? '');
  let mode = $state(initial.station?.mode === 'tracks' ? 'tracks' : initial.station ? 'longset' : 'tracks');
  let tracks = $state<Track[]>(initial.station ? [...initial.station.tracks] : []);
  let logoPath = $state(typeof initial.logo === 'string' ? initial.logo : '');
  let bannerPath=$state(typeof initial.station?.bannerPath==='string'?initial.station.bannerPath:'');
  let useColor=$state(!!initial.station?.bannerColor);
  let bannerColor=$state(typeof initial.station?.bannerColor==='string'?initial.station.bannerColor:'#244b42');
  let useAccent=$state(!!initial.station?.accent);
  let accent=$state(typeof initial.station?.accent==='string'?initial.station.accent:'#c7f464');
  let spinLogo=$state(initial.station?.spinLogo===true);
  const preview=$derived({key,station:name,mode,tracks,bannerPath,bannerColor:useColor?bannerColor:'',spinLogo});
  function openDialog(node:HTMLDialogElement){node.showModal();return {destroy(){node.close();}};}
  let textMode=$state(['titles','lyrics','ignore'].includes(String(initial.station?.textMode))?String(initial.station?.textMode):'auto');
  let customFade=$state(initial.station?.crossfade!=null);
  let fade=$state(initial.station?fadeSeconds(initial.station,{crossfade:5000})*1000:5000);
  let busy = $state(false);
  let error = $state('');
  let confirmDelete = $state(false);
  async function perform(action: () => Promise<void>) {
    busy = true; error = '';
    try { await action(); } catch(e) { error = String(e); }
    finally { busy = false; }
  }
  async function add(folder: boolean) {
    await perform(async () => {
      const picked = await chooseAudio(folder);
      const paths = new Set(tracks.map(t => t.path));
      tracks = [...tracks, ...picked.filter(t => { if(paths.has(t.path)) return false; paths.add(t.path); return true; })];
      if (folder && picked.length === 0) error = 'No supported audio files were added. The selection may have been cancelled.';
    });
  }
  function move(index: number, direction: number) {
    const next = [...tracks]; const target = index + direction;
    [next[index], next[target]] = [next[target], next[index]]; tracks = next;
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    await perform(async () => onsaved(await saveStation($state.snapshot({key, name, mode, tracks, logoPath,options:{textMode,crossfade:customFade?fade:null,bannerPath,bannerColor:useColor?bannerColor:'',accent:useAccent?accent:'',spinLogo}}))));
  }
</script>

<dialog class="editor" use:openDialog oncancel={event=>{if(busy)event.preventDefault();}} onclose={oncancel} aria-label={station ? 'Edit station' : 'Create station'}>
  <div class="modal-heading"><div><p class="eyebrow">TUNE YOUR FREQUENCY</p><h2>{station ? 'EDIT STATION' : 'CREATE STATION'}</h2></div><button aria-label="Close station editor" disabled={busy} onclick={oncancel}>✕</button></div>
  <div style:--accent={useAccent?accent:undefined}><StationBanner station={preview} logo={logoPath} playing={false} preview/></div>
  {#if error}<div class="error" role="alert">{error}</div>{/if}
  <form onsubmit={save}>
    <fieldset disabled={busy}>
      <details open><summary>Essentials</summary>
      <label>Station name<input required maxlength="200" bind:value={name} placeholder="Your station name" /></label>
      <label>Playback style<select bind:value={mode}><option value="tracks">Music station · shuffle</option><option value="longset">Radio broadcast · ordered</option></select></label>
      <div class="actions"><button type="button" class="secondary" onclick={() => add(false)}>Add files</button><button type="button" class="secondary" onclick={() => add(true)}>Add folder</button><span>{tracks.length} tracks</span></div>
      <p>Folders include supported audio in subfolders. Use the arrows to arrange broadcast order.</p>
      <div class="editor-tracks">{#each tracks as track, index (track.path)}<div class="track editor-track"><div><strong>{track.name}</strong><small>{track.path}</small></div><div class="actions"><button type="button" aria-label={`Move ${track.name} up`} disabled={index === 0} onclick={() => move(index, -1)}>↑</button><button type="button" aria-label={`Move ${track.name} down`} disabled={index === tracks.length - 1} onclick={() => move(index, 1)}>↓</button><button type="button" aria-label={`Remove ${track.name}`} onclick={() => tracks = tracks.filter(t => t.path !== track.path)}>Remove</button></div></div>{/each}</div>
      </details>
      <details><summary>Appearance</summary>
        <div class="editor-art">{#if logoPath}<img src={artworkUrl(logoPath)} alt="Station artwork preview" />{/if}<button type="button" class="secondary" onclick={() => perform(async () => { const path = await chooseArtwork(); if (path) logoPath = path; })}>Choose logo</button>{#if logoPath}<button type="button" onclick={() => logoPath = ''}>Remove logo</button>{/if}</div>
        <label class="check"><input type="checkbox" bind:checked={spinLogo}/>Spin logo while playing</label>
        <div class="actions"><button type="button" class="secondary" onclick={()=>perform(async()=>{const path=await chooseArtwork();if(path)bannerPath=path;})}>Choose banner image</button>{#if bannerPath}<button type="button" onclick={()=>bannerPath=''}>Remove banner image</button>{/if}</div>
        {#if bannerPath}<p>{bannerPath.split(/[\\/]/).at(-1)}</p>{/if}
        <label class="check"><input type="checkbox" bind:checked={useColor}/>Use a banner color</label>
        {#if useColor}<label>Banner color<input type="color" bind:value={bannerColor}/></label>{/if}
        <label class="check"><input type="checkbox" bind:checked={useAccent}/>Override player accent</label>
        {#if useAccent}<label>Station accent<input type="color" bind:value={accent}/></label>{/if}
      </details>
      <details><summary>Advanced</summary>
        <label>Timestamped text<select bind:value={textMode}><option value="auto">Automatic (lyrics for music, titles for broadcasts)</option><option value="lyrics">Lyrics</option><option value="titles">Track / segment titles</option><option value="ignore">Ignore</option></select></label>
        <label class="check"><input type="checkbox" bind:checked={customFade}/>Override station crossfade</label>
        {#if customFade}<label>Crossfade (milliseconds)<input type="number" min="0" max="30000" step="100" required bind:value={fade}/></label>{/if}
      </details>
      {#if station}<div class="editor-footer"><button type="button" class="danger" onclick={() => confirmDelete = true}>Delete station</button></div>{/if}
      {#if confirmDelete}<div class="notice"><p>Remove this station from your library? Your audio and artwork files stay on disk.</p><div class="actions"><button type="button" class="danger" onclick={() => perform(async () => onsaved(await deleteStation(key)))}>Delete station permanently</button><button type="button" onclick={() => confirmDelete = false}>Keep station</button></div></div>{/if}
      <div class="modal-footer"><button type="button" class="secondary" onclick={oncancel}>Cancel</button><button class="primary" type="submit" disabled={!name.trim() || !tracks.length}>Save station</button></div>
    </fieldset>
    {#if busy}<p role="status">Working…</p>{/if}
  </form>
</dialog>
