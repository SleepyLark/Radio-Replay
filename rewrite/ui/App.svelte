<script lang="ts">
  import { onMount } from 'svelte';
  import { desktop, emptyConfig, loadConfig, importConfig, type Station } from './library';
  let config = $state(emptyConfig());
  let query = $state('');
  let active = $state<Station | null>(null);
  let busy = $state(true);
  let error = $state('');
  let view = $state('Stations');
  const stations = $derived(config.custom.filter(s => !config.hidden.includes(s.key) && s.station.toLowerCase().includes(query.toLowerCase())));
  onMount(async () => {
    try { config = await loadConfig(); } catch (e) { error = String(e); }
    finally { busy = false; }
  });
  async function importLibrary() {
    busy = true; error = '';
    try { const imported = await importConfig(); if (imported) { config = imported; active = null; } }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<div class="shell">
  <aside>
    <button class="brand" onclick={() => view = 'Stations'} aria-label="Radio Replay home">◉ <span>RADIO<br/>REPLAY</span></button>
    <p class="eyebrow">YOUR FREQUENCIES</p>
    <nav>{#each ['Stations', 'Groups', 'Settings'] as item}<button class:selected={view === item} onclick={() => view = item}>{item}</button>{/each}</nav>
    <div class="sidebar-note"><span class="signal">●</span> Local by design<br/><small>Rust preview · 0.1</small></div>
  </aside>
  <main>
    <header><label class="search"><span>⌕</span><input aria-label="Search stations" placeholder="Search your stations" bind:value={query}/></label><span class="badge">FOUNDATION PREVIEW</span></header>
    {#if error}<div class="error" role="alert">{error}</div>{/if}
    {#if !desktop}<div class="notice">Browser preview. Open the desktop app to import your library.</div>{/if}
    {#if view === 'Stations'}
      <section class="heading"><div><p class="eyebrow">MAKE YOURSELF AT HOME</p><h1>Your stations</h1><p>Your collection, on your frequency.</p></div><button class="primary" onclick={importLibrary} disabled={busy || !desktop}>Import stations.json</button></section>
      {#if busy}<p role="status">Loading library…</p>
      {:else if stations.length}
        <div class="grid">{#each stations as station}<button class="station" class:active={active?.key === station.key} onclick={() => active = station}><div class="art">◉<span>{station.station.slice(0, 2).toUpperCase()}</span></div><strong>{station.station}</strong><small>{station.tracks.length} tracks · {station.mode === 'tracks' ? 'Music station' : 'Broadcast'}</small></button>{/each}</div>
      {:else}<section class="empty"><div class="dial">◎</div><h2>A fresh frequency</h2><p>Import your existing Radio Replay configuration to browse your stations here.</p><small>Your audio stays in its original folders.</small></section>{/if}
      {#if active}<section class="details"><h2>{active.station}</h2><p>Playback is the next migration milestone.</p>{#each active.tracks as track}<div class="track"><span>{track.name}</span><small>{track.path}</small></div>{/each}</section>{/if}
    {:else if view === 'Groups'}<section class="heading"><div><h1>Your groups</h1><p>{config.groups.length} groups preserved from your library. Group editing arrives in a later milestone.</p></div></section>
    {:else}<section class="heading"><div><h1>Settings</h1><p>This preview saves imported data separately from the Electron app.</p><p>Existing appearance and playback settings are preserved for the next milestones.</p></div></section>{/if}
  </main>
  <footer><div class="now-icon">◉</div><div><strong>{active?.station ?? 'Nothing tuned yet'}</strong><small>Playback engine pending</small></div><div class="transport"><button disabled aria-label="Previous station">|◁</button><button disabled aria-label="Play">▷</button><button disabled aria-label="Next station">▷|</button></div><span class="footer-note">YOUR MUSIC. YOUR AIRWAVES.</span></footer>
</div>
