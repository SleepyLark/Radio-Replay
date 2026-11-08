<script lang="ts">
  let {name, silent = false}: {name:'play'|'pause'|'previous'|'next'|'speaker'; silent?:boolean}=$props();
</script>

<svg class="control-icon" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" focusable="false">
  {#if name==='pause'}<rect x="6" y="5" width="4" height="14" rx=".5" fill="currentColor"/><rect x="14" y="5" width="4" height="14" rx=".5" fill="currentColor"/>
  {:else if name==='play'}<path d="M8 5v14l11-7Z" fill="currentColor"/>
  {:else if name==='previous'}<path d="M11 6v12l-8-6Zm10 0v12l-8-6Z" fill="currentColor"/>
  {:else if name==='next'}<path d="M3 6v12l8-6Zm10 0v12l8-6Z" fill="currentColor"/>
  {:else}<g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4Z"/>{#if silent}<path d="m16 9 6 6m0-6-6 6"/>{:else}<path d="M15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14"/>{/if}</g>{/if}
</svg>
