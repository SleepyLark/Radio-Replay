<script lang="ts">
  import type {Notification} from './notifications';
  let {items,raised=false,onview,ondismiss}: {items:Notification[]; raised?:boolean; onview:(item:Notification)=>void; ondismiss:(id:number)=>void}=$props();
</script>

<section class="toast-region" class:above-player-bar={raised} aria-label="Notifications">
  {#each items as item (item.id)}<article class="toast" class:toast-error={item.severity==='error'} role={item.severity==='error'?'alert':'status'} aria-atomic="true">
    <div class="toast-heading"><strong>{item.problems.length?'Files need attention':'Radio Replay'}</strong><button type="button" class="toast-dismiss" aria-label="Dismiss notification" onclick={()=>ondismiss(item.id)}><svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg></button></div>
    <p>{item.message}</p>
    <button type="button" class="toast-details" onclick={()=>onview(item)}>{item.problems.length?`View files (${item.problems.length})`:'View details'}</button>
  </article>{/each}
</section>
