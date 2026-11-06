<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { desktop, emptyConfig, loadConfig, importConfig, artworkUrl, audioUrl, prepareStation, rememberDuration, trackDetails, type TrackDetails, type Config, type Station } from './library';
  import Groups from './Groups.svelte';
  import Settings from './Settings.svelte';
  import { groupLineup,parseTimedText,currentLine,StationHistory } from './playback/text';
  import { Playback, type PlaybackState } from './playback/engine';
  import StationEditor from './StationEditor.svelte';
  import {listenDesktop,publishDesktop,openMini,quitDesktop,type DesktopAction,type DesktopSnapshot} from './desktop';
  import {resolveTuningSound} from './library';
  import bundledStatic from '../../assets/audio/static.mp3?url';
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
  let groupId=$state('all');
  let history=new StationHistory();
  let details=$state<TrackDetails|null>(null);
  const metadataKey=$derived(playback?.key??'');
  const metadataPath=$derived(playback?.path??'');
  const groups=$derived(config.groups.filter(g=>g&&typeof g.id==='string'&&Array.isArray(g.members)));
  const selectedGroup=$derived(groups.find(g=>g.id===groupId));
  const lineup = $derived(groupLineup(config.custom.filter(s => !config.hidden.includes(s.key)),selectedGroup));
  const timed=$derived(parseTimedText(details?.sidecar??null));
  const lineIndex=$derived(currentLine(timed,playback?.seconds??0));
  const lyricStart=$derived(Math.max(0,lineIndex-5));
  const lyricWindow=$derived(timed.slice(lyricStart,lyricStart+16));
  const playingStation=$derived(config.custom.find(s=>s.key===playback?.key));
  const textMode=$derived(['titles','lyrics','ignore'].includes(String(playingStation?.textMode))?String(playingStation?.textMode):playingStation?.mode==='longset'?'titles':'lyrics');
  const embeddedTitle=$derived(details?.tags.title?`${details.tags.artist?details.tags.artist+' - ':''}${details.tags.title}`:playback?.title??'');
  const displayTitle=$derived(textMode==='titles'&&lineIndex>=0?timed[lineIndex].text:embeddedTitle);
  $effect(()=>{ const key=metadataKey,path=metadataPath;details=null;let cancelled=false;if(key&&path)void trackDetails(key,path).then(value=>{if(!cancelled)details=value;}).catch(()=>{});return()=>{cancelled=true;}; });
  $effect(()=>{ const current=lineIndex, mode=textMode, visible=view==='Stations'&&!editing, reduced=config.settings.reduceMotion===true;let cancelled=false; if(visible&&mode==='lyrics'&&current>=0)void tick().then(()=>{if(!cancelled)document.querySelector('.lyrics .current')?.scrollIntoView({block:'nearest',behavior:reduced?'auto':'smooth'});});return()=>{cancelled=true;}; });
  $effect(()=>{document.documentElement.style.setProperty('--accent',typeof config.settings.accent==='string'?config.settings.accent:'#c7f464');document.documentElement.dataset.density=String(config.settings.listDensity??'comfortable');document.documentElement.classList.toggle('reduce-motion',config.settings.reduceMotion===true);});
  function tune(station: Station,navigation=false) { if(!navigation)history.cancelPending();active = station; const members=selectedGroup?new Set(selectedGroup.members):null; const projected=members&&!members.has(station.key)?{...station,tracks:station.tracks.filter(t=>members.has(`${station.key}|${t.path}`))}:station; if(desktop) void player?.tune(projected,config.settings,groupId); }
  function showView(next:string){view=next;void tick().then(()=>document.querySelector('main')?.scrollTo({top:0}));}
  function step(direction: number) {
    if(!lineup.length) return;
    if(config.settings.randomNext===true){const next=history.next(lineup,direction);if(next)tune(next,true);return;}
    const index = lineup.findIndex(s => s.key === playback?.key);
    tune(lineup[(Math.max(0,index) + direction + lineup.length) % lineup.length]);
  }
  function toggle() { if(playback?.playing) void player?.pause(); else void player?.play(); }
  function desktopAction(action:DesktopAction){switch(action.type){case'toggle':toggle();break;case'play':if(!playback?.playing)void player?.play();break;case'pause':if(playback?.playing)void player?.pause();break;case'previous':step(-1);break;case'next':step(1);break;case'mute':player?.mute();break;case'volume':if(Number.isFinite(action.value))player?.setVolume(action.value);break;}}
  async function mini(){try{await publish();await openMini();}catch(e){error=String(e);}}
  function publish(){if(!desktop)return Promise.resolve();const snapshot:DesktopSnapshot={station:playback?.station??'',title:textMode==='titles'?displayTitle:(details?.tags.title||playback?.title||''),artist:details?.tags.artist??'',album:details?.tags.album??'',art:artworkUrl(config.logos[playback?.key??'']),accent:typeof config.settings.accent==='string'?config.settings.accent:'#c7f464',playing:playback?.playing??false,hasStation:!!playback?.key&&!playback.loading,canNavigate:lineup.length>0,seconds:playback?.seconds??0,duration:playback?.duration??0,volume:playback?.volume??.8,muted:playback?.muted??false};return publishDesktop(snapshot);}
  function time(seconds: number) { const n = Math.floor(Math.max(0,seconds)); return `${Math.floor(n / 60)}:${String(n % 60).padStart(2,'0')}`; }
  function edit(station: Station | null) { editTarget = station; editing = true; }
  function saved(next: Config) { player?.invalidate();history=new StationHistory(); config = next; active = next.custom.find(s => s.key === active?.key) ?? null; editing = false;if(groupId!=='all'&&!next.groups.some(g=>g&&g.id===groupId))groupId='all'; }
  function preferencesSaved(next:Config){if(next.settings.crossfade!==config.settings.crossfade)player?.invalidate();config=next;}
  function changeGroup(){player?.stop();active=null;history=new StationHistory();}
  const stations = $derived(lineup.filter(s => s.station.toLowerCase().includes(query.toLowerCase())));
  onMount(() => {
    if(desktop) player = new Playback({prepare:key=>prepareStation(key,groupId),rememberDuration,url:audioUrl,tuningSource:async value=>{if(value==='')return null;if(typeof value!=='string'||value==='bundled')return bundledStatic;const path=await resolveTuningSound();return path?audioUrl(path):null;},onState:state => {playback = state;if(state.playing&&!state.loading)history.commit(state.key);}});
    void (async () => { try { config = await loadConfig(); } catch (e) { error = String(e); } finally { busy = false; } })();
    let disposed=false;let unlisten:(()=>void)|undefined;
    if(desktop)void listenDesktop(desktopAction).then(stop=>{if(disposed)stop();else unlisten=stop;}).catch(e=>error=String(e));
    let publishing=false;const publisher=setInterval(()=>{if(!desktop||publishing)return;publishing=true;void publish().catch(()=>{}).finally(()=>publishing=false);},250);
    return () => {disposed=true;unlisten?.();clearInterval(publisher);player?.dispose();};
  });
  async function importLibrary() {
    busy = true; error = '';
    try { const imported = await importConfig(); if (imported) { saved(imported); active = null; } }
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
    <button class="brand" onclick={() => showView('Stations')} aria-label="Radio Replay home">◉ <span>RADIO<br/>REPLAY</span></button>
    <p class="eyebrow">YOUR FREQUENCIES</p>
    <nav>{#each ['Stations', 'Groups', 'Settings'] as item}<button class:selected={view === item} onclick={() => showView(item)}>{item}</button>{/each}</nav>
    <label class="group-selector">Station group<select bind:value={groupId} onchange={changeGroup}><option value="all">All stations</option>{#each groups as group}<option value={group.id}>{group.name}</option>{/each}</select></label>
    <div class="sidebar-note"><span class="signal">●</span> Local by design<br/><small>Rust preview · 0.1</small></div>
  </aside>
  <main>
    <header><label class="search"><span>⌕</span><input aria-label="Search stations" placeholder="Search your stations" bind:value={query}/></label><div class="actions"><button class="secondary" onclick={mini} disabled={!desktop}>Mini player</button><button onclick={()=>quitDesktop().catch(e=>error=String(e))} disabled={!desktop}>Quit</button></div></header>
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
      {#if active}<section class="details"><div class="actions"><h2>{active.station}</h2><button class="secondary" onclick={() => edit(active)} disabled={!desktop}>Edit station</button></div><p>{playback?.key === active.key ? playback.loading ? 'Tuning…' : displayTitle : 'Select this station to tune in.'}</p>{#if details?.tags.album}<small>{details.tags.album}</small>{/if}
      {#if textMode==='lyrics'&&timed.length}<div class="lyrics" aria-label="Synchronized lyrics">{#each lyricWindow as line,index}<p class:current={index+lyricStart===lineIndex}>{line.text}</p>{/each}</div>{/if}
      {#each active.tracks as track}<div class="track"><span>{track.name}</span><small>{track.path}</small></div>{/each}</section>{/if}
      {/if}
    {:else if view === 'Groups'}<Groups {config} onsaved={saved}/>
    {:else}<Settings {config} onsaved={preferencesSaved} oncachecleared={()=>player?.clearCache()}/>{/if}
  </main>
  <footer><div class="now-icon">◉</div><div class="now-playing"><strong>{playback?.station || 'Nothing tuned yet'}</strong><small>{displayTitle || 'Choose a station to tune in'}</small></div><div class="player-controls"><div class="transport"><button disabled={!desktop || !lineup.length} aria-label="Previous station" onclick={() => step(-1)}>|◁</button><button disabled={!playback?.key || playback.loading} aria-label={playback?.playing ? 'Pause' : 'Play'} onclick={toggle}>{playback?.playing ? '❚❚' : '▷'}</button><button disabled={!desktop || !lineup.length} aria-label="Next station" onclick={() => step(1)}>▷|</button></div>{#if config.settings.showProgress!==false}<div class="progress"><span>{time(playback?.seconds ?? 0)}</span><progress aria-label="Recording progress" max="1" value={playback?.duration ? playback.seconds / playback.duration : 0}></progress><span>{time(playback?.duration ?? 0)}</span></div>{/if}</div><div class="volume"><button aria-label={playback?.muted ? 'Unmute' : 'Mute'} onclick={() => player?.mute()} disabled={!desktop}>{playback?.muted ? 'Muted' : 'Volume'}</button><input aria-label="Volume" type="range" min="0" max="1" step="0.01" value={playback?.volume ?? .8} oninput={event => player?.setVolume(Number(event.currentTarget.value))} disabled={!desktop}/></div></footer>
</div>
