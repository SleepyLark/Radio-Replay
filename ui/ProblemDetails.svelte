<script lang="ts">
  import Dialog from './Dialog.svelte';
  import {displayPath,type Notification} from './notifications';
  let {notification,oncancel,onedit,onremove,removableCount=0}: {notification:Notification; oncancel:()=>void; onedit?:()=>void; onremove?:()=>Promise<void>; removableCount?:number}=$props();
  let busy=$state(false),error=$state('');
  async function remove(){if(!onremove)return;busy=true;error='';try{await onremove();}catch(e){error=String(e);}finally{busy=false;}}
  let query=$state('');
  let limit=$state(100);
  const filtered=$derived(notification.problems.filter(problem=>`${problem.name} ${problem.path} ${problem.reason}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())));
</script>

<Dialog title={notification.problems.length?'Problematic files':'Notification details'} {oncancel} {busy}>
  {#if notification.station}<p class="problem-station">{notification.station}</p>{/if}
  <p class="problem-summary">{notification.message}</p>
  {#if notification.problems.length}
    <label>Find a file<input type="search" bind:value={query} oninput={()=>limit=100} placeholder="Search names, paths or reasons…"/></label>
    <p role="status">{filtered.length} file{filtered.length===1?'':'s'}{query?' matching your search':''}</p>
    <ul class="problem-list">{#each filtered.slice(0,limit) as problem (`${problem.kind}:${problem.path}`)}<li><strong>{problem.name||displayPath(problem.path).split(/[\\/]/).at(-1)}</strong><code>{displayPath(problem.path)}</code><p>{problem.reason}</p>{#if problem.kind==='cache'}<small>Duration cache</small>{/if}</li>{:else}<li>No files match this search.</li>{/each}</ul>
    {#if filtered.length>limit}<button type="button" onclick={()=>limit+=100}>Show more files</button>{/if}
  {/if}
  {#if onremove}<p>{removableCount} station file{removableCount===1?'':'s'} can be removed. Files stay on disk.</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#snippet footer()}{#if onremove}<button type="button" class="danger problem-remove" disabled={busy||!removableCount} onclick={remove}>Remove problematic files</button>{/if}{#if onedit}<button type="button" disabled={busy} onclick={onedit}>Edit station</button>{/if}<button type="button" class="primary" disabled={busy} onclick={oncancel}>Close</button>{/snippet}
</Dialog>

<style>.problem-remove{margin-right:auto}</style>
