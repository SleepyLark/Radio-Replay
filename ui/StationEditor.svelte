<script lang="ts">
  import {untrack} from 'svelte';
  import Dialog from './Dialog.svelte';
  import StationBanner from './StationBanner.svelte';
  import FileList from './FileList.svelte';
  import {moveFolder} from './playlist';
  import {sampleBanner} from './appearance';
  import {fadeSeconds} from './playback/timeline';
  import {chooseAudio, chooseArtwork, chooseBumperFolder, saveStation, deleteStation, artworkUrl, type Config, type Station, type Track} from './library';
  let {station,logo,settings,incoming,onsaved,oncancel}: {station:Station|null; logo:unknown; settings:Record<string,unknown>; incoming:Track[]; onsaved:(config:Config)=>void; oncancel:()=>void}=$props();
  const initial=untrack(()=>({station,logo,settings}));
  const key=initial.station?.key??crypto.randomUUID();
  let name=$state(initial.station?.station??'');
  let mode=$state(initial.station?.mode==='tracks'?'tracks':initial.station?'longset':'tracks');
  let tracks=$state.raw<Track[]>(initial.station?[...initial.station.tracks]:[]);
  let logoPath=$state(typeof initial.logo==='string'?initial.logo:'');
  let bannerPath=$state(String(initial.station?.bannerPath??'')), bannerAccent=$state(String(initial.station?.bannerAccent??''));
  let useColor=$state(!!initial.station?.bannerColor), bannerColor=$state(String(initial.station?.bannerColor??'#244b42'));
  let useAccent=$state(!!initial.station?.accent), accent=$state(String(initial.station?.accent??initial.settings.accent??'#c7f464'));
  let spinLogo=$state(initial.station?.spinLogo===true);
  let textMode=$state(['titles','lyrics','ignore'].includes(String(initial.station?.textMode))?String(initial.station?.textMode):'auto');
  let customFade=$state(initial.station?.crossfade!=null), fade=$state(initial.station?fadeSeconds(initial.station,initial.settings)*1000:5000);
  let busy=$state(false), error=$state(''), confirmDelete=$state(false);
  const originalBumpers=untrack(()=>station?.bumpers);
  let bumperEnabled=$state(originalBumpers?.enabled===true), bumperFolder=$state(originalBumpers?.folder??'');
  let bumperMethod=$state<'songs'|'time'>(originalBumpers?.method==='time'?'time':'songs');
  let bumperMin=$state<number|undefined>(originalBumpers?.min??2), bumperMax=$state<number|undefined>(originalBumpers?.max??5);
  const invalidBumperRange=$derived(bumperMin==null||bumperMax==null||!Number.isFinite(bumperMin)||!Number.isFinite(bumperMax)||bumperMin>bumperMax||bumperMin<(bumperMethod==='songs'?1:.1)||bumperMax>(bumperMethod==='songs'?1000:360)||bumperMethod==='songs'&&(!Number.isInteger(bumperMin)||!Number.isInteger(bumperMax)));
  const invalidBumpers=$derived(mode==='tracks'&&bumperEnabled&&(!bumperFolder||invalidBumperRange));
  const preview=$derived({key,station:name,mode,tracks,bannerPath,bannerColor:useColor?bannerColor:'',spinLogo});
  let lastIncoming:Track[]|undefined;
  function merge(picked:Track[]){const paths=new Set(tracks.map(t=>t.path));tracks=[...tracks,...picked.filter(t=>{if(paths.has(t.path))return false;paths.add(t.path);return true;})];}
  $effect(()=>{const picked=incoming;if(picked===lastIncoming)return;lastIncoming=picked;if(picked.length)untrack(()=>merge(picked));});
  async function perform(action:()=>Promise<void>){busy=true;error='';try{await action();}catch(e){error=String(e);}finally{busy=false;}}
  async function add(folder:boolean){await perform(async()=>merge(await chooseAudio(folder)));}
  function move(index:number,direction:number){const next=[...tracks];[next[index],next[index+direction]]=[next[index+direction],next[index]];tracks=next;}
  function removeTracks(paths:string[]){const removed=new Set(paths);tracks=tracks.filter(track=>!removed.has(track.path));}
  function reorderFolder(folder:string,target:string|undefined,position:'before'|'after'|'first'){tracks=moveFolder(tracks,folder,target,position);}
  async function banner(){await perform(async()=>{const path=await chooseArtwork();if(!path)return;bannerPath=path;bannerAccent='';try{bannerAccent=await sampleBanner(artworkUrl(path));}catch{/* A failed sample keeps the selected image and default accent. */}});}
  async function save(){await perform(async()=>onsaved(await saveStation($state.snapshot({key,name,mode,tracks,logoPath,options:{textMode,crossfade:customFade?fade:null,bannerPath,bannerAccent,bannerColor:useColor?bannerColor:'',accent:useAccent?accent:'',spinLogo,bumpers:mode==='tracks'&&(bumperEnabled||bumperFolder)?{enabled:bumperEnabled,folder:bumperFolder,method:bumperMethod,min:invalidBumperRange?(bumperMethod==='songs'?2:10):bumperMin!,max:invalidBumperRange?(bumperMethod==='songs'?5:15):bumperMax!}:null}}))));}
</script>

<Dialog title={station?'Edit station':'Create station'} {oncancel} {busy} onsubmit={()=>void save()}>
  <StationBanner station={preview} logo={logoPath} layout={settings.bannerLayout??'row'} preview/>
  <fieldset disabled={busy}>
    <details class="editor-section" open><summary>Essentials</summary>
      <label>Station name<input required maxlength="200" bind:value={name} placeholder="Your station name"/></label>
      <p>Playback style</p><div class="preset-actions"><button type="button" aria-pressed={mode==='tracks'} onclick={()=>mode='tracks'}>Music station</button><button type="button" aria-pressed={mode==='longset'} onclick={()=>mode='longset'}>Radio broadcast</button></div>
      <p id="playbackHelp">{mode==='tracks'?'Files play in a shuffled order. Timestamps supply lyrics by default.':'Recordings play in order. Timestamps supply track titles by default.'}</p>
      <div class="file-actions"><button type="button" onclick={()=>add(false)}>Add files</button><button type="button" onclick={()=>add(true)}>Add folder</button></div>
      <p>{tracks.length} file{tracks.length===1?'':'s'}</p>{#if mode==='longset'}<p id="fileOrderHelp">Numbers show the broadcast order. Use ↑ / ↓ to move files or entire folders, or ⇈ First to move a folder to the beginning.</p>{/if}
      <FileList {tracks} reorder={mode==='longset'} onmove={move} onmovefolder={reorderFolder} onremove={removeTracks}/>
    </details>
    {#if mode==='tracks'}
    <details class="editor-section" open={bumperEnabled}><summary>Bumpers</summary>
      <label class="editor-check"><input type="checkbox" bind:checked={bumperEnabled}/>Enable bumper clips between songs</label>
      {#if bumperEnabled}
      <p>{bumperFolder||'No folder selected'}</p>
      <button type="button" onclick={()=>perform(async()=>{const folder=await chooseBumperFolder();if(folder)bumperFolder=folder;})}>Choose bumper folder</button>
      <label>Interval method<select value={bumperMethod} onchange={event=>{bumperMethod=event.currentTarget.value as 'songs'|'time';bumperMin=bumperMethod==='songs'?2:10;bumperMax=bumperMethod==='songs'?5:15;}}><option value="songs">Song count</option><option value="time">Listening time</option></select></label>
      <label class="inline-number">Minimum {bumperMethod==='songs'?'songs':'minutes'}<input type="number" required min={bumperMethod==='songs'?1:.1} max={bumperMethod==='songs'?1000:360} step={bumperMethod==='songs'?1:.1} bind:value={bumperMin}/></label>
      <label class="inline-number">Maximum {bumperMethod==='songs'?'songs':'minutes'}<input type="number" required min={bumperMethod==='songs'?1:.1} max={bumperMethod==='songs'?1000:360} step={bumperMethod==='songs'?1:.1} bind:value={bumperMax}/></label>
      <p>{bumperMethod==='songs'?'A new song count is chosen after each bumper.':'A new approximate time target is chosen after each bumper. The closest song boundary is used, so the actual interval may fall outside this range.'} Bumper duration is excluded. Set both values equal for a fixed target.</p>
      {#if invalidBumpers}<p role="status">Choose a folder and a valid minimum/maximum range.</p>{/if}
      {/if}
    </details>
    {/if}
    <details class="editor-section"><summary>Appearance</summary>
      <div class="editor-section"><strong>Station logo</strong><p>Set a PNG, JPG, WebP, or animated GIF for the station.</p><p>{logoPath||'None'}</p><div class="file-actions"><button type="button" onclick={()=>perform(async()=>{const path=await chooseArtwork();if(path)logoPath=path;})}>Choose logo</button><button type="button" disabled={!logoPath} onclick={()=>logoPath=''}>Remove logo</button></div><label class="editor-check"><input type="checkbox" bind:checked={spinLogo}/>Spin logo while playing</label></div>
      <div class="editor-section"><strong>Station banner</strong><p>Use an image or color across the radio display behind the station name.</p><p>{bannerPath||'No banner image'}</p><div class="file-actions"><button type="button" onclick={banner}>Choose banner image</button><button type="button" disabled={!bannerPath} onclick={()=>{bannerPath='';bannerAccent='';}}>Remove image</button></div><label class="editor-check"><input type="checkbox" bind:checked={useColor}/>Use a banner color</label><label class="inline-color">Banner color<input type="color" bind:value={bannerColor} disabled={!useColor}/></label></div>
      <label class="editor-check"><input type="checkbox" bind:checked={useAccent}/>Override player accent</label><label class="inline-color">Station accent<input type="color" bind:value={accent} disabled={!useAccent}/></label><p>Unchecked: follows Settings → Appearance.</p>
    </details>
    <details class="editor-section"><summary>Advanced</summary><label>Use timestamped text as<select bind:value={textMode}><option value="auto">Default from playback style</option><option value="titles">Track titles</option><option value="lyrics">Lyrics</option><option value="ignore">Ignore sidecars</option></select></label><p>Only text with valid timestamps is displayed. Untimed notes stay out of the player.</p><label class="editor-check"><input type="checkbox" bind:checked={customFade}/>Override station crossfade</label>{#if customFade}<label class="inline-number">Station crossfade<input type="number" min="0" max="30000" step="100" required bind:value={fade}/> ms</label>{/if}</details>
    {#if station}<button type="button" class="danger" onclick={()=>confirmDelete=true}>Delete station</button>{/if}
    {#if confirmDelete}<p class="error">Remove this station? Audio and artwork files stay on disk.</p><button type="button" class="danger" onclick={()=>perform(async()=>onsaved(await deleteStation(key)))}>Delete station permanently</button>{/if}
  </fieldset>
  {#if error}<p class="error" role="alert">{error}</p>{/if}{#if busy}<p role="status">Working…</p>{/if}
  {#snippet footer()}<button type="button" disabled={busy} onclick={oncancel}>Cancel</button><button type="submit" class="primary" disabled={busy||!name.trim()||!tracks.length||invalidBumpers}>Save station</button>{/snippet}
</Dialog>
