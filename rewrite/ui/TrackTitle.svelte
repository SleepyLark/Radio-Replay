<script lang="ts">
  import { onMount } from 'svelte';
  let {text, speed = 25, reduced = false}: {text: string; speed?: number; reduced?: boolean} = $props();
  let viewport: HTMLDivElement;
  let title: HTMLDivElement;
  let width = $state(0);
  let fontsReady = $state(false);
  let systemReduced = $state(false);
  onMount(() => {
    const measure = () => width = viewport.clientWidth;
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const motion = () => systemReduced = preference.matches;
    preference.addEventListener('change', motion);
    motion();
    void document.fonts.ready.then(()=>{fontsReady=true;measure();});
    return () => { observer.disconnect(); preference.removeEventListener('change', motion); };
  });
  $effect(() => {
    const content = text, available = width, disabled = reduced || systemReduced, rate = speed, ready = fontsReady;
    if (!title || !available || !content || disabled || !ready) return;
    const overflow = Math.ceil(title.scrollWidth - available);
    if (overflow <= 0) return;
    const duration = overflow / Math.max(10, rate) * 1000 + 800;
    const hold = 400 / duration;
    const animation = title.animate([
      {transform:'translateX(0)', offset:0},
      {transform:'translateX(0)', offset:hold, easing:'cubic-bezier(0.15,0,0.85,1)'},
      {transform:`translateX(-${overflow}px)`, offset:1-hold},
      {transform:`translateX(-${overflow}px)`, offset:1}
    ], {duration, iterations:Infinity, direction:'alternate'});
    return () => animation.cancel();
  });
</script>

<div class="title-viewport" bind:this={viewport}><div class="track-title" bind:this={title} aria-live="polite">{text}</div></div>
