<script lang="ts">
  import { untrack } from 'svelte';
  import { desktop,saveSettings,clearMetadataCache,type Config } from './library';
  import { fadeSeconds } from './playback/timeline';
  let {config,onsaved,oncachecleared}:{config:Config;onsaved:(config:Config)=>void;oncachecleared:()=>void}=$props();
  const initial=untrack(()=>config.settings);
  let accent=$state(typeof initial.accent==='string'?initial.accent:'#c7f464');
  let fade=$state(fadeSeconds({key:'',station:'',tracks:[],mode:'tracks'},initial)*1000);
  let randomNext=$state(initial.randomNext===true);let showProgress=$state(initial.showProgress!==false);let reduceMotion=$state(initial.reduceMotion===true);
  let density=$state(initial.listDensity==='compact'?'compact':'comfortable');let busy=$state(false);let status=$state('');let error=$state('');
  async function save(event:SubmitEvent){event.preventDefault();busy=true;status='';error='';try{onsaved(await saveSettings({accent,crossfade:fade,randomNext,showProgress,reduceMotion,listDensity:density}));status='Settings saved.';}catch(e){error=String(e);}finally{busy=false;}}
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
    <div class="actions"><button class="primary">Save settings</button><button class="secondary" type="button" onclick={clear}>Clear metadata cache</button></div>
  </fieldset>
</form>
