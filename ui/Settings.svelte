<script lang="ts">
  import {untrack, onDestroy} from 'svelte';
  import {desktop, saveSettings, clearMetadataCache, chooseAudio, recoverLegacyLibrary, showConfig,exportConfig, type Config} from './library';
  import {fadeSeconds} from './playback/timeline';
  import {quitDesktop} from './desktop';
  import type {LibraryUpdate} from './library-update';
  import ScanNoticesView from './ScanNotices.svelte';
  import type {ScanNotices} from './library';
  let {config,onsaved,onrecovered,onpreview,oncachecleared,onback,onerror}: {config:Config; onsaved:(change:LibraryUpdate)=>void; onrecovered:(config:Config)=>void; onpreview:(color:string|null)=>void; oncachecleared:()=>void; onback:()=>void; onerror:(message:string)=>void}=$props();
  const initial=untrack(()=>config.settings);
  let category=$state('General'), status=$state(''), error=$state(''), busy=$state(false);
  let scanReport=$state<ScanNotices|null>(null);
  let accent=$state(String(initial.accent??'#c7f464'));
  let fade=$state(fadeSeconds({key:'',station:'',tracks:[],mode:'tracks'},initial)*1000);
  let randomNext=$state(initial.randomNext===true), showProgress=$state(initial.showProgress!==false), reduceMotion=$state(initial.reduceMotion===true), closeToTray=$state(initial.closeToTray===true);
  let density=$state(initial.listDensity==='compact'?'compact':'comfortable'), layout=$state(initial.bannerLayout==='column'?'column':'row');
  let scanlines=$state(initial.showScanlines!==false), matchBanner=$state(initial.matchBannerAccent!==false), bounceSpeed=$state(Number(initial.bounceSpeed)||25);
  let tuningTransition=$state(initial.tuningTransition!==false), tuningMinMs=$state(Number(initial.tuningMinMs)||0), tuningMaxMs=$state(Number(initial.tuningMaxMs??1500)), staticPath=$state(initial.staticPath==='off'?'':String(initial.staticPath??'bundled'));
  let queued:Record<string,unknown>={}, saving=$state(false), timer:ReturnType<typeof setTimeout>|undefined;
  let inFlight:Promise<boolean>|undefined;
  let destroyed=false;
  function queue(patch:Record<string,unknown>){Object.assign(queued,patch);clearTimeout(timer);status='Saving…';error='';timer=setTimeout(()=>void flush(),180);}
  function flush():Promise<boolean>{
    clearTimeout(timer);if(inFlight)return inFlight;if(!Object.keys(queued).length)return Promise.resolve(true);
    inFlight=(async()=>{
      saving=true;
      try {
        while(Object.keys(queued).length){
          const patch=queued;queued={};status='Saving…';
          try{const next=await saveSettings(patch);onsaved(next);if(!destroyed&&patch.accent===accent)onpreview(null);onerror('');error='';status='Saved';}
          catch(e){queued={...patch,...queued};error=String(e);onerror(error);status='Changes not saved';return false;}
        }
        return true;
      }finally{saving=false;}
    })().finally(()=>{inFlight=undefined;});
    return inFlight;
  }
  onDestroy(()=>{destroyed=true;onpreview(null);void flush();});
  async function leave(){if(await flush())onback();}
  async function quit(){try{if(await flush())await quitDesktop();}catch(e){error=String(e);onerror(error);}}
  function range(){if(tuningMinMs>tuningMaxMs){error='Minimum transition cannot exceed maximum.';return;}queue({tuningMinMs,tuningMaxMs});}
  async function chooseStatic(){busy=true;try{const report=await chooseAudio(false);if(report){scanReport=report.notices;if(report.tracks[0]){staticPath=report.tracks[0].path;queue({staticPath});}}}catch(e){error=String(e);}finally{busy=false;}}
  async function clear(){busy=true;error='';try{await clearMetadataCache();oncachecleared();status='Metadata and duration cache cleared.';}catch(e){error=String(e);}finally{busy=false;}}
  async function recover(){busy=true;error='';try{if(!await flush())return;const next=await recoverLegacyLibrary();onrecovered(next.config);scanReport=next.notices;status='Archive stations recovered.';}catch(e){error=String(e);}finally{busy=false;}}
  async function exportLibrary(){busy=true;error='';try{if(!await flush())return;if(await exportConfig())status='Library exported.';}catch(e){error=String(e);}finally{busy=false;}}
  async function showLibrary(){busy=true;try{if(await flush())await showConfig();}catch(e){error=String(e);}finally{busy=false;}}
</script>

<section id="settingsView">
  <p class="home-kicker">PREFERENCES</p><h1>Settings</h1>
  <nav class="settings-tabs" aria-label="Settings categories">{#each ['General','Appearance','Playback','Advanced'] as item}<button type="button" aria-pressed={category===item} onclick={()=>category=item}>{item}</button>{/each}</nav>
  <fieldset disabled={busy||!desktop}>
    <section data-settings-panel="general" hidden={category!=='General'}><h2>Your radio, your way</h2><p>Create a Music station for shuffled songs, or a Radio broadcast for recordings that play in order. Each station can override the defaults in its editor.</p><p>↑ / ↓ tune stations · Space plays or pauses · + / − adjust volume · M mutes. Scroll over the volume meter to adjust it.</p><p>Preferences save automatically.</p><label class="checkbox-setting"><input type="checkbox" bind:checked={closeToTray} onchange={()=>queue({closeToTray})}/>Keep playing in the tray when the main window closes</label></section>
    <section data-settings-panel="appearance" hidden={category!=='Appearance'}><h2>Appearance</h2>
      <label>Station banner layout<select bind:value={layout} onchange={()=>queue({bannerLayout:layout})}><option value="column">Column — logo above name</option><option value="row">Row — logo left of name</option></select></label><p>Applies to every station and its editor preview.</p>
      <label>Accent color<input type="color" bind:value={accent} oninput={event=>{accent=event.currentTarget.value;onpreview(accent);}} onchange={event=>queue({accent:event.currentTarget.value})}/></label>
      <label>Station list spacing<select bind:value={density} onchange={()=>queue({listDensity:density})}><option value="comfortable">Comfortable</option><option value="compact">Compact</option></select></label>
      <label class="checkbox-setting"><input type="checkbox" bind:checked={scanlines} onchange={()=>queue({showScanlines:scanlines})}/>Display scanlines</label>
      <label class="checkbox-setting"><input type="checkbox" bind:checked={reduceMotion} onchange={()=>queue({reduceMotion})}/>Reduce motion (titles and logos)</label>
      <label class="checkbox-setting"><input type="checkbox" bind:checked={showProgress} onchange={()=>queue({showProgress})}/>Show recording progress</label>
      <label class="checkbox-setting"><input type="checkbox" bind:checked={matchBanner} onchange={()=>queue({matchBannerAccent:matchBanner})}/>Match player accent to each station banner</label><p>The accent softly changes when you tune to a station with a banner image or color.</p>
      <label>Long title bounce speed<input type="number" min="10" max="200" step="5" required bind:value={bounceSpeed} onchange={e=>{if(e.currentTarget.reportValidity())queue({bounceSpeed});}}/> pixels per second</label>
    </section>
    <section data-settings-panel="playback" hidden={category!=='Playback'}><h2>Playback</h2>
      <label class="checkbox-setting"><input type="checkbox" bind:checked={randomNext} onchange={()=>queue({randomNext})}/>Choose a random station when pressing Next</label><p>Previous retraces station history; Next retraces forward history before choosing another random station.</p>
      <label>Default crossfade for music stations<input type="number" min="0" max="30000" step="100" required bind:value={fade} onchange={e=>{if(e.currentTarget.reportValidity())queue({crossfade:fade});}}/> ms</label><p>Broadcasts play without a crossfade unless you set an override in Edit station. A value of 0 disables crossfade.</p>
      <h2>Tuning sound</h2><label class="checkbox-setting"><input type="checkbox" bind:checked={tuningTransition} onchange={()=>queue({tuningTransition})}/>Play a random static transition when tuning</label>
      <label>Minimum transition<input type="number" min="0" max="1500" step="1" required bind:value={tuningMinMs} onchange={e=>{if(e.currentTarget.reportValidity())range();}}/> ms</label>
      <label>Maximum transition<input type="number" min="0" max="1500" step="1" required bind:value={tuningMaxMs} onchange={e=>{if(e.currentTarget.reportValidity())range();}}/> ms</label>
      <p>{staticPath==='bundled'?'Bundled static':staticPath?staticPath.split(/[\\/]/).at(-1):'Disabled'}</p><div class="settings-actions"><button onclick={chooseStatic}>Choose audio file</button><button onclick={()=>{staticPath='bundled';queue({staticPath});}}>Use bundled</button><button onclick={()=>{staticPath='';queue({staticPath});}}>Disable</button></div>
    </section>
    <section data-settings-panel="advanced" hidden={category!=='Advanced'}><h2>Files and data</h2><div class="settings-actions"><button onclick={showLibrary}>Show stations.json</button><button onclick={exportLibrary}>Export library</button><button onclick={clear}>Clear metadata cache</button></div><p>Library backups keep the last five saves and seven daily snapshots in the data folder.</p><p>Clearing cached metadata and durations frees their storage. Tracks are measured again as needed; your stations and audio files are kept.</p>
      {#if config.root}<h2>Legacy archive</h2><p>{config.root}</p><button onclick={recover}>Recover archive stations</button>{/if}
    </section>
  </fieldset>
  <ScanNoticesView report={scanReport} ondismiss={()=>scanReport=null}/>
  <p role="status" aria-live="polite">{status}</p>{#if error}<p class="error" role="alert">{error}</p><button onclick={()=>void flush()}>Retry save</button>{/if}
  <div class="settings-exit"><button class="settings-back" onclick={leave}>← Back to stations</button><button disabled={!desktop||saving} onclick={quit}>Quit Radio Replay</button></div>
</section>
