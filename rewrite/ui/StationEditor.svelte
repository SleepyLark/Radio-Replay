<script lang="ts">
  import { untrack } from 'svelte';
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
    await perform(async () => onsaved(await saveStation($state.snapshot({key, name, mode, tracks, logoPath,options:{textMode,crossfade:customFade?fade:null}}))));
  }
</script>

<section class="editor" aria-label={station ? 'Edit station' : 'Create station'}>
  <h2>{station ? 'Edit station' : 'Create station'}</h2>
  {#if error}<div class="error" role="alert">{error}</div>{/if}
  <form onsubmit={save}>
    <fieldset disabled={busy}>
      <label>Station name<input required maxlength="200" bind:value={name} placeholder="Your station name" /></label>
      <label>Playback style<select bind:value={mode}><option value="tracks">Music station · shuffle</option><option value="longset">Radio broadcast · ordered</option></select></label>
      <label>Timestamped text<select bind:value={textMode}><option value="auto">Automatic (lyrics for music, titles for broadcasts)</option><option value="lyrics">Lyrics</option><option value="titles">Track / segment titles</option><option value="ignore">Ignore</option></select></label>
      <label class="check"><input type="checkbox" bind:checked={customFade}/>Override station crossfade</label>
      {#if customFade}<label>Crossfade (milliseconds)<input type="number" min="0" max="30000" step="100" required bind:value={fade}/></label>{/if}
      <div class="editor-art">{#if logoPath}<img src={artworkUrl(logoPath)} alt="Station artwork preview" />{/if}<button type="button" class="secondary" onclick={() => perform(async () => { const path = await chooseArtwork(); if (path) logoPath = path; })}>Choose artwork</button>{#if logoPath}<button type="button" onclick={() => logoPath = ''}>Remove artwork</button>{/if}</div>
      <div class="actions"><button type="button" class="secondary" onclick={() => add(false)}>Add files</button><button type="button" class="secondary" onclick={() => add(true)}>Add folder</button><span>{tracks.length} tracks</span></div>
      <p>Folders include supported audio in subfolders. Use the arrows to arrange broadcast order.</p>
      <div class="editor-tracks">{#each tracks as track, index (track.path)}<div class="track editor-track"><div><strong>{track.name}</strong><small>{track.path}</small></div><div class="actions"><button type="button" aria-label={`Move ${track.name} up`} disabled={index === 0} onclick={() => move(index, -1)}>↑</button><button type="button" aria-label={`Move ${track.name} down`} disabled={index === tracks.length - 1} onclick={() => move(index, 1)}>↓</button><button type="button" aria-label={`Remove ${track.name}`} onclick={() => tracks = tracks.filter(t => t.path !== track.path)}>Remove</button></div></div>{/each}</div>
      <div class="actions editor-footer"><button class="primary" type="submit" disabled={!name.trim() || !tracks.length}>Save station</button><button type="button" class="secondary" onclick={oncancel}>Cancel</button>{#if station}<button type="button" class="danger" onclick={() => confirmDelete = true}>Delete station</button>{/if}</div>
      {#if confirmDelete}<div class="notice"><p>Remove this station from your library? Your audio and artwork files stay on disk.</p><div class="actions"><button type="button" class="danger" onclick={() => perform(async () => onsaved(await deleteStation(key)))}>Delete station permanently</button><button type="button" onclick={() => confirmDelete = false}>Keep station</button></div></div>{/if}
    </fieldset>
    {#if busy}<p role="status">Working…</p>{/if}
  </form>
</section>
