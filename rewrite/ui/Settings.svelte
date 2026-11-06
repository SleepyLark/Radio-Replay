<script lang="ts">
  import { untrack } from 'svelte';
  import { desktop,saveSettings,clearMetadataCache,type Config } from './library';
  import {chooseAudio} from './library';
  import {recoverLegacyLibrary} from './library';
  import { fadeSeconds } from './playback/timeline';
  let {config,onsaved,oncachecleared}:{config:Config;onsaved:(config:Config)=>void;oncachecleared:()=>void}=$props();
  const initial=untrack(()=>config.settings);
  let accent=$state(typeof initial.accent==='string'?initial.accent:'#c7f464');
  let fade=$state(fadeSeconds({key:'',station:'',tracks:[],mode:'tracks'},initial)*1000);
  let randomNext=$state(initial.randomNext===true);let showProgress=$state(initial.showProgress!==false);let reduceMotion=$state(initial.reduceMotion===true);
  let density=$state(initial.listDensity==='compact'?'compact':'comfortable');let busy=$state(false);let status=$state('');let error=$state('');
  let closeToTray=$state(initial.closeToTray!==false);
  let tuningTransition=$state(initial.tuningTransition!==false);let tuningMinMs=$state(typeof initial.tuningMinMs==='number'?initial.tuningMinMs:0);let tuningMaxMs=$state(typeof initial.tuningMaxMs==='number'?initial.tuningMaxMs:1500);let staticPath=$state(typeof initial.staticPath==='string'?initial.staticPath:'bundled');
  async function chooseStatic(){busy=true;error='';try{const tracks=await chooseAudio(false);if(tracks[0])staticPath=tracks[0].path;}catch(e){error=String(e);}finally{busy=false;}}
  async function recover(){busy=true;error='';try{const before=config.custom.length;const next=await recoverLegacyLibrary();onsaved(next);status=`Recovered ${next.custom.length-before} archive stations.`;}catch(e){error=String(e);}finally{busy=false;}}
  async function save(event:SubmitEvent){event.preventDefault();busy=true;status='';error='';try{onsaved(await saveSettings({accent,crossfade:fade,randomNext,showProgress,reduceMotion,listDensity:density,closeToTray,tuningTransition,tuningMinMs,tuningMaxMs,staticPath}));status='Settings saved.';}catch(e){error=String(e);}finally{busy=false;}}
  async function clear(){busy=true;error='';oncachecleared();try{await clearMetadataCache();status='Metadata and duration cache cleared.';}catch(e){error=String(e);}finally{busy=false;}}
</script>
<section class="heading"><div><h1>Settings</h1><p>Playback and appearance preferences are saved in the preview library.</p></div></section>
<form class="editor" onsubmit={save}>{#if error}<div class="error" role="alert">{error}</div>{/if}{#if status}<p role="status">{status}</p>{/if}
  <fieldset disabled={busy||!desktop}>
    <label>Accent color<input type="color" bind:value={accent}/></label>
    <label>Music crossfade (milliseconds)<input type="number" min="0" max="30000" step="100" bind:value={fade} required/></label>
    <label>Station density<select bind:value={density}><option value="comfortable">Comfortable</option><option value="compact">Compact</option></select></label>
    <label class="check"><input type="checkbox" bind:checked={randomNext}/>Random next station (Previous retraces history)</label>
    <label class="check"><input type="checkbox" bind:checked={showProgress}/>Show recording progress</label>
    <label class="check"><input type="checkbox" bind:checked={reduceMotion}/>Reduce motion</label>
    <label class="check"><input type="checkbox" bind:checked={closeToTray}/>Keep playing in the tray when the main window closes</label>
    <label class="check"><input type="checkbox" bind:checked={tuningTransition}/>Play tuning static between stations</label>
    {#if tuningTransition}<label>Minimum tuning delay (milliseconds)<input type="number" min="0" max="1500" step="1" required bind:value={tuningMinMs}/></label><label>Maximum tuning delay (milliseconds)<input type="number" min="0" max="1500" step="1" required bind:value={tuningMaxMs}/></label><p>{staticPath==='bundled'?'Bundled tuning static':staticPath||'No tuning sound'}</p><div class="actions"><button class="secondary" type="button" onclick={chooseStatic}>Choose tuning sound</button><button type="button" onclick={()=>staticPath='bundled'}>Use bundled static</button></div>{/if}
    <div class="actions"><button class="primary">Save settings</button><button class="secondary" type="button" onclick={clear}>Clear metadata cache</button></div>
    {#if config.root}<h2>Legacy archive</h2><p>{config.root}</p><p>Recover missing station definitions from an older archive layout. Existing stations and source files stay in place.</p><button class="secondary" type="button" onclick={recover}>Recover archive stations</button>{/if}
  </fieldset>
</form>
