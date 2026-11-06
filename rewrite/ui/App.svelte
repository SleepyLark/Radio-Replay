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
  import StationBanner from './StationBanner.svelte';
  import RadioControls from './RadioControls.svelte';
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
  let managing=$state(false);
  let lastWheel=0;
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
  const accent=$derived(typeof playingStation?.accent==='string'&&/^#[0-9a-f]{6}$/i.test(playingStation.accent)?playingStation.accent:typeof config.settings.accent==='string'?config.settings.accent:'#c7f464');
  $effect(()=>{ const current=lineIndex, mode=textMode, visible=view==='Player'&&!editing, reduced=config.settings.reduceMotion===true;let cancelled=false; if(visible&&mode==='lyrics'&&current>=0)void tick().then(()=>{if(!cancelled)document.querySelector('.lyrics .current')?.scrollIntoView({block:'nearest',behavior:reduced?'auto':'smooth'});});return()=>{cancelled=true;}; });
  $effect(()=>{document.documentElement.style.setProperty('--accent',accent);document.documentElement.dataset.density=String(config.settings.listDensity??'comfortable');document.documentElement.classList.toggle('reduce-motion',config.settings.reduceMotion===true);});
  function tune(station: Station,navigation=false) { if(!navigation)history.cancelPending();active = station;showView('Player');if(playback?.key===station.key&&!playback.error)return; const members=selectedGroup?new Set(selectedGroup.members):null; const projected=members&&!members.has(station.key)?{...station,tracks:station.tracks.filter(t=>members.has(`${station.key}|${t.path}`))}:station; if(desktop) void player?.tune(projected,config.settings,groupId); }
  function showView(next:string){view=next;void tick().then(()=>document.querySelector('main')?.scrollTo({top:0}));}
  function step(direction: number) {
    if(!lineup.length) return;
    if(config.settings.randomNext===true){const next=history.next(lineup,direction);if(next)tune(next,true);return;}
    const index = lineup.findIndex(s => s.key === playback?.key);
    tune(lineup[index<0?(direction>0?0:lineup.length-1):(index + direction + lineup.length) % lineup.length]);
  }
  function toggle() { if(playback?.playing) void player?.pause(); else void player?.play(); }
  function desktopAction(action:DesktopAction){switch(action.type){case'toggle':toggle();break;case'play':if(!playback?.playing)void player?.play();break;case'pause':if(playback?.playing)void player?.pause();break;case'previous':step(-1);break;case'next':step(1);break;case'mute':player?.mute();break;case'volume':if(Number.isFinite(action.value))player?.setVolume(action.value);break;}}
  async function mini(){try{await publish();await openMini();}catch(e){error=String(e);}}
  function publish(){if(!desktop)return Promise.resolve();const snapshot:DesktopSnapshot={station:playingStation?.station??'',title:textMode==='titles'?displayTitle:(details?.tags.title||playback?.title||''),artist:details?.tags.artist??'',album:details?.tags.album??'',art:artworkUrl(config.logos[playback?.key??'']),accent,playing:playback?.playing??false,hasStation:!!playback?.key&&!playback.loading,canNavigate:lineup.length>0,seconds:playback?.seconds??0,duration:playback?.duration??0,volume:playback?.volume??.8,muted:playback?.muted??false};return publishDesktop(snapshot);}
  function edit(station: Station | null) { editTarget = station; editing = true; }
  function saved(next: Config) {
    const wasPlaying=playback?.playing;
    const before=config.custom.find(s=>s.key===playback?.key), after=next.custom.find(s=>s.key===playback?.key);
    const changed=before && (!after || (before.mode??'longset')!==(after.mode??'longset') || (before.crossfade??null)!==(after.crossfade??null) || JSON.stringify(before.tracks.map(t=>t.path))!==JSON.stringify(after.tracks.map(t=>t.path)) || JSON.stringify(config.groups.find(g=>g.id===groupId))!==JSON.stringify(next.groups.find(g=>g.id===groupId)));
    if(changed){player?.invalidate();history=new StationHistory();}
    config=next;active=next.custom.find(s=>s.key===active?.key)??null;editing=false;
    if(!active&&view==='Player')showView('Stations');
    if(groupId!=='all'&&!next.groups.some(g=>g&&g.id===groupId))groupId='all';
    if(changed&&active){if(wasPlaying&&groupLineup(next.custom, next.groups.find(g=>g.id===groupId)).some(s=>s.key===active?.key))tune(active);else if(view==='Player')showView('Stations');}
  }
  function preferencesSaved(next:Config){const changed=next.settings.crossfade!==config.settings.crossfade,wasPlaying=playback?.playing;if(changed)player?.invalidate();config=next;if(changed&&wasPlaying&&active)tune(active);}
  function changeGroup(){player?.stop();active=null;history=new StationHistory();showView('Stations');}
  function wheel(event:WheelEvent){if(Math.abs(event.deltaY)<12||event.ctrlKey||event.target instanceof HTMLElement&&event.target.closest('.lyrics,input,button'))return;event.preventDefault();if(Date.now()-lastWheel<650)return;lastWheel=Date.now();step(event.deltaY>0?1:-1);}
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
    try { const imported = await importConfig(); if (imported) { player?.invalidate();saved(imported); active = null;showView('Stations'); } }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<svelte:window onkeydown={(event) => {
  if(editing || managing || !['Stations','Player'].includes(view) || event.ctrlKey || event.metaKey || event.altKey || (event.target instanceof HTMLElement && event.target.closest('input,select,textarea,button'))) return;
  if(event.code === 'Space') { event.preventDefault(); toggle(); }
  if(event.key === 'ArrowDown') { event.preventDefault(); step(1); }
  if(event.key === 'ArrowUp') { event.preventDefault(); step(-1); }
  if(event.key.toLowerCase() === 'm') player?.mute();
  if(event.key==='+'||event.key==='='){event.preventDefault();player?.setVolume((playback?.volume??.8)+.05);}
  if(event.key==='-'){event.preventDefault();player?.setVolume((playback?.volume??.8)-.05);}
}}/>

<div class="shell">
  <aside>
    <button class="brand" onclick={() => showView('Stations')} aria-label="Radio Replay home">◉ <span>RADIO REPLAY</span></button>
    <p class="brand-subtitle">LOCAL BROADCAST SYSTEM</p>
    <button class="create-button secondary" onclick={()=>edit(null)} disabled={busy||!desktop}>＋ Create station</button>
    <button class="secondary manage-button" onclick={()=>managing=true} disabled={busy||!desktop}>Manage stations</button>
    <nav>{#each ['Stations', 'Groups', 'Settings'] as item}<button class:selected={view === item} onclick={() => showView(item)}>{item}</button>{/each}</nav>
    <label class="group-selector">Station group<select bind:value={groupId} onchange={changeGroup}><option value="all">All stations</option>{#each groups as group}<option value={group.id}>{group.name}</option>{/each}</select></label>
    <label class="search"><span>⌕</span><input aria-label="Search stations" placeholder="Search stations…" bind:value={query}/></label>
    <div class="station-count">{stations.length} FREQUENCIES</div>
    <div class="station-list" aria-label="Station lineup">{#each stations as station}<button class="sidebar-station" class:selected={playback?.key===station.key} aria-current={playback?.key===station.key?'true':undefined} onclick={()=>tune(station)}><span>{station.station}</span><small>{station.mode==='tracks'?`${station.tracks.length} tracks`:'broadcast'}</small></button>{/each}</div>
    <div class="sidebar-note"><span class="signal">●</span> LOCAL SIGNAL <span>RUST PREVIEW</span></div>
  </aside>
  <main>
    <header class="topbar"><span class="eyebrow">YOUR RADIO / {view.toUpperCase()}</span><div class="actions"><button onclick={mini} disabled={!desktop}>↗ Mini player</button><button onclick={()=>quitDesktop().catch(e=>error=String(e))} disabled={!desktop}>Quit</button></div></header>
    {#if error}<div class="error" role="alert">{error}</div>{/if}
    {#if playback?.error}<div class="notice" role="status">{playback.error}</div>{/if}
    {#if !desktop}<div class="notice">Browser preview. Open the desktop app to import your library.</div>{/if}
    {#if view === 'Stations'}
      <section class="heading home-heading"><div><p class="eyebrow">YOUR RADIO</p><h1>CHOOSE A STATION</h1><p>{selectedGroup?.name??'Your collection'}, on your frequency.</p></div><span class="badge">{lineup.length} ON THE DIAL</span></section>
      {#if busy}<p role="status">Loading library…</p>
      {:else if stations.length}
        <div class="grid">{#each stations as station,index}<button class="station" class:active={playback?.key === station.key} onclick={() => tune(station)}><div class="art">{#if artworkUrl(config.logos[station.key])}<img src={artworkUrl(config.logos[station.key])} alt="" />{:else}◉{/if}</div><div class="card-copy"><strong>{station.station}</strong><small>{station.tracks.length} tracks · {station.mode === 'tracks' ? 'Shuffle program' : 'Radio broadcast'}</small></div><span class="card-number">{String(index+1).padStart(2,'0')}</span></button>{/each}</div>
      {:else}<section class="empty"><div class="dial">◎</div><h2>{query?'NO MATCHING FREQUENCIES':'A FRESH FREQUENCY'}</h2><p>{query?'Try a different station name.':'Create a station or import your existing Radio Replay library.'}</p>{#if !query}<button class="primary" onclick={()=>edit(null)} disabled={!desktop}>Create station</button>{/if}</section>{/if}
      <div class="library-bottom"><span>LOCAL FILES. ENDLESS BROADCAST.</span><button onclick={importLibrary} disabled={busy||!desktop}>Import stations.json ↗</button></div>
    {:else if view==='Player' && active}
      <section class="radio-screen" aria-label="Radio player" onwheel={wheel}>
        <div class="player-navigation"><button onclick={()=>showView('Stations')}>← All Stations</button><small>↑ ↓ TO TUNE</small></div>
        <div class="now-playing"><StationBanner station={playingStation??active} logo={config.logos[active.key]} number={Math.max(1,lineup.findIndex(s=>s.key===active?.key)+1)} playing={playback?.playing}/>
          <button class="edit-link" onclick={()=>edit(active)}>✎ Edit station</button>
          <div class="track-display"><span class="on-air"><i class:live={playback?.playing}></i>{playback?.loading?'TUNING':playback?.playing?'ON AIR':'PAUSED'}</span><h2>{displayTitle||'Tuning in…'}</h2>{#if details?.tags.album}<p>{details.tags.album}</p>{/if}</div>
        </div>
        <RadioControls {playback} enabled={desktop&&lineup.length>0} showProgress={config.settings.showProgress!==false} {toggle} {step} mute={()=>player?.mute()} volume={value=>player?.setVolume(value)}/>
        {#if textMode==='lyrics'&&timed.length}<div class="lyrics" aria-label="Synchronized lyrics">{#each lyricWindow as line,index}<p class:current={index+lyricStart===lineIndex}>{line.text}</p>{/each}</div>{/if}
        <p class="keyboard-hint">SPACE PLAY / PAUSE <span>↑ ↓ STATIONS</span><span>＋ − VOLUME</span><span>M MUTE</span></p>
      </section>
    {:else if view === 'Groups'}<Groups {config} onsaved={saved}/>
    {:else}<Settings {config} onsaved={preferencesSaved} oncachecleared={()=>player?.clearCache()}/>{/if}
  </main>
  {#if view!=='Player'&&playback?.key}<footer class="compact-player"><button class="compact-title" onclick={()=>showView('Player')}><strong>{playingStation?.station??playback.station}</strong><small>{displayTitle}</small></button><RadioControls {playback} enabled={desktop&&lineup.length>0} showProgress={config.settings.showProgress!==false} {toggle} {step} mute={()=>player?.mute()} volume={value=>player?.setVolume(value)}/></footer>{/if}
</div>
{#if editing}{#key editTarget?.key}<StationEditor station={editTarget} logo={editTarget ? config.logos[editTarget.key] : ''} onsaved={saved} oncancel={() => editing = false}/>{/key}{/if}
{#if managing}<dialog class="manage-dialog" use:openDialog onclose={()=>managing=false}><div class="modal-heading"><div><p class="eyebrow">YOUR LIBRARY</p><h2>MANAGE STATIONS</h2></div><button aria-label="Close station manager" onclick={()=>managing=false}>✕</button></div><p>Edit a station’s files, artwork, or playback style.</p><div class="manage-list">{#each config.custom as station}<button onclick={()=>{managing=false;edit(station);}}><span>{station.station}<small>{station.tracks.length} tracks</small></span><span>Edit ↗</span></button>{/each}</div><button class="secondary" onclick={()=>{managing=false;void importLibrary();}}>Import stations.json</button></dialog>{/if}

<script lang="ts" module>
  function openDialog(node:HTMLDialogElement){node.showModal();return {destroy(){node.close();}};}
</script>
