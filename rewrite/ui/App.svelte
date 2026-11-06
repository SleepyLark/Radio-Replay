<script lang="ts">
  import { onMount } from 'svelte';
  import { desktop, emptyConfig, loadConfig, importConfig, artworkUrl, audioUrl, prepareStation, type Config, type Station } from './library';
  import { Playback, type PlaybackState } from './playback/engine';
  import StationEditor from './StationEditor.svelte';
  let config = $state(emptyConfig());
  let query = $state('');
  let active = $state<Station | null>(null);
  let busy = $state(true);
  let error = $state('');
  let view = $state('Stations');
  let editing = $state(false);
  let editTarget = $state<Station | null>(null);
  let player: Playback | null = null;
  let playback = $state<PlaybackState | null>(null);
  const lineup = $derived(config.custom.filter(s => !config.hidden.includes(s.key)));
  function tune(station: Station) { active = station; if(desktop) void player?.tune(station,config.settings); }
  function step(direction: number) {
    if(!lineup.length) return;
    const index = lineup.findIndex(s => s.key === playback?.key);
    tune(lineup[(Math.max(0,index) + direction + lineup.length) % lineup.length]);
  }
  function toggle() { if(playback?.playing) void player?.pause(); else void player?.play(); }
  function time(seconds: number) { const n = Math.floor(Math.max(0,seconds)); return `${Math.floor(n / 60)}:${String(n % 60).padStart(2,'0')}`; }
  function edit(station: Station | null) { editTarget = station; editing = true; }
  function saved(next: Config) { player?.invalidate(); config = next; active = next.custom.find(s => s.key === active?.key) ?? null; editing = false; }
  const stations = $derived(config.custom.filter(s => !config.hidden.includes(s.key) && s.station.toLowerCase().includes(query.toLowerCase())));
  onMount(() => {
    if(desktop) player = new Playback({prepare:prepareStation,url:audioUrl,onState:state => playback = state});
    void (async () => { try { config = await loadConfig(); } catch (e) { error = String(e); } finally { busy = false; } })();
    return () => player?.dispose();
  });
  async function importLibrary() {
    busy = true; error = '';
    try { const imported = await importConfig(); if (imported) { player?.invalidate(); config = imported; active = null; } }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<svelte:window onkeydown={(event) => {
  if(editing || view !== 'Stations' || event.ctrlKey || event.metaKey || event.altKey || (event.target instanceof HTMLElement && event.target.closest('input,select,textarea,button'))) return;
  if(event.code === 'Space') { event.preventDefault(); toggle(); }
  if(event.key === 'ArrowDown') { event.preventDefault(); step(1); }
  if(event.key === 'ArrowUp') { event.preventDefault(); step(-1); }
  if(event.key.toLowerCase() === 'm') player?.mute();
}}/>

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
    {#if playback?.error}<div class="notice" role="status">{playback.error}</div>{/if}
    {#if !desktop}<div class="notice">Browser preview. Open the desktop app to import your library.</div>{/if}
    {#if view === 'Stations'}
      <section class="heading"><div><p class="eyebrow">MAKE YOURSELF AT HOME</p><h1>Your stations</h1><p>Your collection, on your frequency.</p></div><div class="actions"><button class="secondary" onclick={importLibrary} disabled={busy || !desktop || editing}>Import stations.json</button><button class="primary" onclick={() => edit(null)} disabled={busy || !desktop || editing}>Create station</button></div></section>
      {#if editing}
        {#key editTarget?.key}<StationEditor station={editTarget} logo={editTarget ? config.logos[editTarget.key] : ''} onsaved={saved} oncancel={() => editing = false}/>{/key}
      {:else}
      {#if busy}<p role="status">Loading library…</p>
      {:else if stations.length}
        <div class="grid">{#each stations as station}<button class="station" class:active={active?.key === station.key} onclick={() => tune(station)}><div class="art">{#if artworkUrl(config.logos[station.key])}<img src={artworkUrl(config.logos[station.key])} alt="" />{:else}◉{/if}<span>{station.station.slice(0, 2).toUpperCase()}</span></div><strong>{station.station}</strong><small>{station.tracks.length} tracks · {station.mode === 'tracks' ? 'Music station' : 'Broadcast'}</small></button>{/each}</div>
      {:else}<section class="empty"><div class="dial">◎</div><h2>A fresh frequency</h2><p>Import your existing Radio Replay configuration to browse your stations here.</p><small>Your audio stays in its original folders.</small></section>{/if}
      {#if active}<section class="details"><div class="actions"><h2>{active.station}</h2><button class="secondary" onclick={() => edit(active)} disabled={!desktop}>Edit station</button></div><p>{playback?.key === active.key ? playback.loading ? 'Tuning…' : playback.title : 'Select this station to tune in.'}</p>{#each active.tracks as track}<div class="track"><span>{track.name}</span><small>{track.path}</small></div>{/each}</section>{/if}
      {/if}
    {:else if view === 'Groups'}<section class="heading"><div><h1>Your groups</h1><p>{config.groups.length} groups preserved from your library. Group editing arrives in a later milestone.</p></div></section>
    {:else}<section class="heading"><div><h1>Settings</h1><p>This preview saves imported data separately from the Electron app.</p><p>Existing appearance and playback settings are preserved for the next milestones.</p></div></section>{/if}
  </main>
  <footer><div class="now-icon">◉</div><div class="now-playing"><strong>{playback?.station || 'Nothing tuned yet'}</strong><small>{playback?.title || 'Choose a station to tune in'}</small></div><div class="player-controls"><div class="transport"><button disabled={!desktop || !lineup.length} aria-label="Previous station" onclick={() => step(-1)}>|◁</button><button disabled={!playback?.key || playback.loading} aria-label={playback?.playing ? 'Pause' : 'Play'} onclick={toggle}>{playback?.playing ? '❚❚' : '▷'}</button><button disabled={!desktop || !lineup.length} aria-label="Next station" onclick={() => step(1)}>▷|</button></div><div class="progress"><span>{time(playback?.seconds ?? 0)}</span><progress aria-label="Recording progress" max="1" value={playback?.duration ? playback.seconds / playback.duration : 0}></progress><span>{time(playback?.duration ?? 0)}</span></div></div><div class="volume"><button aria-label={playback?.muted ? 'Unmute' : 'Mute'} onclick={() => player?.mute()} disabled={!desktop}>{playback?.muted ? 'Muted' : 'Volume'}</button><input aria-label="Volume" type="range" min="0" max="1" step="0.01" value={playback?.volume ?? .8} oninput={event => player?.setVolume(Number(event.currentTarget.value))} disabled={!desktop}/></div></footer>
</div>
