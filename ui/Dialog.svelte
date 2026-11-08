<script lang="ts">
  import type {Snippet} from 'svelte';
  let {title, children, footer, oncancel, onsubmit, busy = false}: {title: string; children: Snippet; footer: Snippet; oncancel: () => void; onsubmit?: (event: SubmitEvent) => void; busy?: boolean} = $props();
  function open(node: HTMLDialogElement) {
    node.showModal();
    return {destroy() {node.close();}};
  }
</script>

<dialog id="editor" class="editor" use:open oncancel={event=>{if(busy)event.preventDefault();}} onclose={oncancel} aria-label={title}>
  <form onsubmit={event=>{event.preventDefault();onsubmit?.(event);}}>
    <h2>{title}</h2>
    <div class="editor-body">{@render children()}</div>
    <menu>{@render footer()}</menu>
  </form>
</dialog>
