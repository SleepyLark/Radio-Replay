<script lang="ts">
  import Dialog from './Dialog.svelte';
  import {libraryRecoveryInfo,recoverLibrary,showConfig,type Config,type RecoveryInfo} from './library';
  let {info,onrecovered,onreload,oncancel}: {info:RecoveryInfo;onrecovered:(config:Config)=>void;onreload:()=>Promise<void>;oncancel:()=>void}=$props();
  let busy=$state(false),error=$state(''),confirmFresh=$state(false);
  async function recover(action:'restoreBackup'|'startFresh') {
    busy=true;error='';
    try{onrecovered(await recoverLibrary(action));}
    catch(e){error=String(e);try{info=await libraryRecoveryInfo();}catch{/* Retain the original recovery error. */}}
    finally{busy=false;}
  }
</script>

<Dialog title="Your library file could not be read" {busy} {oncancel}>
  <p>Your saved library is protected. Restore a readable backup or start a new library. The current file will be kept separately in the data folder.</p>
  <p class="error">{info.error}</p>
  {#if !info.canRestore}<p>No readable backup is available.</p>{/if}
  {#if confirmFresh}<p role="status">Start with an empty library? Your damaged library and existing backups will be kept.</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet footer()}
    <button type="button" disabled={busy} onclick={()=>showConfig().catch(e=>error=String(e))}>Open data folder</button>
    <button type="button" disabled={busy} onclick={onreload}>Reload library</button>
    {#if confirmFresh}
      <button type="button" disabled={busy} onclick={()=>confirmFresh=false}>Cancel reset</button>
      <button type="button" class="danger" disabled={busy||!info.canReset} onclick={()=>recover('startFresh')}>Confirm start fresh</button>
    {:else}
      <button type="button" disabled={busy||!info.canReset} onclick={()=>confirmFresh=true}>Start fresh</button>
      <button type="button" class="primary" disabled={busy||!info.canRestore} onclick={()=>recover('restoreBackup')}>Restore from backup</button>
    {/if}
  {/snippet}
</Dialog>
