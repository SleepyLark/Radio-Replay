<script lang="ts">
  import Dialog from './Dialog.svelte';
  import {deleteStations, type Config, type Station} from './library';
  let {config,onsaved,onedit,onimport,oncancel}: {config:Config; onsaved:(config:Config)=>void; onedit:(station:Station)=>void; onimport:()=>void; oncancel:()=>void}=$props();
  let selected=$state<string[]>([]), confirming=$state(false), busy=$state(false), error=$state('');
  const selection=$derived(new Set(selected));
  function select(key:string){confirming=false;selected=selection.has(key)?selected.filter(k=>k!==key):[...selected,key];}
  async function remove(){if(!confirming){confirming=true;return;}busy=true;error='';try{onsaved(await deleteStations($state.snapshot(selected)));}catch(e){error=String(e);}finally{busy=false;}}
</script>

<Dialog title="Manage stations" {oncancel} {busy} onsubmit={()=>void remove()}>
  <p>Edit a station or select several to remove their entries. Audio files stay where they are.</p>
  <div class="manage-import"><button type="button" disabled={busy} onclick={onimport}>Import library</button><small>Import stations from a stations.json file.</small></div>
  <fieldset disabled={busy}>
    <label><input type="checkbox" checked={selected.length>0&&selected.length===config.custom.length} indeterminate={selected.length>0&&selected.length<config.custom.length} onchange={event=>{selected=event.currentTarget.checked?config.custom.map(s=>s.key):[];confirming=false;}}/> Select all {config.custom.length} stations</label>
    <div class="member-list bulk-list">{#each config.custom as station}<div class="bulk-row"><label><input type="checkbox" checked={selection.has(station.key)} onchange={()=>select(station.key)}/>{station.station}<small>({station.tracks.length} file{station.tracks.length===1?'':'s'})</small></label><button type="button" class="station-edit" onclick={()=>onedit(station)}>Edit</button></div>{:else}<p>No stations yet.</p>{/each}</div>
    <p>{selected.length} selected</p>
    {#if confirming}<p class="error" role="alert">Remove {selected.length} station{selected.length===1?'':'s'}? Audio files will stay on disk.</p>{/if}
  </fieldset>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet footer()}<button type="button" disabled={busy} onclick={oncancel}>Cancel</button><button class="danger" disabled={busy||!selected.length}>{confirming?'Confirm delete':selected.length?`Delete ${selected.length} station${selected.length===1?'':'s'}`:'Delete selected'}</button>{/snippet}
</Dialog>
