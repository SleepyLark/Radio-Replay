<script lang="ts">
  import Dialog from './Dialog.svelte';
  import {displayPath,type Notification} from './notifications';
  let {notification,oncancel,onedit,ondismiss}: {notification:Notification; oncancel:()=>void; onedit?:()=>void; ondismiss:()=>void}=$props();
  let query=$state('');
  let limit=$state(100);
  const filtered=$derived(notification.problems.filter(problem=>`${problem.name} ${problem.path} ${problem.reason}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())));
</script>

<Dialog title={notification.problems.length?'Problematic files':'Notification details'} {oncancel}>
  {#if notification.station}<p class="problem-station">{notification.station}</p>{/if}
  <p class="problem-summary">{notification.message}</p>
  {#if notification.problems.length}
    <label>Find a file<input type="search" bind:value={query} oninput={()=>limit=100} placeholder="Search names, paths or reasons…"/></label>
    <p role="status">{filtered.length} file{filtered.length===1?'':'s'}{query?' matching your search':''}</p>
    <ul class="problem-list">{#each filtered.slice(0,limit) as problem (`${problem.kind}:${problem.path}`)}<li><strong>{problem.name||displayPath(problem.path).split(/[\\/]/).at(-1)}</strong><code>{displayPath(problem.path)}</code><p>{problem.reason}</p>{#if problem.kind==='cache'}<small>Duration cache</small>{/if}</li>{:else}<li>No files match this search.</li>{/each}</ul>
    {#if filtered.length>limit}<button type="button" onclick={()=>limit+=100}>Show more files</button>{/if}
  {/if}
  {#snippet footer()}{#if onedit}<button type="button" onclick={onedit}>Edit station</button>{/if}<button type="button" onclick={ondismiss}>Dismiss notification</button><button type="button" class="primary" onclick={oncancel}>Close</button>{/snippet}
</Dialog>
