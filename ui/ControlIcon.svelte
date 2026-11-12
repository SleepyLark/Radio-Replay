<script lang="ts">
  let {name, silent = false}: {name:'play'|'pause'|'previous'|'next'|'speaker'|'pip'|'pip-exit'; silent?:boolean}=$props();
</script>

<svg class="control-icon" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" focusable="false">
  {#if name==='pause'}<rect x="6" y="5" width="4" height="14" rx=".5" fill="currentColor"/><rect x="14" y="5" width="4" height="14" rx=".5" fill="currentColor"/>
  {:else if name==='play'}<path d="M8 5v14l11-7Z" fill="currentColor"/>
  {:else if name==='previous'}<path d="M11 6v12l-8-6Zm10 0v12l-8-6Z" fill="currentColor"/>
  {:else if name==='next'}<path d="M3 6v12l8-6Zm10 0v12l8-6Z" fill="currentColor"/>
  {:else if name==='pip'}<path d="M2,11V9H5.6L1.3,4.7L2.7,3.3L7,7.6V4H9v7H2Zm2,9q-0.82,0-1.41-0.59T2,18V13H4v5h8v2H4Zm16-7V6H11V4h9q0.83,0 1.41,0.59Q22,5.18 22,6v7H20Zm-6,7V15h8v5H14Z" fill="currentColor"/>
  {:else if name==='pip-exit'}<path d="M4,20q-0.82,0-1.41-0.59T2,18V11H4v7H20V6H11V4h9q0.83,0 1.41,0.59Q22,5.18 22,6V18q0,0.82-0.59,1.41T20,20H4Zm13.08-3.5l1.43-1.43L15.4,12H18V10H12v6h2V13.43l3.07,3.08ZM2,9V4H9V9H2Zm10,3Z" fill="currentColor"/>
  {:else}<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4Z"/>{#if silent}<path d="m16 9 6 6m0-6-6 6"/>{:else}<path d="M15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14"/>{/if}</g>{/if}
</svg>
