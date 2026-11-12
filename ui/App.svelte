<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import Toasts from './Toasts.svelte';
  import ProblemDetails from './ProblemDetails.svelte';
  import type {Notification} from './notifications';
  import { desktop, emptyConfig, loadConfig, importConfig, artworkUrl, audioUrl, prepareStation, rememberDuration, trackDetails, type TrackDetails, type Config, type Station } from './library';
  import Groups from './Groups.svelte';
  import Settings from './Settings.svelte';
  import { groupLineup,parseTimedText,currentLine,StationHistory } from './playback/text';
  import { Playback, type PlaybackState } from './playback/engine';
  import StationEditor from './StationEditor.svelte';
  import {listenDesktop,publishDesktop,openMini,quitDesktop,type DesktopAction,type DesktopSnapshot} from './desktop';
  import {resolveTuningSound} from './library';
  import bundledStatic from '../assets/audio/static.mp3?url';
  import StationBanner from './StationBanner.svelte';
  import RadioControls from './RadioControls.svelte';
  import ControlIcon from './ControlIcon.svelte';
  import TrackTitle from './TrackTitle.svelte';
  import ManageStations from './ManageStations.svelte';
  import {connectMediaSession,mediaKeyAction} from './media-session';
  import {stationAccent, sampleBanner} from './appearance';
  import type {Group} from './library';
  import {listenAudioDrops, type Track} from './library';
  let config = $state.raw(emptyConfig());
  let query = $state('');
  let active = $state<Station | null>(null);
  let busy = $state(true);
  let error = $state('');
  let notifications=$state<Notification[]>([]);
  let notificationDetails=$state<Notification|null>(null);
  let nextNotificationId=0;
  let lastPlaybackNotice=0;
  function notify(item:Omit<Notification,'id'>){notifications=[{...item,id:++nextNotificationId},...notifications.filter(existing=>existing.source!==item.source)].slice(0,3);}
  function dismissNotification(id:number){const item=notifications.find(item=>item.id===id);notifications=notifications.filter(item=>item.id!==id);if(item?.source==='app')error='';}
  $effect(()=>{const message=error;untrack(()=>{if(message)notify({source:'app',severity:'error',message,problems:[]});else notifications=notifications.filter(item=>item.source!=='app');});});
  $effect(()=>{const notice=playbackNotice;untrack(()=>{if(!notice){notifications=notifications.filter(item=>item.source!=='playback');return;}if(notice.id===lastPlaybackNotice)return;lastPlaybackNotice=notice.id;notify({source:'playback',severity:notice.severity,message:notice.message,stationKey:notice.stationKey,station:notice.station,problems:notice.problems});});});
  let view = $state('Stations');
  let editing = $state(false);
  let editTarget = $state<Station | null>(null);
  let player: Playback | null = null;
  let playback = $state.raw<PlaybackState | null>(null);
  const activeKey=$derived(playback?.key);
  const playbackNotice=$derived(playback?.notice);
  let groupId=$state('all');
  let history=new StationHistory();
  let details=$state<TrackDetails|null>(null);
  let managing=$state(false);
  let groupDraft=$state<Group|null|undefined>(undefined);
  let sampledAccent=$state('');
  let droppedTracks=$state<Track[]>([]);
  let readingDrops=$state(0);
  let editorSession=0;
  const dropTargets=new Map<number,number>();
  const bannerSamples=new Map<string,string>();
  let lastWheel=0;
  let browserMedia:ReturnType<typeof connectMediaSession>|undefined;
  const metadataKey=$derived(playback?.key??'');
  const metadataPath=$derived(playback?.path??'');
  const groups=$derived(config.groups.filter(g=>g&&typeof g.id==='string'&&Array.isArray(g.members)));
  const selectedGroup=$derived(groups.find(g=>g.id===groupId));
  const lineup = $derived(groupLineup(config.custom.filter(s => !config.hidden.includes(s.key)),selectedGroup));
  const timed=$derived(parseTimedText(details?.sidecar??null));
  const lineIndex=$derived(currentLine(timed,playback?.seconds??0));
  const playingStation=$derived(config.custom.find(s=>s.key===playback?.key));
  const textMode=$derived(['titles','lyrics','ignore'].includes(String(playingStation?.textMode))?String(playingStation?.textMode):playingStation?.mode==='longset'?'titles':'lyrics');
  const embeddedTitle=$derived(details?.tags.title?`${details.tags.artist?details.tags.artist+' - ':''}${details.tags.title}`:playback?.title??'');
  const displayTitle=$derived(textMode==='titles'&&lineIndex>=0?timed[lineIndex].text:embeddedTitle);
  $effect(()=>{ const key=metadataKey,path=metadataPath;details=null;let cancelled=false;if(key&&path)void trackDetails(key,path).then(value=>{if(!cancelled)details=value;}).catch(()=>{});return()=>{cancelled=true;}; });
  const bannerPath=$derived(artworkUrl(playingStation?.bannerPath));
  const accent=$derived(stationAccent(playingStation,config.settings,sampledAccent));
  $effect(()=>{const url=bannerPath;let cancelled=false;sampledAccent=bannerSamples.get(url)??'';if(url&&!bannerSamples.has(url))void sampleBanner(url).then(color=>{if(bannerSamples.size>=128)bannerSamples.delete(bannerSamples.keys().next().value!);bannerSamples.set(url,color);if(!cancelled)sampledAccent=color;}).catch(()=>{});return()=>{cancelled=true;};});
  $effect(()=>{document.documentElement.style.setProperty('--accent',accent);document.documentElement.style.setProperty('--green',accent);document.documentElement.dataset.density=String(config.settings.listDensity??'comfortable');document.documentElement.classList.toggle('reduce-motion',config.settings.reduceMotion===true);document.documentElement.classList.toggle('no-scanlines',config.settings.showScanlines===false);});
  function tune(station: Station,navigation=false,reveal=true) { if(!navigation)history.cancelPending();active = station;if(reveal)showView('Player');if(playback?.key===station.key&&(!playback.error||playback.notice?.severity==='warning'))return; const members=selectedGroup?new Set(selectedGroup.members):null; const projected=members&&!members.has(station.key)?{...station,tracks:station.tracks.filter(t=>members.has(`${station.key}|${t.path}`))}:station; if(desktop) void player?.tune(projected,config.settings,groupId); }
  function showView(next:string){view=next;void tick().then(()=>document.querySelector('.deck')?.scrollTo({top:0}));}
  function step(direction: number) {
    if(!lineup.length) return;
    if(config.settings.randomNext===true){const next=history.next(lineup,direction);if(next)tune(next,true);return;}
    const index = lineup.findIndex(s => s.key === playback?.key);
    tune(lineup[index<0?(direction>0?0:lineup.length-1):(index + direction + lineup.length) % lineup.length]);
  }
  function toggle() { if(playback?.playing) void player?.pause(); else void player?.play(); }
  function adjustVolume(delta:number){player?.setVolume((playback?.muted?0:(playback?.volume??.8))+delta);}
  async function keyboardStep(direction:number) {
    const followFocus=document.activeElement?.matches('.sidebar-station');
    step(direction);
    if(!followFocus)return;
    await tick();
    const selected=document.querySelector<HTMLElement>('.sidebar-station.active');
    const target=selected??document.querySelector<HTMLElement>('.back-stations');
    target?.focus({preventScroll:true});
    selected?.scrollIntoView({block:'nearest',inline:'nearest'});
  }
  function desktopAction(action:DesktopAction){switch(action.type){case'toggle':toggle();break;case'play':if(!playback?.playing)void player?.play();break;case'pause':if(playback?.playing)void player?.pause();break;case'previous':step(-1);break;case'next':step(1);break;case'mute':player?.mute();break;case'volume':if(Number.isFinite(action.value))player?.setVolume(action.value);break;}}
  async function mini(){try{await publish();await openMini();}catch(e){error=String(e);}}
  function publish(){if(!desktop)return Promise.resolve();const snapshot:DesktopSnapshot={station:playingStation?.station??'',title:textMode==='titles'?displayTitle:(details?.tags.title||playback?.title||''),artist:details?.tags.artist??'',album:details?.tags.album??'',art:artworkUrl(config.logos[playback?.key??'']),banner:bannerPath,bannerColor:typeof playingStation?.bannerColor==='string'?playingStation.bannerColor:'',accent,playing:playback?.playing??false,hasStation:!!playback?.key&&!playback.loading,canNavigate:lineup.length>0,seconds:playback?.seconds??0,duration:playback?.duration??0,volume:playback?.volume??.8,muted:playback?.muted??false};browserMedia?.update(snapshot);return publishDesktop(snapshot);}
  function edit(station: Station | null) { editorSession++;droppedTracks=[];editTarget = station; editing = true; }
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
  function preferencesSaved(next:Config){const changed=next.settings.crossfade!==config.settings.crossfade,wasPlaying=playback?.playing;if(changed)player?.invalidate();config=next;if(changed&&wasPlaying&&active)tune(active,false,false);}
  function changeGroup(){player?.stop();active=null;history=new StationHistory();showView('Stations');}
  function wheel(event:WheelEvent){if(Math.abs(event.deltaY)<12||event.ctrlKey||event.target instanceof Element&&event.target.closest('.volume-module,.lyrics,input,button'))return;event.preventDefault();if(Date.now()-lastWheel<650)return;lastWheel=Date.now();step(event.deltaY>0?1:-1);}
  const stations = $derived(lineup.filter(s => `${s.station} ${s.game??''}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())));
  const sections=$derived.by(()=>{const result=new Map<string,Station[]>();for(const station of stations){const heading=groupId==='all'?String(station.game??''):'Stations';if(!result.has(heading))result.set(heading,[]);result.get(heading)!.push(station);}return [...result];});
  function groupSaved(next:Config,id:string){saved(next);groupDraft=undefined;if(id!==groupId){groupId=id;changeGroup();}}
  onMount(() => {
    if(desktop)browserMedia=connectMediaSession(desktopAction);
    if(desktop) player = new Playback({prepare:key=>prepareStation(key,groupId),rememberDuration,url:audioUrl,tuningSource:async value=>{if(value===''||value==='off')return null;if(typeof value!=='string'||value==='bundled')return bundledStatic;const path=await resolveTuningSound();return path?audioUrl(path):null;},onState:state => {playback = state;if(state.playing&&!state.loading)history.commit(state.key);}});
    void (async () => { try { config = await loadConfig(); } catch (e) { error = String(e); } finally { busy = false; } })();
    let disposed=false;let unlisten:(()=>void)|undefined;
    let unlistenDrops:(()=>void)|undefined;
    if(desktop)void listenAudioDrops(id=>{readingDrops++;dropTargets.set(id,managing||groupDraft!==undefined?-1:editing?editorSession:0);},drop=>{readingDrops=Math.max(0,readingDrops-1);const target=dropTargets.get(drop.id);dropTargets.delete(drop.id);if(managing||groupDraft!==undefined||target===undefined||target<0||target===0&&editing||target>0&&(!editing||target!==editorSession))return;if(drop.error){error=drop.error;return;}if(target===0)edit(null);droppedTracks=drop.tracks;}).then(stop=>{if(disposed)stop();else unlistenDrops=stop;}).catch(e=>error=String(e));
    if(desktop)void listenDesktop(desktopAction).then(stop=>{if(disposed)stop();else unlisten=stop;}).catch(e=>error=String(e));
    let publishing=false;const publisher=setInterval(()=>{if(!desktop||publishing)return;publishing=true;void publish().catch(()=>{}).finally(()=>publishing=false);},250);
    return () => {disposed=true;unlisten?.();unlistenDrops?.();clearInterval(publisher);browserMedia?.dispose();player?.dispose();};
  });
  async function importLibrary() {
    busy = true; error = '';
    try { const imported = await importConfig(); if (imported) { player?.invalidate();saved(imported); active = null;showView('Stations'); } }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<svelte:window onkeydown={(event) => {
  const mediaAction=mediaKeyAction(event);if(mediaAction){event.preventDefault();desktopAction(mediaAction);return;}
  if(editing || managing || notificationDetails || groupDraft!==undefined || !['Stations','Player'].includes(view) || event.ctrlKey || event.metaKey || event.altKey || (event.target instanceof HTMLElement && event.target.closest('input,select,textarea'))) return;
  if(event.code==='Space'&&event.target instanceof HTMLElement&&event.target.closest('button'))return;
  if(event.code === 'Space') { event.preventDefault(); toggle(); }
  if(event.key === 'ArrowDown') { event.preventDefault(); void keyboardStep(1); }
  if(event.key === 'ArrowUp') { event.preventDefault(); void keyboardStep(-1); }
  if(event.key.toLowerCase() === 'm') player?.mute();
  if(event.key==='+'||event.key==='='){event.preventDefault();adjustVolume(.05);}
  if(event.key==='-'){event.preventDefault();adjustVolume(-.05);}
}}/>

<div class="shell">
  <main class="electron-shell">
    <aside>
      <button class="brand" onclick={()=>showView('Stations')} aria-label="Radio Replay home"><span class="signal">◉</span><span><b>RADIO REPLAY</b><small>LOCAL BROADCAST SYSTEM</small></span></button>
      <button class="choose" onclick={()=>edit(null)} disabled={busy||!desktop}>＋ CREATE STATION</button>
      <button class="choose secondary" onclick={()=>managing=true} disabled={busy||!desktop}>MANAGE STATIONS</button>
      <div class="sidebar-meta"><button class="config-action" onclick={()=>showView('Settings')}>⚙ Settings</button><div class="library-path">{busy?'Loading stations…':`${config.custom.length} station${config.custom.length===1?'':'s'}`}</div></div>
      <div class="groupbar group-selector">
        <select aria-label="Station group" bind:value={groupId} onchange={changeGroup}><option value="all">All stations</option>{#each groups as group}<option value={group.id}>{group.name}</option>{/each}</select>
        <button aria-label="Create group" title="Create group" onclick={()=>groupDraft=null} disabled={!desktop}>＋</button>
        <button aria-label="Edit group" title="Edit group" onclick={()=>groupDraft=selectedGroup} disabled={!selectedGroup||!desktop}>✎</button>
      </div>
      <input id="stationSearch" type="search" aria-label="Search stations" placeholder="Search stations…" bind:value={query}/>
      <nav id="stationList" aria-label="Radio stations">
        {#each sections as [heading,list]}<section class="game">{#if heading}<h2>{heading}</h2>{/if}{#each list as station}<button class="station sidebar-station" class:active={activeKey===station.key} aria-current={activeKey===station.key?'true':undefined} onclick={()=>tune(station)}><span>{station.station}</span><small>{station.mode==='tracks'?`${station.tracks.length} track${station.tracks.length===1?'':'s'}`:'broadcast'}</small></button>{/each}</section>{:else}<div class="empty">{busy?'Loading stations…':'No stations in this group.'}</div>{/each}
      </nav>
    </aside>
    <section class="deck">
      {#if readingDrops}<div class="notice" role="status">Reading dropped audio…</div>{/if}
      {#if !desktop}<div class="notice">Browser preview. Open the desktop app to import your library.</div>{/if}
      {#if view==='Stations'}
        <section id="homeView">
          <p class="home-kicker">YOUR RADIO</p><h1>Choose a station</h1><p id="homeSummary">{stations.length} station{stations.length===1?'':'s'} available</p>
          <div id="homeStations">{#each stations as station}<button class="home-station" class:active={activeKey===station.key} onclick={()=>tune(station)}>
            {#if artworkUrl(config.logos[station.key])}<img src={artworkUrl(config.logos[station.key])} alt="" loading="lazy" decoding="async"/>{:else}<span class="home-icon">◉</span>{/if}
            <span>{#if station.game}<small>{String(station.game)}</small>{/if}<strong>{station.station}</strong><em>{station.mode==='tracks'?`${station.tracks.length} track${station.tracks.length===1?'':'s'}`:'Continuous broadcast'}</em></span>
          </button>{:else}<p class="empty">{query?'No matching stations.':'No stations yet. Create one to get started.'}</p>{/each}</div>
        </section>
      {:else if view==='Player'&&active}
        <div id="playerView" class="radio-screen" onwheel={wheel}>
          <header><button class="back-stations" onclick={()=>showView('Stations')}>← All Stations</button><div class="player-tools"><span class="hint">↑ ↓ TO TUNE</span><button class="mini-action" aria-label="Mini player" title="Mini player" onclick={mini} disabled={!desktop}><ControlIcon name="pip"/></button></div></header>
          <div class="now-playing">
            <StationBanner station={playingStation??active} logo={config.logos[active.key]} number={Math.max(1,lineup.findIndex(s=>s.key===active?.key)+1)} playing={playback?.playing} layout={config.settings.bannerLayout??'row'}/>
            <button class="logo-action edit-link" onclick={()=>edit(active)}>Edit station</button>
            <section class="now"><div class="now-content">
              <div class="on-air"><i></i>{playback?.loading?'TUNING':playback?.playing?'ON AIR':'PAUSED'}</div>
              <TrackTitle text={displayTitle||'Tuning…'} speed={Number(config.settings.bounceSpeed)||25} reduced={config.settings.reduceMotion===true}/>
              {#if details?.tags.album}<div class="track-album">{details.tags.album}</div>{/if}
              {#if textMode==='lyrics'&&lineIndex>=0}<div class="metadata lyrics" aria-label="Synchronized lyrics"><span class="current">{timed[lineIndex].text}</span></div>{/if}
            </div></section>
          </div>
          <RadioControls {playback} enabled={desktop&&lineup.length>0} showProgress={config.settings.showProgress!==false} {toggle} {step} mute={()=>player?.mute()} volume={value=>player?.setVolume(value)}/>
        </div>
      {:else}<Settings {config} onsaved={preferencesSaved} oncachecleared={()=>player?.clearCache()} onback={()=>showView(active?'Player':'Stations')} onerror={message=>error=message}/>{/if}
    </section>
  {#if view==='Stations' && playback?.key}<footer class="compact-player">
    <button class="compact-title" onclick={()=>showView(active?'Player':'Stations')}><span class="now-icon">{#if artworkUrl(config.logos[playback?.key??''])}<img src={artworkUrl(config.logos[playback?.key??''])} alt=""/>{:else}◉{/if}</span><span><strong>{playingStation?.station??'Nothing tuned yet'}</strong><small>{displayTitle||'Choose a station to tune in'}</small></span></button>
    <RadioControls {playback} enabled={desktop&&lineup.length>0} showProgress={config.settings.showProgress!==false} {toggle} {step} mute={()=>player?.mute()} volume={value=>player?.setVolume(value)} compact/>
  </footer>{/if}
  </main>
</div>
<Toasts items={notifications} raised={view==='Stations'&&!!playback?.key} onview={item=>notificationDetails=item} ondismiss={dismissNotification}/>
{#if notificationDetails}<ProblemDetails notification={notificationDetails} oncancel={()=>notificationDetails=null} ondismiss={()=>{dismissNotification(notificationDetails!.id);notificationDetails=null;}} onedit={config.custom.some(station=>station.key===notificationDetails?.stationKey)?()=>{const station=config.custom.find(station=>station.key===notificationDetails?.stationKey);notificationDetails=null;if(station)edit(station);}:undefined}/>{/if}
{#if editing}{#key editTarget?.key}<StationEditor station={editTarget} logo={editTarget?config.logos[editTarget.key]:''} settings={config.settings} incoming={droppedTracks} onsaved={saved} oncancel={()=>editing=false}/>{/key}{/if}
{#if groupDraft!==undefined}<Groups {config} group={groupDraft} onsaved={groupSaved} oncancel={()=>groupDraft=undefined}/>{/if}
{#if managing}<ManageStations {config} onimport={()=>{managing=false;void importLibrary();}} onsaved={next=>{saved(next);managing=false;}} onedit={station=>{managing=false;edit(station);}} oncancel={()=>managing=false}/>{/if}
